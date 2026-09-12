use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, now_ms, validate},
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
                .execute("UPDATE runs SET status='running' WHERE id=?1", [run_id])?;
        } else {
            self.db.execute("INSERT INTO runs(id,grant_id,request,status,started_ms) VALUES (?1,?2,?3,'running',?4)", params![run_id,grant_id,serialized,now_ms().to_string()])?;
        }
        Ok(())
    }

    pub fn run_grant(&self, run_id: &str) -> Result<Grant> {
        let grant_id: String = self
            .db
            .query_row("SELECT grant_id FROM runs WHERE id=?1", [run_id], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| Error::missing("run not found"))?;
        self.grant(&grant_id)
    }

    pub fn require_active(&self, run_id: &str) -> Result<Grant> {
        let grant = self.run_grant(run_id)?;
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
        Ok(grant)
    }

    pub fn finish_run(&self, run_id: &str, result: &AgentResult) -> Result<()> {
        let status = serde_json::to_value(&result.disposition)?
            .as_str()
            .unwrap()
            .to_owned();
        self.db.execute(
            "UPDATE runs SET status=?2,result=?3,finished_ms=?4 WHERE id=?1 AND status='running'",
            params![
                run_id,
                status,
                serde_json::to_string(result)?,
                now_ms().to_string()
            ],
        )?;
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
            .prepare("SELECT body FROM effects WHERE run_id=?1 ORDER BY rowid")?;
        let effects = effects
            .query_map([run_id], |r| r.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<Value>(&row?)?))
            .collect::<Result<Vec<_>>>()?;
        let mut usage = self
            .db
            .prepare("SELECT usage FROM permits WHERE run_id=?1 ORDER BY rowid")?;
        let usage = usage
            .query_map([run_id], |r| r.get::<_, Option<String>>(0))?
            .map(|row| {
                Ok(row?
                    .map(|body| serde_json::from_str::<Usage>(&body))
                    .transpose()?)
            })
            .collect::<Result<Vec<_>>>()?;
        let unknown = usage
            .iter()
            .filter(|u| u.as_ref().is_none_or(|u| !u.complete))
            .count();
        let mut input = 0_u64;
        let mut output = 0_u64;
        let mut cost = 0_u64;
        for u in usage.iter().flatten() {
            input += counter(&u.input_tokens)?;
            output += counter(&u.output_tokens)?;
            cost += counter(&u.cost_microusd)?;
        }
        let checkpoint = self.load_checkpoint(run_id)?.map(|c| {
            let tool_calls = c.messages.iter().filter(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
                .filter_map(|m| m.get("content").and_then(Value::as_array))
                .flatten().filter(|part| part["type"] == "toolCall")
                .map(|part| json!({"id":part["id"],"name":part["name"],"record_kind":part["arguments"]["kind"],"inventory":part["arguments"]["inventory"],"record_id":part["arguments"]["id"]})).collect::<Vec<_>>();
            json!({"format":c.format,"profile":c.profile,"operator":c.operator,"message_count":c.messages.len(),"pending_operations":c.pending_operations,"event_cursor":c.event_cursor,"tool_calls":tool_calls})
        });
        Ok(
            json!({"id":run_id,"status":status,"request":serde_json::from_str::<Value>(&request)?,"result":result.map(|r|serde_json::from_str::<Value>(&r)).transpose()?,"started_ms":started,"finished_ms":finished,"effects":effects,"checkpoint":checkpoint,"agent_activity_count":activity_count,"model_usage":{"calls":usage.len(),"unknown_calls":unknown,"observed_input_tokens":input.to_string(),"observed_output_tokens":output.to_string(),"observed_cost_microusd":cost.to_string(),"complete":unknown==0}}),
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
            .map(|s| Ok(serde_json::from_str(&s)?))
            .transpose()
    }
}
