use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{derived_split, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

impl Store {
    pub(crate) fn summary_head(&self, segment: &str) -> Result<Option<(String, i64)>> {
        Ok(self.db.query_row("SELECT id,through_sequence FROM context_summaries WHERE segment_id=?1 AND status='committed' ORDER BY through_sequence DESC LIMIT 1",[segment],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
    }

    pub(crate) fn require_compaction(&self, run: &str, summary: &str) -> Result<CompactionPlan> {
        let plan: Option<CompactionPlan> = self.db.query_row("SELECT s.segment_id,s.after_sequence,s.through_sequence FROM context_summaries s JOIN context_segments c ON c.id=s.segment_id WHERE s.id=?1 AND c.run_id=?2",params![summary,run],|r|Ok(CompactionPlan{id:summary.into(),segment_id:r.get(0)?,after:r.get::<_,i64>(1)?.to_string(),through:r.get::<_,i64>(2)?.to_string()})).optional()?;
        let plan = plan.ok_or_else(|| Error::denied("compaction does not belong to this run"))?;
        let latest: String = self.db.query_row(
            "SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1",
            [run],
            |r| r.get(0),
        )?;
        let grant = self.run_grant(run)?;
        if latest != plan.segment_id
            || !self.context_available(&grant, &plan.segment_id)?
            || !self.context_is_current(&plan.segment_id)?
        {
            return Err(Error::conflict(
                "compaction input no longer eligible; authorize a clean continuation",
            ));
        }
        Ok(plan)
    }

    pub fn prepare_compaction(&self, run: &str) -> Result<CompactionPreparation> {
        let tx = self.write_transaction()?;
        let grant = self.require_active(run)?;
        let segment: Option<String> = tx
            .query_row(
                "SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1",
                [run],
                |r| r.get(0),
            )
            .optional()?;
        let Some(segment) = segment else {
            return Ok(CompactionPreparation { plan: None });
        };
        if !self.context_available(&grant, &segment)? || !self.context_is_current(&segment)? {
            return Err(Error::conflict(
                "compaction input no longer eligible; authorize a clean continuation",
            ));
        }
        let head = self.summary_head(&segment)?;
        let after = head.as_ref().map_or(0, |(_, through)| *through);
        let target = self.context_tail(&segment)?;
        let mut through = after;
        let mut bytes = 0;
        let mut query=tx.prepare("SELECT sequence,body FROM context_items WHERE segment_id=?1 AND sequence>?2 AND sequence<=?3 AND NOT(sequence=1 AND kind='owner_instruction') ORDER BY sequence LIMIT 20")?;
        for row in query.query_map(params![segment, after, target], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (sequence, body) = row?;
            let body: Value = serde_json::from_str(
                &body.ok_or_else(|| Error::denied("compaction input deleted"))?,
            )?;
            let size = serde_json::to_vec(&visible_message(&body)?)?.len();
            if through > after && bytes + size > 180_000 {
                break;
            }
            bytes += size;
            through = sequence;
        }
        drop(query);
        if through == after {
            return Ok(CompactionPreparation { plan: None });
        }
        let existing:Option<String>=tx.query_row("SELECT id FROM context_summaries WHERE segment_id=?1 AND after_sequence=?2 AND through_sequence=?3 AND status='prepared' ORDER BY rowid LIMIT 1",params![segment,after,through],|r|r.get(0)).optional()?;
        let summary = existing.unwrap_or_else(id);
        tx.execute("INSERT OR IGNORE INTO context_summaries(id,segment_id,after_sequence,through_sequence,predecessor_id,created_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![summary,segment,after,through,head.map(|(id,_)|id),now_ms().to_string()])?;
        tx.commit()?;
        Ok(CompactionPreparation {
            plan: Some(CompactionPlan {
                id: summary,
                segment_id: segment,
                after: after.to_string(),
                through: through.to_string(),
            }),
        })
    }

    pub fn read_compaction(&self, run: &str, summary: &str) -> Result<CompactionInput> {
        let tx = self.db.unchecked_transaction()?;
        self.require_active(run)?;
        let plan = self.require_compaction(run, summary)?;
        let previous: Option<String> = tx.query_row(
            "SELECT predecessor_id FROM context_summaries WHERE id=?1",
            [summary],
            |r| r.get(0),
        )?;
        let previous_summary = previous.map(|id| self.read_summary(run, &id)).transpose()?;
        let request: String =
            tx.query_row("SELECT request FROM runs WHERE id=?1", [run], |r| r.get(0))?;
        let request: AgentRunRequest = serde_json::from_str(&request)?;
        let mut messages = Vec::new();
        for row in tx.prepare("SELECT body FROM context_items WHERE segment_id=?1 AND sequence>CAST(?2 AS INTEGER) AND sequence<=CAST(?3 AS INTEGER) AND NOT(sequence=1 AND kind='owner_instruction') ORDER BY sequence")?.query_map(params![plan.segment_id,plan.after,plan.through],|r|r.get::<_,Option<String>>(0))? {
            let message:Value=serde_json::from_str(&row?.ok_or_else(||Error::denied("compaction input deleted"))?)?;
            messages.push(visible_message(&message)?.as_object().unwrap().clone());
        }
        let input = CompactionInput {
            plan,
            owner_task: request.prompt,
            previous_summary,
            messages,
        };
        if serde_json::to_vec(&input)?.len() > 480 * 1024 {
            return Err(Error::exhausted(
                "compaction input exceeds the bounded transport window",
            ));
        }
        Ok(input)
    }

    pub fn read_summary(&self, run: &str, summary: &str) -> Result<ContextSummary> {
        self.require_compaction(run, summary)?;
        let grant = self.run_grant(run)?;
        let row:Option<(String,i64,String)>=self.db.query_row("SELECT s.segment_id,s.through_sequence,s.body FROM context_summaries s JOIN context_segments c ON c.id=s.segment_id WHERE s.id=?1 AND c.run_id=?2 AND s.status='committed'",params![summary,run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (segment, through, text) =
            row.ok_or_else(|| Error::denied("summary is absent or unavailable to this run"))?;
        self.require_source(&grant, "event", summary)?;
        let mut sources = Vec::new();
        for row in self.db.prepare("SELECT kind,id,version FROM context_sources WHERE segment_id=?1 AND first_sequence<=?2 ORDER BY kind,id,version")?.query_map(params![segment,through],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))? {
            let (kind,id,version)=row?;
            sources.push(ContextSource{kind:serde_json::from_value(Value::String(kind))?,id,version});
        }
        Ok(ContextSummary {
            id: summary.into(),
            through: through.to_string(),
            text,
            sources,
        })
    }

    pub fn commit_compaction(
        &self,
        run: &str,
        request: &CompactionCommit,
    ) -> Result<ContextSummary> {
        validate("CompactionCommit", &serde_json::to_value(request)?)?;
        if request.text.len() > 16 * 1024 {
            return Err(Error::exhausted("summary exceeds 16 KiB"));
        }
        let tx = self.write_transaction()?;
        let plan = self.require_compaction(run, &request.id)?;
        let (status, body, permit, predecessor): (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT status,body,permit_id,predecessor_id FROM context_summaries WHERE id=?1",
            [&request.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        if status == "committed" {
            if body.as_deref() != Some(&request.text)
                || permit.as_deref() != Some(&request.permit_id)
            {
                return Err(Error::conflict(
                    "summary already committed with another result",
                ));
            }
            return self.read_summary(run, &request.id);
        }
        if status != "prepared"
            || self.summary_head(&plan.segment_id)?.map(|(id, _)| id) != predecessor
        {
            return Err(Error::conflict("compaction predecessor changed"));
        }
        let usage: Option<Option<String>> = tx
            .query_row(
                "SELECT usage FROM permits WHERE id=?1 AND run_id=?2 AND compaction_id=?3",
                params![request.permit_id, run, request.id],
                |r| r.get(0),
            )
            .optional()?;
        let usage: Usage = serde_json::from_str(&usage.flatten().ok_or_else(|| {
            Error::denied("compaction requires settled usage from its own model permit")
        })?)?;
        if !usage.complete {
            return Err(Error::denied(
                "incomplete model usage cannot establish a compaction result",
            ));
        }
        let mut references=tx.prepare("SELECT DISTINCT id FROM context_sources WHERE segment_id=?1 AND first_sequence<=CAST(?2 AS INTEGER) ORDER BY id")?.query_map(params![plan.segment_id,plan.through],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if let Some(previous) = &predecessor {
            references.push(previous.clone());
        }
        let grant = self.run_grant(run)?;
        let activity_sequence:i64=tx.query_row("SELECT coalesce(max(CAST(sequence AS INTEGER)),0)+1 FROM events WHERE run_id=?1 AND producer='pi-compactor'",[run],|r|r.get(0))?;
        self.ingest(&Event{id:request.id.clone(),scope:grant.scope,run_id:run.into(),producer:"pi-compactor".into(),sequence:activity_sequence.to_string(),kind:"context_summary".into(),timestamp_ms:now_ms().to_string(),parents:predecessor.into_iter().collect(),correlation:plan.segment_id.clone(),artifacts:vec![],payload:json!({"role":"assistant","summary":request.text,"after":plan.after,"through":plan.through,"permit_id":request.permit_id,"authority":"Pi-authored interpretation; verify effects through host receipts"}).as_object().unwrap().clone(),provenance:Provenance{origin:Origin::Observed,source_refs:references,scenario_family:"context-compaction".into(),split:derived_split(&grant.visible_splits),limitations:vec!["Semantic continuation summary, not an independent validation of task truth.".into()]}})?;
        tx.execute(
            "UPDATE context_summaries SET status='committed',body=?2,permit_id=?3 WHERE id=?1",
            params![request.id, request.text, request.permit_id],
        )?;
        let summary = self.read_summary(run, &request.id)?;
        tx.commit()?;
        Ok(summary)
    }
}

fn visible_message(message: &Value) -> Result<Value> {
    let parts = crate::context::visible_content(message)?;
    Ok(
        json!({"role":message["role"],"timestamp":message["timestamp"],"content":parts,"toolName":message.get("toolName"),"toolCallId":message.get("toolCallId"),"isError":message.get("isError")}),
    )
}
