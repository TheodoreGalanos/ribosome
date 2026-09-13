use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

const ITEM_BYTES: usize = 240 * 1024;
const PAGE_BYTES: usize = 480 * 1024;

impl Store {
    fn context_count(&self, segment: &str) -> Result<i64> {
        Ok(self.db.query_row(
            "SELECT coalesce(max(sequence),0) FROM context_items WHERE segment_id=?1",
            [segment],
            |r| r.get(0),
        )?)
    }

    fn context_state(&self, grant: &Grant, segment: String, rebuilt: bool) -> Result<ContextState> {
        let generation: i64 = self
            .db
            .query_row(
                "SELECT generation FROM source_policy WHERE client=?1 AND project=?2",
                params![grant.scope.client, grant.scope.project],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let summary = self.summary_head(&segment)?;
        Ok(ContextState {
            count: self.context_count(&segment)?.to_string(),
            tail_after: self.context_tail(&segment)?.to_string(),
            segment_id: segment,
            generation: generation.to_string(),
            rebuilt,
            summary_ref: summary.as_ref().map(|(id, _)| id.clone()),
            summary_through: Some(summary.map_or(0, |(_, through)| through).to_string()),
        })
    }

    pub(crate) fn context_tail(&self, segment: &str) -> Result<i64> {
        let owner_bytes: i64 = self.db.query_row("SELECT coalesce(sum(length(CAST(body AS BLOB))),0) FROM context_items WHERE segment_id=?1 AND sequence=1 AND kind='owner_instruction'",[segment],|r|r.get(0))?;
        // Leave capacity for the owner request, the bounded summary and framing.
        let tail_bytes = 180_000usize.saturating_sub(owner_bytes as usize + 16 * 1024 + 2000);
        let mut bytes = 0;
        let mut boundary = None;
        let mut query = self.db.prepare("SELECT sequence,kind,coalesce(length(CAST(CASE WHEN kind='observed_evidence' THEN json_remove(body,'$.details') ELSE body END AS BLOB)),0) FROM context_items WHERE segment_id=?1 ORDER BY sequence DESC")?;
        for row in query.query_map([segment], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (sequence, kind, size) = row?;
            bytes += size as usize;
            if bytes > tail_bytes
                && let Some(boundary) = boundary
            {
                return Ok(boundary);
            }
            if kind != "observed_evidence" {
                boundary = Some(sequence - 1);
            }
        }
        Ok(0)
    }

    fn require_context_owner(&self, run: &str, segment: &str) -> Result<()> {
        let owned: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM context_segments WHERE id=?1 AND run_id=?2)",
            params![segment, run],
            |r| r.get(0),
        )?;
        if !owned {
            return Err(Error::denied("context belongs to another run"));
        }
        Ok(())
    }

    pub(crate) fn inherit_context_sources(
        &self,
        run: &str,
        submission: &mut RecordSubmission,
    ) -> Result<()> {
        submission
            .provenance
            .source_refs
            .extend(self.run_context_sources(run)?);
        submission.provenance.source_refs.sort();
        submission.provenance.source_refs.dedup();
        Ok(())
    }

    pub(crate) fn run_context_sources(&self, run: &str) -> Result<Vec<String>> {
        let mut references = self.db.prepare("SELECT DISTINCT id FROM context_sources WHERE segment_id=(SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1) ORDER BY id")?.query_map([run], |r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let summary:Option<String>=self.db.query_row("SELECT s.id FROM context_summaries s WHERE s.segment_id=(SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1) AND s.status='committed' ORDER BY through_sequence DESC LIMIT 1",[run],|r|r.get(0)).optional()?;
        references.extend(summary);
        if let Some(invocation) = self.run_invocation(run)? {
            references.push(invocation.implementation.id);
            references.extend(invocation.recipient_refs);
        }
        references.sort();
        references.dedup();
        Ok(references)
    }

    pub(crate) fn context_available(&self, grant: &Grant, segment: &str) -> Result<bool> {
        let source_format: i64 = self.db.query_row(
            "SELECT source_format FROM context_segments WHERE id=?1",
            [segment],
            |r| r.get(0),
        )?;
        if source_format != 4 {
            return Ok(false);
        }
        let mut query = self
            .db
            .prepare("SELECT DISTINCT kind,id FROM context_sources WHERE segment_id=?1")?;
        let sources = query
            .query_map([segment], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        match self.require_sources(grant, sources) {
            Ok(()) => Ok(true),
            Err(error) if matches!(error.code, -32001 | -32004) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Called by the trusted adapter before every provider call. All prior
    /// evidence in this segment is conservatively inherited by later messages.
    /// A withdrawn ancestor starts a new segment without reading its payload.
    pub fn authorize_context(&self, run: &str) -> Result<ContextState> {
        let tx = self.write_transaction()?;
        let grant = self.require_active(run)?;
        let work_source = self.work_source(&grant, run)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1",
                [run],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(segment) = &previous
            && self.context_available(&grant, segment)?
            && self.context_is_current(segment)?
        {
            let state = self.context_state(&grant, segment.clone(), false)?;
            tx.commit()?;
            return Ok(state);
        }
        let legacy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM checkpoints WHERE run_id=?1)",
            [run],
            |r| r.get(0),
        )?;
        let rebuilt = previous.is_some() || legacy;
        let segment = id();
        let reason = if previous.is_some() {
            "source unavailable, required artifact revised, or legacy source lineage incomplete"
        } else if legacy {
            "legacy continuation lacks verified lineage"
        } else {
            "initial context"
        };
        tx.execute(
            "INSERT INTO context_segments(id,run_id,predecessor,reason,source_format) VALUES(?1,?2,?3,?4,4)",
            params![segment, run, previous, reason],
        )?;
        if let Some(source) = work_source {
            tx.execute("INSERT INTO context_sources(segment_id,first_sequence,kind,id,version) VALUES(?1,1,'event',?2,?3)", params![segment,source.id,source.version])?;
        }
        if rebuilt {
            let request: String =
                tx.query_row("SELECT request FROM runs WHERE id=?1", [run], |r| r.get(0))?;
            let request: AgentRunRequest = serde_json::from_str(&request)?;
            let references = [
                ContinuationKind::Obligation,
                ContinuationKind::Effect,
                ContinuationKind::Work,
            ]
            .into_iter()
            .map(|kind| {
                self.continuation(
                    run,
                    &ContinuationRead {
                        kind,
                        after: "0".into(),
                        limit: 20,
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
            let text = format!(
                "{}\n\nThe host started a clean continuation segment: {reason}. Previous observations and interpretations have been withheld. Reinspect permitted current evidence. Continue the owner task and inspect these references using record_read, action_lookup or work_status. They do not establish completion or current validity. For any incomplete page, call continuation_read with its kind and next as after; even an empty page can advance. evidence_cursor is the last checkpointed evidence-read position, not proof of complete investigation; repeat scoped evidence reads when their filters or required sources change. References: {}",
                request.prompt,
                serde_json::to_string(&references)?
            );
            let message = json!({"role":"user","content":text,"timestamp":now_ms()});
            self.insert_context_message(run, &grant, &segment, 1, &message)?;
        }
        let state = self.context_state(&grant, segment, rebuilt)?;
        tx.commit()?;
        Ok(state)
    }

    pub fn append_context(&self, run: &str, batch: &ContextAppend) -> Result<ContextState> {
        validate("ContextAppend", &serde_json::to_value(batch)?)?;
        if serde_json::to_vec(batch)?.len() > PAGE_BYTES {
            return Err(Error::exhausted("context batch exceeds 480 KiB"));
        }
        let grant = self.run_grant(run)?;
        let tx = self.write_transaction()?;
        self.require_context_owner(run, &batch.segment_id)?;
        let latest: String = tx.query_row(
            "SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1",
            [run],
            |r| r.get(0),
        )?;
        if latest != batch.segment_id {
            return Err(Error::conflict(
                "cannot append to a superseded context segment",
            ));
        }
        let after = cursor(&batch.after)?;
        let count = self.context_count(&batch.segment_id)?;
        if after > count {
            return Err(Error::conflict("context append has a sequence gap"));
        }
        for (offset, entry) in batch.entries.iter().enumerate() {
            let sequence = after + offset as i64 + 1;
            let body = Value::Object(entry.message.clone());
            if sequence <= count {
                let previous: Option<String> = tx.query_row(
                    "SELECT body FROM context_items WHERE segment_id=?1 AND sequence=?2",
                    params![batch.segment_id, sequence],
                    |r| r.get(0),
                )?;
                if previous.as_deref() != Some(serde_json::to_string(&body)?.as_str()) {
                    return Err(Error::conflict(
                        "context sequence reused with different content",
                    ));
                }
                // Retried payloads cannot change the host's existing lineage.
                continue;
            }
            for source in &entry.sources {
                let kind = match source.kind {
                    ContextSourceKind::Record => "record",
                    ContextSourceKind::Event => "event",
                    ContextSourceKind::Artifact => "artifact",
                };
                // A source may have been withdrawn since a tool returned. Keep
                // that observation with its lineage; authorization will rebuild.
                tx.execute("INSERT OR IGNORE INTO context_sources(segment_id,first_sequence,kind,id,version) VALUES(?1,?2,?3,?4,?5)", params![batch.segment_id,sequence,kind,source.id,source.version])?;
            }
            let mut needs_evidence_anchor = false;
            for source in entry
                .sources
                .iter()
                .filter(|source| source.kind == ContextSourceKind::Record)
            {
                let mut captured = false;
                for artifact in entry
                    .sources
                    .iter()
                    .filter(|source| source.kind == ContextSourceKind::Artifact)
                {
                    captured |= tx.query_row("SELECT EXISTS(SELECT 1 FROM source_edges WHERE subject_kind='artifact' AND subject_id=?1 AND source_kind='record' AND source_id=?2 AND source_version=?3)",params![artifact.id,source.id,source.version],|row|row.get::<_,bool>(0))?;
                }
                if !captured {
                    let current: Option<String> = tx
                        .query_row(
                            "SELECT version FROM records WHERE id=?1",
                            [&source.id],
                            |row| row.get(0),
                        )
                        .optional()?;
                    let withdrawn:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_tombstones WHERE kind='record' AND id=?1)",[&source.id],|row|row.get(0))?;
                    if current.as_deref() != Some(source.version.as_str()) && !withdrawn {
                        return Err(Error::conflict(
                            "record revision lacks a retained observation; rebuild from fresh evidence",
                        ));
                    }
                    needs_evidence_anchor = true;
                }
            }
            let activity =
                self.insert_context_message(run, &grant, &batch.segment_id, sequence, &body)?;
            if needs_evidence_anchor {
                // Direct adapter observations need the same immutable ancestry
                // as wrapped tool results. The saved activity already owns it.
                tx.execute("INSERT INTO context_sources(segment_id,first_sequence,kind,id,version) VALUES(?1,?2,'event',?3,?4)",params![batch.segment_id,sequence,activity.id,activity.version])?;
            }
        }
        let state = self.context_state(&grant, batch.segment_id.clone(), false)?;
        tx.commit()?;
        Ok(state)
    }

    fn insert_context_message(
        &self,
        run: &str,
        grant: &Grant,
        segment: &str,
        sequence: i64,
        message: &Value,
    ) -> Result<ContextSource> {
        let body = serde_json::to_string(message)?;
        if body.len() > ITEM_BYTES {
            return Err(Error::exhausted(
                "context item exceeds 240 KiB; use bounded evidence or artifact excerpts",
            ));
        }
        let role = message["role"]
            .as_str()
            .ok_or_else(|| Error::invalid("context message role required"))?;
        let (kind, activity_role) = match role {
            "user" => ("owner_instruction", AgentActivityRole::User),
            "assistant" => ("interpretation", AgentActivityRole::Assistant),
            "toolResult" => ("observed_evidence", AgentActivityRole::ToolResult),
            _ => return Err(Error::invalid("unsupported context message role")),
        };
        let timestamp = message["timestamp"]
            .as_u64()
            .ok_or_else(|| Error::invalid("context message timestamp required"))?;
        self.db.execute("INSERT INTO context_items(segment_id,sequence,previous_sequence,kind,body) VALUES(?1,?2,?3,?4,?5)", params![segment,sequence,sequence.checked_sub(1).filter(|n| *n>0),kind,body])?;
        let mut sources = self.db.prepare("SELECT DISTINCT id FROM context_sources WHERE segment_id=?1 AND first_sequence<=?2 ORDER BY id")?.query_map(params![segment,sequence], |r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let summary = self.summary_head(segment)?.map(|(id, _)| id);
        self.db.execute(
            "UPDATE context_items SET summary_ref=?3 WHERE segment_id=?1 AND sequence=?2",
            params![segment, sequence, summary],
        )?;
        sources.extend(summary);
        let activity_sequence: i64 = self.db.query_row("SELECT coalesce(max(CAST(sequence AS INTEGER)),0)+1 FROM events WHERE run_id=?1 AND producer='pi-agent' AND client=?2 AND project=?3", params![run,grant.scope.client,grant.scope.project], |r| r.get(0))?;
        let parts = visible_content(message)?;
        self.append_agent_activity(
            run,
            &AgentActivityBatch {
                entries: vec![AgentActivity {
                    sequence: activity_sequence.to_string(),
                    role: activity_role,
                    timestamp_ms: timestamp.to_string(),
                    source_refs: sources,
                    content: json!({"parts":parts}).as_object().unwrap().clone(),
                }],
            },
        )?;
        let event:String=self.db.query_row("SELECT id FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND producer='pi-agent' AND sequence=?4",params![grant.scope.client,grant.scope.project,run,activity_sequence.to_string()],|row|row.get(0))?;
        Ok(ContextSource {
            kind: ContextSourceKind::Event,
            id: event,
            version: activity_sequence.to_string(),
        })
    }

    pub fn read_context(&self, run: &str, request: &ContextRead) -> Result<ContextPage> {
        validate("ContextRead", &serde_json::to_value(request)?)?;
        let tx = self.db.unchecked_transaction()?;
        let grant = self.require_active(run)?;
        self.require_context_owner(run, &request.segment_id)?;
        if !self.context_available(&grant, &request.segment_id)?
            || !self.context_is_current(&request.segment_id)?
        {
            return Err(Error::denied(
                "context source withdrawn; authorize a clean segment",
            ));
        }
        let mut next = cursor(&request.after)?;
        let mut messages = Vec::new();
        let mut bytes = 0;
        let mut query = tx.prepare("SELECT sequence,body FROM context_items WHERE segment_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        for row in query.query_map(params![request.segment_id, next, request.limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (sequence, body) = row?;
            let body = body.ok_or_else(|| {
                Error::denied("context content deleted; authorize a clean segment")
            })?;
            if bytes + body.len() > PAGE_BYTES {
                break;
            }
            bytes += body.len();
            next = sequence;
            messages.push(serde_json::from_str(&body)?);
        }
        Ok(ContextPage {
            messages,
            next: next.to_string(),
            complete: next >= self.context_count(&request.segment_id)?,
        })
    }

    pub(crate) fn check_context_checkpoint(
        &self,
        run: &str,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        if checkpoint.format == "pi-0.85.1/1" {
            return Ok(());
        }
        if !checkpoint.messages.is_empty() {
            return Err(Error::invalid(
                "version 2 continuation references retained messages; embedded messages forbidden",
            ));
        }
        let context = checkpoint.context.as_ref().ok_or_else(|| {
            Error::invalid("version 2 continuation requires a context descriptor")
        })?;
        self.require_context_owner(run, &context.segment_id)?;
        if cursor(&context.count)? != self.context_count(&context.segment_id)? {
            return Err(Error::conflict(
                "continuation does not match retained context frontier",
            ));
        }
        if serde_json::to_vec(checkpoint)?.len() > 256 * 1024 {
            return Err(Error::exhausted("continuation descriptor exceeds 256 KiB"));
        }
        Ok(())
    }

    pub(crate) fn inspect_context_messages(
        &self,
        run: &str,
    ) -> Result<(usize, Vec<serde_json::Map<String, Value>>)> {
        let grant = self.run_grant(run)?;
        let segments = self
            .db
            .prepare("SELECT id FROM context_segments WHERE run_id=?1 ORDER BY rowid")?
            .query_map([run], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut count = 0;
        let mut messages = Vec::new();
        for segment in segments {
            count += self.context_count(&segment)? as usize;
            if self.context_available(&grant, &segment)? {
                for row in self.db.prepare("SELECT body FROM context_items WHERE segment_id=?1 AND body IS NOT NULL ORDER BY sequence")?.query_map([segment], |r| r.get::<_,String>(0))? {
                    messages.push(serde_json::from_str(&row?)?);
                }
            }
        }
        Ok((count, messages))
    }

    pub(crate) fn redact_context_source(
        &self,
        grant: &Grant,
        kind: &str,
        source: &str,
    ) -> Result<()> {
        // Legacy copies have no trustworthy source set. Delete their content
        // conservatively within the affected scope, preserving operation IDs
        // and protected retention. Migration itself preserves observations.
        self.db.execute("UPDATE checkpoints SET body=json_set(body,'$.messages',json('[]')) WHERE json_extract(body,'$.format')<>'pi-0.85.1/2' AND json_array_length(body,'$.messages')>0 AND run_id IN (SELECT r.id FROM runs r JOIN grants g ON g.id=r.grant_id WHERE json_extract(g.body,'$.scope.client')=?1 AND json_extract(g.body,'$.scope.project')=?2 AND NOT EXISTS(SELECT 1 FROM json_each(g.body,'$.visible_splits') WHERE value<>'development'))",params![grant.scope.client,grant.scope.project])?;
        self.db.execute("UPDATE messages SET body='{}',acknowledged=1 WHERE source_format=0 AND body<>'{}' AND grant_id IN (SELECT id FROM grants g WHERE json_extract(g.body,'$.scope.client')=?1 AND json_extract(g.body,'$.scope.project')=?2 AND NOT EXISTS(SELECT 1 FROM json_each(g.body,'$.visible_splits') WHERE value<>'development'))",params![grant.scope.client,grant.scope.project])?;
        self.db.execute("UPDATE attachment_feedback SET body=json_set(body,'$.summary','','$.detail','Unverified legacy feedback removed after source deletion.','$.state','expired') WHERE json_extract(body,'$.run_id') IN (SELECT id FROM runs WHERE result_source IS NULL AND grant_id IN (SELECT id FROM grants g WHERE json_extract(g.body,'$.scope.client')=?1 AND json_extract(g.body,'$.scope.project')=?2 AND NOT EXISTS(SELECT 1 FROM json_each(g.body,'$.visible_splits') WHERE value<>'development')))",params![grant.scope.client,grant.scope.project])?;
        self.db.execute("UPDATE runs SET result=NULL WHERE result_source IS NULL AND result IS NOT NULL AND grant_id IN (SELECT id FROM grants g WHERE json_extract(g.body,'$.scope.client')=?1 AND json_extract(g.body,'$.scope.project')=?2 AND NOT EXISTS(SELECT 1 FROM json_each(g.body,'$.visible_splits') WHERE value<>'development'))",params![grant.scope.client,grant.scope.project])?;
        self.db.execute("UPDATE artifact_snapshots SET result_content=NULL WHERE result_content IS NOT NULL AND available=0 AND result_method IN ('message.send','message.inbox') AND client=?1 AND project=?2 AND split='development'",params![grant.scope.client,grant.scope.project])?;
        // The durable cleanup traversal visits each source and derivative.
        // Redact the copied continuation from its first direct observation.
        self.db.execute("WITH boundaries AS (SELECT s.segment_id,min(s.first_sequence) AS first FROM context_sources s JOIN context_segments c ON c.id=s.segment_id JOIN runs r ON r.id=c.run_id JOIN grants g ON g.id=r.grant_id WHERE s.kind=?4 AND s.id=?1 AND json_extract(g.body,'$.scope.client')=?2 AND json_extract(g.body,'$.scope.project')=?3 GROUP BY s.segment_id) UPDATE context_items SET body=NULL WHERE EXISTS(SELECT 1 FROM boundaries b WHERE b.segment_id=context_items.segment_id AND context_items.sequence>=b.first)", params![source,grant.scope.client,grant.scope.project,kind])?;
        self.db.execute("WITH boundaries AS (SELECT s.segment_id,min(s.first_sequence) AS first FROM context_sources s JOIN context_segments c ON c.id=s.segment_id JOIN runs r ON r.id=c.run_id JOIN grants g ON g.id=r.grant_id WHERE s.kind=?4 AND s.id=?1 AND json_extract(g.body,'$.scope.client')=?2 AND json_extract(g.body,'$.scope.project')=?3 GROUP BY s.segment_id) UPDATE context_summaries SET body=NULL,status='redacted' WHERE EXISTS(SELECT 1 FROM boundaries b WHERE b.segment_id=context_summaries.segment_id AND context_summaries.through_sequence>=b.first)", params![source,grant.scope.client,grant.scope.project,kind])?;
        Ok(())
    }
}

fn cursor(value: &str) -> Result<i64> {
    i64::try_from(counter(value)?)
        .map_err(|_| Error::invalid("context cursor exceeds SQLite integer capacity"))
}

pub(crate) fn visible_content(message: &Value) -> Result<Vec<Value>> {
    Ok(if let Some(text) = message["content"].as_str() {
        vec![json!({"type":"text","text":text})]
    } else {
        message["content"].as_array().ok_or_else(|| Error::invalid("context message content required"))?.iter().filter_map(|part| match part["type"].as_str() {
            Some("thinking") => None,
            Some("text") => Some(json!({"type":"text","text":part["text"]})),
            Some("toolCall") => Some(json!({"type":"toolCall","id":part["id"],"name":part["name"],"arguments":part["arguments"]})),
            _ => Some(json!({"type":part["type"],"omitted":true,"reason":"Non-text content is retained in the context store."})),
        }).collect()
    })
}
