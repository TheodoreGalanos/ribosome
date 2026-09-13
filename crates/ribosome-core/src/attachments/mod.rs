mod events;
mod feedback;
mod repair;
mod service;

pub use service::AttachmentHost;

use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    subscriptions::Subscription,
    validation::{counter, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Trusted host configuration, never supplied through a model tool.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AttachmentPolicy {
    pub event_kinds: Vec<String>,
    pub batch_size: u32,
    pub max_delay_ms: u32,
    pub max_backlog: u32,
    pub feedback_ttl_ms: u32,
    pub allow_steering: bool,
    pub allow_coordinated_writes: bool,
}

impl Default for AttachmentPolicy {
    fn default() -> Self {
        Self {
            event_kinds: vec![
                "tool.completed".into(),
                "artifact.changed".into(),
                "execution.completed".into(),
                "execution.failed".into(),
            ],
            batch_size: 8,
            max_delay_ms: 200,
            max_backlog: 1000,
            feedback_ttl_ms: 300000,
            allow_steering: false,
            allow_coordinated_writes: false,
        }
    }
}

impl AttachmentPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.event_kinds.is_empty()
            || self.event_kinds.len() > 100
            || self
                .event_kinds
                .iter()
                .any(|k| k.starts_with("ribosome.") || k.len() > 200)
            || !(1..=100).contains(&self.batch_size)
            || !(1..=300000).contains(&self.max_delay_ms)
            || !(100..=10000).contains(&self.max_backlog)
            || !(1..=300000).contains(&self.feedback_ttl_ms)
        {
            return Err(Error::invalid(
                "invalid attachment routing or capacity limits",
            ));
        }
        Ok(())
    }

    pub fn capabilities(&self) -> Vec<AttachmentCapability> {
        let mut result = vec![AttachmentCapability::Observe];
        if self.allow_steering {
            result.push(AttachmentCapability::Steer);
        }
        if self.allow_coordinated_writes {
            result.push(AttachmentCapability::CoordinatedWrite);
        }
        result
    }
}

impl Store {
    pub fn attachment(&self, id: &str) -> Result<Attachment> {
        let body: String = self
            .db
            .query_row("SELECT body FROM attachments WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| Error::missing("attachment not found"))?;
        let mut attachment: Attachment = serde_json::from_str(&body)?;
        attachment.cursor =
            self.db
                .query_row("SELECT cursor FROM subscriptions WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        Ok(attachment)
    }

    pub fn run_attachment(&self, run_id: &str) -> Result<Option<Attachment>> {
        let id: Option<String> = self
            .db
            .query_row(
                "SELECT attachment_id FROM attachment_work WHERE work_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|id| self.attachment(&id)).transpose()
    }

    fn save_attachment(&self, attachment: &Attachment) -> Result<()> {
        validate("Attachment", &serde_json::to_value(attachment)?)?;
        self.db.execute(
            "UPDATE attachments SET body=?2 WHERE id=?1",
            params![attachment.id, serde_json::to_string(attachment)?],
        )?;
        Ok(())
    }

    pub fn open_attachment(
        &self,
        grant: &Grant,
        request: &AttachmentOpen,
        policy: &AttachmentPolicy,
        template: &AgentRunRequest,
    ) -> Result<Attachment> {
        validate("AttachmentOpen", &serde_json::to_value(request)?)?;
        policy.validate()?;
        if now_ms() >= counter(&grant.budget.deadline_ms)? {
            return Err(Error::exhausted("attachment grant expired"));
        }
        if !request
            .capabilities
            .contains(&AttachmentCapability::Observe)
            || request
                .capabilities
                .iter()
                .any(|c| !policy.capabilities().contains(c))
        {
            return Err(Error::denied("attachment capability not offered by host"));
        }
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM attachments WHERE id=?1)",
            [&request.id],
            |r| r.get(0),
        )?;
        if exists {
            let mut old = self.attachment(&request.id)?;
            if old.grant_id != grant.id
                || old.execution_id != request.execution_id
                || old.connector != request.connector
                || old.connector_version != request.connector_version
                || old.capabilities != request.capabilities
            {
                return Err(Error::conflict(
                    "attachment identity or capabilities changed",
                ));
            }
            if matches!(
                old.state,
                AttachmentState::Detached | AttachmentState::Completed
            ) {
                return Err(Error::conflict(
                    "attachment is terminal; use a new execution identity",
                ));
            }
            old.state = AttachmentState::Active;
            old.updated_ms = now_ms().to_string();
            self.save_attachment(&old)?;
            return Ok(old);
        }
        let count: u32 = self.db.query_row(
            "SELECT count(*) FROM attachments WHERE grant_id=?1",
            [&grant.id],
            |r| r.get(0),
        )?;
        if count >= 16 {
            return Err(Error::exhausted("attachment capacity reached"));
        }
        let duplicate: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM attachments a JOIN grants g ON a.grant_id=g.id WHERE a.execution_id=?1 AND json_extract(g.body,'$.scope.client')=?2 AND json_extract(g.body,'$.scope.project')=?3)", params![request.execution_id,grant.scope.client,grant.scope.project], |r|r.get(0))?;
        if duplicate {
            return Err(Error::conflict(
                "execution already has an attachment in this scope",
            ));
        }
        let held: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM attachments WHERE json_extract(body,'$.handoff.state') IN ('held','unknown'))", [], |r|r.get(0))?;
        if held {
            return Err(Error::conflict(
                "writer handoff must settle before another attachment opens",
            ));
        }
        let cursor = if request.start == AttachmentOpenStart::Now {
            self.event_cursor()?
        } else {
            "0".into()
        };
        let now = now_ms().to_string();
        let attachment = Attachment {
            id: request.id.clone(),
            grant_id: grant.id.clone(),
            scope: grant.scope.clone(),
            execution_id: request.execution_id.clone(),
            connector: request.connector.clone(),
            connector_version: request.connector_version.clone(),
            capabilities: request.capabilities.clone(),
            state: AttachmentState::Active,
            source_status: AttachmentSourceStatus::Unknown,
            start_cursor: cursor.clone(),
            cursor: cursor.clone(),
            frontier: Default::default(),
            coverage: vec![if request.start == AttachmentOpenStart::Now {
                "Earlier execution history was not requested.".into()
            } else {
                "Includes only history already supplied by the host; missing history is unknown."
                    .into()
            }],
            created_ms: now.clone(),
            updated_ms: now,
            last_error: String::new(),
            handoff: None,
        };
        let tx = self.write_transaction()?;
        tx.execute(
            "INSERT INTO attachments(id,grant_id,execution_id,body) VALUES (?1,?2,?3,?4)",
            params![
                attachment.id,
                grant.id,
                attachment.execution_id,
                serde_json::to_string(&attachment)?
            ],
        )?;
        self.subscribe_from(
            &Subscription {
                id: attachment.id.clone(),
                grant_id: grant.id.clone(),
                kinds: policy.event_kinds.clone(),
                profile: template.profile.clone(),
                operator: template.operator.clone(),
                subject: attachment.id.clone(),
                batch_size: policy.batch_size,
                max_delay_ms: policy.max_delay_ms,
            },
            Some(&attachment.execution_id),
            &cursor,
            Some(&attachment.id),
        )?;
        tx.commit()?;
        Ok(attachment)
    }

    fn event_cursor(&self) -> Result<String> {
        let cursor: i64 =
            self.db
                .query_row("SELECT coalesce(max(cursor),0) FROM events", [], |r| {
                    r.get(0)
                })?;
        Ok(cursor.to_string())
    }

    fn active_attachment(&self, id: &str) -> Result<(Attachment, Grant)> {
        let a = self.attachment(id)?;
        let g = self.grant(&a.grant_id)?;
        if a.state != AttachmentState::Active {
            return Err(Error::denied("attachment is not active"));
        }
        if now_ms() >= counter(&g.budget.deadline_ms)? {
            return Err(Error::exhausted("attachment grant expired"));
        }
        Ok((a, g))
    }

    pub fn interrupt_attachment(&self, id: &str, reason: &str) -> Result<()> {
        let mut a = self.attachment(id)?;
        if matches!(
            a.state,
            AttachmentState::Detached | AttachmentState::Completed
        ) {
            return Ok(());
        }
        a.state = AttachmentState::Interrupted;
        if a.source_status == AttachmentSourceStatus::Running {
            a.source_status = AttachmentSourceStatus::Unknown;
        }
        a.last_error = reason.chars().take(2000).collect();
        if a.coverage.len() < 20 && !a.coverage.contains(&a.last_error) {
            a.coverage.push(a.last_error.clone());
        }
        a.updated_ms = now_ms().to_string();
        if let Some(h) = &mut a.handoff
            && h.state == RepairHandoffState::Held
        {
            h.state = RepairHandoffState::Unknown;
        }
        self.save_attachment(&a)
    }

    pub fn complete_attachment_source(&self, id: &str) -> Result<()> {
        let (mut a, _) = self.active_attachment(id)?;
        if matches!(
            a.source_status,
            AttachmentSourceStatus::Running | AttachmentSourceStatus::Unknown
        ) {
            a.source_status = AttachmentSourceStatus::Completed;
        }
        a.updated_ms = now_ms().to_string();
        self.save_attachment(&a)
    }

    pub fn detach_attachment(&self, id: &str) -> Result<()> {
        let mut a = self.attachment(id)?;
        if a.handoff
            .as_ref()
            .is_some_and(|h| h.state != RepairHandoffState::Released)
        {
            return Err(Error::conflict(
                "unsettled writer handoff; keep the external writer stopped",
            ));
        }
        let status = self.attachment_status(id)?;
        a.state = if a.source_status == AttachmentSourceStatus::Completed
            && status.queued_work == 0
            && status.running_work == 0
            && status.pending_feedback == 0
        {
            AttachmentState::Completed
        } else {
            AttachmentState::Detached
        };
        a.updated_ms = now_ms().to_string();
        let tx = self.write_transaction()?;
        self.save_attachment(&a)?;
        tx.execute("UPDATE work SET status='cancelled',body=json_set(body,'$.status','cancelled') WHERE id IN (SELECT work_id FROM attachment_work WHERE attachment_id=?1) AND status IN ('queued','interrupted')", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn attachment_ids(&self, grant_id: &str) -> Result<Vec<String>> {
        Ok(self
            .db
            .prepare("SELECT id FROM attachments WHERE grant_id=?1 ORDER BY id")?
            .query_map([grant_id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn attachment_status(&self, id: &str) -> Result<AttachmentStatus> {
        let a = self.attachment(id)?;
        let count = |states: &str| -> Result<u32> {
            Ok(self.db.query_row("SELECT count(*) FROM work w JOIN attachment_work aw ON aw.work_id=w.id WHERE aw.attachment_id=?1 AND w.status IN (SELECT value FROM json_each(?2))",params![id,states], |r|r.get(0))?)
        };
        let pending: u32 = self.db.query_row("SELECT count(*) FROM attachment_feedback WHERE attachment_id=?1 AND json_extract(body,'$.state') IN ('pending','delivered')",[id], |r|r.get(0))?;
        let usage: String = self.db.query_row("SELECT json_object('calls',count(*),'unknown_calls',coalesce(sum(CASE WHEN usage IS NULL OR json_extract(usage,'$.complete')<>1 THEN 1 ELSE 0 END),0),'observed_cost_microusd',CAST(coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) AS TEXT)) FROM permits WHERE grant_id=?1 AND state!='released'",[&a.grant_id], |r|r.get(0))?;
        let feedback_states: String = self.db.query_row("SELECT coalesce(json_group_object(state,total),'{}') FROM (SELECT json_extract(body,'$.state') state,count(*) total FROM attachment_feedback WHERE attachment_id=?1 GROUP BY state)",[id], |r|r.get(0))?;
        Ok(AttachmentStatus {
            attachment: a,
            queued_work: count("[\"queued\",\"interrupted\"]")?,
            running_work: count("[\"running\"]")?,
            pending_feedback: pending,
            usage: serde_json::from_str(&usage)?,
            feedback_states: serde_json::from_str(&feedback_states)?,
        })
    }
}
