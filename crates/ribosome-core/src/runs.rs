use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, derived_split, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

impl Store {
    pub fn begin_run(&self, run_id: &str, grant_id: &str, request: &AgentRunRequest) -> Result<()> {
        let grant = self.grant(grant_id)?;
        if !grant.profiles.contains(&request.profile) {
            return Err(Error::denied("profile not granted"));
        }
        validate("AgentRunRequest", &serde_json::to_value(request)?)?;
        if (request.operator == "execute-motif@1") != request.invocation.is_some() {
            return Err(Error::invalid(
                "execute-motif requires a pinned invocation and other operators cannot receive one",
            ));
        }
        if let Some(invocation) = &request.invocation {
            if request.profile != Profile::Caretaker
                || request.discovery_corpus.is_some()
                || grant.discovery_corpus.is_some()
            {
                return Err(Error::denied(
                    "instruction execution requires a caretaker outside discovery",
                ));
            }
            self.validate_invocation(&grant, run_id, invocation)?;
        }
        let effective = self.discovery_grant(&grant, request.discovery_corpus.as_ref())?;
        if effective.discovery_corpus.is_some()
            && !crate::discovery_corpus::corpus_operator_allowed(
                &request.profile,
                &request.operator,
            )
        {
            return Err(Error::denied(
                "corpus assignments support curator work and caretaker proofreading",
            ));
        }
        let parent: Option<String> = self
            .db
            .query_row(
                "SELECT json_extract(w.body,'$.parent_id') FROM work w JOIN runs r ON r.id=json_extract(w.body,'$.parent_id') WHERE w.id=?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(parent) = parent {
            let parent = self.run_grant(&parent)?;
            if parent.discovery_corpus.is_some()
                && parent.discovery_corpus != effective.discovery_corpus
            {
                return Err(Error::denied(
                    "child cannot replace its parent's discovery corpus",
                ));
            }
        }
        let tx = self.write_transaction()?;
        let existing: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT grant_id,request FROM runs WHERE id=?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let mut pinned = request.clone();
        pinned.checkpoint = None;
        let serialized = serde_json::to_string(&pinned)?;
        if let Some((previous_grant, previous_request)) = existing {
            if previous_grant != grant_id || previous_request != serialized {
                return Err(Error::conflict(
                    "run identity or pinned configuration changed",
                ));
            }
            let status: String =
                self.db
                    .query_row("SELECT status FROM runs WHERE id=?1", [run_id], |r| {
                        r.get(0)
                    })?;
            if status != "interrupted" {
                return Err(Error::conflict("only interrupted runs can resume"));
            }
            self.db
                .execute("UPDATE runs SET status=CASE WHEN EXISTS(SELECT 1 FROM run_waits WHERE run_id=?1) THEN 'waiting' ELSE 'running' END WHERE id=?1", [run_id])?;
        } else {
            self.db.execute("INSERT INTO runs(id,grant_id,request,status,started_ms) VALUES (?1,?2,?3,'running',?4)", params![run_id,grant_id,serialized,now_ms().to_string()])?;
        }
        self.bind_run_allocation_in(&grant, request)?;
        tx.commit()?;
        Ok(())
    }

    pub fn run_grant(&self, run_id: &str) -> Result<Grant> {
        let (grant_id, corpus): (String, Option<String>) = self
            .db
            .query_row(
                "SELECT grant_id,json_extract(request,'$.discovery_corpus') FROM runs WHERE id=?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("run not found"))?;
        let corpus: Option<VersionRef> =
            corpus.map(|body| serde_json::from_str(&body)).transpose()?;
        let mut grant = self.discovery_grant(&self.grant(&grant_id)?, corpus.as_ref())?;
        let operator: String = self.db.query_row(
            "SELECT json_extract(request,'$.operator') FROM runs WHERE id=?1",
            [run_id],
            |r| r.get(0),
        )?;
        if matches!(operator.as_str(), "execute-motif@1" | "recombination@1") {
            grant.prepared_run = Some(run_id.into());
        }
        Ok(grant)
    }

    pub fn require_active(&self, run_id: &str) -> Result<Grant> {
        let grant = self.run_grant(run_id)?;
        self.work_source(&grant, run_id)?;
        if self.run_invocation(run_id)?.is_some() {
            self.invocation_material(run_id, &grant)?;
        }
        if self
            .run_attachment(run_id)?
            .is_some_and(|a| a.state != AttachmentState::Active)
        {
            return Err(Error::denied("attached execution is no longer active"));
        }
        let status: String =
            self.db
                .query_row("SELECT status FROM runs WHERE id=?1", [run_id], |r| {
                    r.get(0)
                })?;
        if status != "running" {
            return Err(Error::denied("run is not active"));
        }
        if now_ms() >= counter(&grant.budget.deadline_ms)? {
            return Err(Error::exhausted("grant deadline reached"));
        }
        self.require_allocation_open(&grant, &self.run_allocation_in(run_id)?.id)?;
        Ok(grant)
    }

    pub fn finish_run(&self, run_id: &str, result: &AgentResult) -> Result<()> {
        validate("AgentResult", &serde_json::to_value(result)?)?;
        let tx = self.write_transaction()?;
        let running: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1 AND status IN ('running','waiting'))",
            [run_id],
            |r| r.get(0),
        )?;
        if !running {
            return Ok(());
        }
        let grant = self.run_grant(run_id)?;
        let source = id();
        let sequence:i64 = tx.query_row("SELECT coalesce(max(CAST(sequence AS INTEGER)),0)+1 FROM events WHERE run_id=?1 AND producer='run-result'",[run_id],|r|r.get(0))?;
        self.ingest(&Event { id:source.clone(),scope:grant.scope,run_id:run_id.into(),producer:"run-result".into(),sequence:sequence.to_string(),kind:"run_finished".into(),timestamp_ms:now_ms().to_string(),parents:vec![],correlation:run_id.into(),artifacts:vec![],payload:json!({"run_id":run_id,"disposition":result.disposition,"authority":"run-reported disposition; summary is retained with the run"}).as_object().unwrap().clone(),provenance:Provenance{origin:Origin::Observed,source_refs:self.run_context_sources(run_id)?,scenario_family:"run-result".into(),split:derived_split(&grant.visible_splits),limitations:vec![]}})?;
        let status = serde_json::to_value(&result.disposition)?
            .as_str()
            .unwrap()
            .to_owned();
        self.db.execute(
            "UPDATE runs SET status=?2,result=?3,finished_ms=?4,result_source=?5 WHERE id=?1 AND status IN ('running','waiting')",
            params![
                run_id,
                status,
                serde_json::to_string(result)?,
                now_ms().to_string(),
                source
            ],
        )?;
        self.db.execute("UPDATE budget_allocations SET body=json_set(body,'$.disposition',?2) WHERE id=(SELECT allocation_id FROM run_allocations WHERE run_id=?1)", params![run_id,status])?;
        if result.disposition != Disposition::Interrupted {
            tx.execute("DELETE FROM run_waits WHERE run_id=?1", [run_id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn inspect_run(&self, run_id: &str) -> Result<Value> {
        let activity_count: u32 = self.db.query_row(
            "SELECT count(*) FROM events WHERE run_id=?1 AND producer='pi-agent'",
            [run_id],
            |r| r.get(0),
        )?;
        let (status, request, result, started, finished): (
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        ) = self
            .db
            .query_row(
                "SELECT status,request,result,started_ms,finished_ms FROM runs WHERE id=?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("run not found"))?;
        let mut effects = self
            .db
            .prepare("SELECT CASE WHEN settlement IS NULL THEN body ELSE json_set(body,'$.settlement',json(settlement)) END FROM effects WHERE run_id=?1 ORDER BY rowid")?;
        let effects = effects
            .query_map([run_id], |r| r.get::<_, String>(0))?
            .map(|row| {
                let receipt = serde_json::from_str::<ActionReceipt>(&row?)?;
                Ok(serde_json::to_value(self.receipt_for_delivery(
                    &self.run_grant(run_id)?,
                    receipt,
                )?)?)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut usage = self.db.prepare(
            "SELECT usage FROM permits WHERE run_id=?1 AND state!='released' ORDER BY rowid",
        )?;
        let usage = usage
            .query_map([run_id], |r| r.get::<_, Option<String>>(0))?
            .map(|row| {
                Ok(row?
                    .map(|body| serde_json::from_str::<Usage>(&body))
                    .transpose()?)
            })
            .collect::<Result<Vec<_>>>()?;
        let unsettled = usage
            .iter()
            .filter(|u| u.as_ref().is_none_or(|u| !u.complete))
            .count();
        let (undispatched,released): (u32,u32) = self.db.query_row("SELECT coalesce(sum(state='reserved'),0),coalesce(sum(state='released'),0) FROM permits WHERE run_id=?1", [run_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
        let unknown = unsettled.saturating_sub(undispatched as usize);
        let budget = self.run_budget_status(run_id)?;
        let mut input = 0_u64;
        let mut output = 0_u64;
        let mut cost = 0_u64;
        for u in usage.iter().flatten() {
            input += counter(&u.input_tokens)?;
            output += counter(&u.output_tokens)?;
            cost += counter(&u.cost_microusd)?;
        }
        let (retained_count, messages) = self.inspect_context_messages(run_id)?;
        let available = self.run_result_available(run_id)?;
        let result = result.filter(|_| available);
        let checkpoint = self.load_checkpoint(run_id)?.map(|c| {
            let tool_calls = messages.iter().filter(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
                .filter_map(|m| m.get("content").and_then(Value::as_array))
                .flatten().filter(|part| part["type"] == "toolCall")
                .map(|part| json!({"id":part["id"],"name":part["name"],"record_kind":part["arguments"]["kind"],"inventory":part["arguments"]["inventory"],"record_id":part["arguments"]["id"]})).collect::<Vec<_>>();
            json!({"format":c.format,"profile":c.profile,"operator":c.operator,"message_count":if c.context.is_some(){retained_count}else{c.messages.len()},"pending_operations":c.pending_operations,"event_cursor":c.event_cursor,"tool_calls":tool_calls})
        });
        Ok(
            json!({"id":run_id,"status":status,"request":serde_json::from_str::<Value>(&request)?,"result":result.map(|r|serde_json::from_str::<Value>(&r)).transpose()?,"result_available":available,"started_ms":started,"finished_ms":finished,"effects":effects,"checkpoint":checkpoint,"agent_activity_count":activity_count,"budget":budget,"timings":self.inspect_timings(run_id)?,"waiting_on":self.waiting_work(run_id)?,"model_usage":{"calls":usage.len(),"unknown_calls":unknown,"undispatched_calls":undispatched,"released_calls":released,"observed_input_tokens":input.to_string(),"observed_output_tokens":output.to_string(),"observed_cost_microusd":cost.to_string(),"complete":unsettled==0}}),
        )
    }

    pub fn checkpoint(&self, run_id: &str, checkpoint: &Checkpoint) -> Result<()> {
        validate("Checkpoint", &serde_json::to_value(checkpoint)?)?;
        let request: String =
            self.db
                .query_row("SELECT request FROM runs WHERE id=?1", [run_id], |r| {
                    r.get(0)
                })?;
        let pinned: AgentRunRequest = serde_json::from_str(&request)?;
        if checkpoint.profile != pinned.profile
            || checkpoint.operator != pinned.operator
            || checkpoint.provider != pinned.provider
            || checkpoint.model != pinned.model
        {
            return Err(Error::conflict(
                "checkpoint changes pinned profile, operator, or model",
            ));
        }
        self.check_context_checkpoint(run_id, checkpoint)?;
        self.db.execute("INSERT INTO checkpoints(run_id,body) VALUES (?1,?2) ON CONFLICT(run_id) DO UPDATE SET body=excluded.body", params![run_id,serde_json::to_string(checkpoint)?])?;
        Ok(())
    }

    pub fn load_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        self.db
            .query_row(
                "SELECT body FROM checkpoints WHERE run_id=?1",
                [run_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|s| {
                let mut checkpoint: Checkpoint = serde_json::from_str(&s)?;
                if checkpoint.format != "pi-0.85.1/2" {
                    checkpoint.messages.clear();
                }
                Ok(checkpoint)
            })
            .transpose()
    }

    pub(crate) fn run_result_available(&self, run_id: &str) -> Result<bool> {
        let result: Option<(Option<String>, bool)> = self
            .db
            .query_row(
                "SELECT result_source,result IS NOT NULL FROM runs WHERE id=?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((source, present)) = result else {
            return Ok(false);
        };
        if !present {
            return Ok(false);
        }
        match source {
            Some(source) => self.source_available(&self.run_grant(run_id)?, "event", &source),
            None => Ok(false),
        }
    }

    pub(crate) fn run_result_for_delivery(&self, run_id: &str) -> Result<AgentResult> {
        let (status, body): (String, Option<String>) = self.db.query_row(
            "SELECT status,result FROM runs WHERE id=?1",
            [run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if self.run_result_available(run_id)? {
            return Ok(serde_json::from_str(body.as_deref().unwrap())?);
        }
        Ok(AgentResult { disposition:serde_json::from_value(Value::String(status))?,summary:"The run's completion text is withheld because its source lineage or retained payload is unavailable. Inspect authorized host receipts for the outcome.".into() })
    }
}
