use super::*;
use serde_json::{Value, json};

impl Store {
    pub fn append_attachment_events(
        &self,
        request: &AttachmentEvents,
        policy: &AttachmentPolicy,
    ) -> Result<IngestionReceipt> {
        validate("AttachmentEvents", &serde_json::to_value(request)?)?;
        if serde_json::to_vec(request)?.len() > 256 * 1024 {
            return Err(Error::exhausted("event batch exceeds 256 KiB"));
        }
        let (mut a, grant) = self.active_attachment(&request.attachment_id)?;
        let backlog: u32 = self.db.query_row("SELECT count(*) FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND cursor>CAST(?4 AS INTEGER)",params![a.scope.client,a.scope.project,a.execution_id,a.cursor], |r|r.get(0))?;
        let mut inserted = 0;
        let mut duplicates = 0;
        let tx = self.write_transaction()?;
        for input in &request.events {
            if input.kind.starts_with("ribosome.") {
                return Err(Error::denied(
                    "Ribosome feedback cannot be ingested as source activity",
                ));
            }
            if input
                .artifacts
                .iter()
                .any(|r| !grant.paths.contains(&r.path))
            {
                return Err(Error::denied("event references an ungranted artifact"));
            }
            let event = Event {
                id: format!("{}:{}", a.id,input.id), scope: a.scope.clone(), run_id: a.execution_id.clone(), producer: format!("source:{}:{}",a.id,input.producer), sequence: input.sequence.clone(), kind: input.kind.clone(), timestamp_ms: input.timestamp_ms.clone(), parents: input.parents.iter().map(|id|format!("{}:{id}",a.id)).collect(), correlation: input.correlation.clone(), artifacts: input.artifacts.clone(), payload: input.payload.clone(),
                provenance: Provenance { origin:Origin::Observed,source_refs:vec![],scenario_family:"attached-execution".into(),split:crate::validation::derived_split(&grant.visible_splits),limitations:vec!["Reported by the external host; assistant statements are not Ribosome effect receipts.".into()] },
            };
            if !self.ingest(&event)? {
                duplicates += 1;
                continue;
            }
            inserted += 1;
            if backlog + inserted > policy.max_backlog {
                return Err(Error::exhausted(
                    "attachment evidence backlog full; source coverage interrupted",
                ));
            }
            let previous = a
                .frontier
                .get(&input.producer)
                .and_then(Value::as_str)
                .map(counter)
                .transpose()?
                .unwrap_or(0);
            let sequence = counter(&input.sequence)?;
            if sequence <= previous {
                return Err(Error::conflict(
                    "source sequence decreased or was reused; use a new producer identity after restart",
                ));
            }
            if sequence != previous + 1 && a.coverage.len() < 20 {
                a.coverage.push(format!(
                    "Producer {} has unobserved sequence numbers between {} and {}.",
                    input.producer, previous, sequence
                ));
            }
            if !a.frontier.contains_key(&input.producer) && a.frontier.len() >= 100 {
                return Err(Error::exhausted("attachment producer capacity reached"));
            }
            a.frontier
                .insert(input.producer.clone(), json!(input.sequence));
            match input.kind.as_str() {
                "execution.started" => a.source_status = AttachmentSourceStatus::Running,
                "execution.completed" => a.source_status = AttachmentSourceStatus::Completed,
                "execution.failed" => a.source_status = AttachmentSourceStatus::Failed,
                "execution.cancelled" => a.source_status = AttachmentSourceStatus::Cancelled,
                _ => {}
            }
        }
        a.updated_ms = now_ms().to_string();
        self.save_attachment(&a)?;
        let cursor: i64 = self.db.query_row("SELECT coalesce(max(cursor),0) FROM events WHERE client=?1 AND project=?2 AND run_id=?3",params![a.scope.client,a.scope.project,a.execution_id],|r|r.get(0))?;
        tx.commit()?;
        Ok(IngestionReceipt {
            inserted,
            duplicates,
            cursor: cursor.to_string(),
        })
    }

    pub(crate) fn route_attachment(&self, id: &str) -> Result<()> {
        let (a, _) = self.active_attachment(id)?;
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE client=?1 AND project=?2 AND run_id=?3)",
            params![a.scope.client, a.scope.project, a.execution_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(());
        }
        // During a repair, only its explicitly bound work may execute.
        if a.handoff
            .as_ref()
            .is_some_and(|h| h.state != RepairHandoffState::Released)
        {
            return Ok(());
        }
        self.poll_subscription_with_flush(id, a.source_status != AttachmentSourceStatus::Running)?;
        Ok(())
    }
}
