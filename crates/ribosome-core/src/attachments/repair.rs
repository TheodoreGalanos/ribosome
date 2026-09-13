use super::*;
use crate::{effects::Runtime, validation::id};

impl Store {
    pub(crate) fn require_attachment_write(&self, run_id: &str, action: &Action) -> Result<()> {
        let live_write = action.kind == ActionKind::Apply
            || (action.branch_id.is_none()
                && matches!(action.kind, ActionKind::Edit | ActionKind::Execute));
        if !live_write {
            return Ok(());
        }
        let Some(a) = self.run_attachment(run_id)? else {
            return Ok(());
        };
        if !a
            .capabilities
            .contains(&AttachmentCapability::CoordinatedWrite)
            || !a.handoff.as_ref().is_some_and(|h| {
                h.state == RepairHandoffState::Held
                    && h.work_id == run_id
                    && counter(&h.deadline_ms).is_ok_and(|t| now_ms() < t)
            })
        {
            return Err(Error::denied(
                "live attachment writes require a current cooperative writer handoff",
            ));
        }
        Ok(())
    }

    pub fn begin_attachment_repair(
        &self,
        attachment_id: &str,
        template: &AgentRunRequest,
    ) -> Result<RepairHandoff> {
        let (mut a, grant) = self.active_attachment(attachment_id)?;
        if !a
            .capabilities
            .contains(&AttachmentCapability::CoordinatedWrite)
            || grant.mode != Mode::Apply
            || grant.required_checks.as_ref().is_none_or(Vec::is_empty)
        {
            return Err(Error::denied(
                "coordinated repair requires the host capability, an apply grant and mandatory checks",
            ));
        }
        if let Some(h) = &a.handoff
            && h.state != RepairHandoffState::Released
        {
            return Err(Error::conflict(
                "previous handoff must be reconciled and released",
            ));
        }
        let others: u32 = self.db.query_row("SELECT count(*) FROM attachments WHERE id<>?1 AND json_extract(body,'$.state') NOT IN ('detached','completed')",[attachment_id], |r|r.get(0))?;
        if others != 0 {
            return Err(Error::denied(
                "coordinated repair supports one external writer coordinator per workspace",
            ));
        }
        let status = self.attachment_status(attachment_id)?;
        if status.running_work + status.queued_work != 0 {
            return Err(Error::conflict(
                "finish pending maintenance before handing off the writer",
            ));
        }
        let mut evidence = self.db.prepare("SELECT id FROM events WHERE client=?1 AND project=?2 AND run_id=?3 ORDER BY cursor DESC LIMIT 100")?.query_map(params![a.scope.client,a.scope.project,a.execution_id], |r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        evidence.reverse();
        if evidence.is_empty() {
            return Err(Error::denied("repair requires observed source evidence"));
        }
        let generation = id();
        let request = WorkRequest {
            subject: format!("repair:{generation}"),
            profile: Profile::Caretaker,
            operator: "excision-repair@1".into(),
            reason: format!(
                "{} The external host has stopped its writers and granted a bounded handoff. Repair the supported defect through a checked branch and preserve independent work. Complete within this run; do not request follow-up work.",
                template.prompt
            ),
            evidence_refs: evidence,
        };
        self.validate_work_request(&grant, &request)?;
        let tx = self.write_transaction()?;
        let work = self.enqueue_work_in(&tx, &grant, attachment_id, &request)?;
        tx.execute(
            "INSERT INTO attachment_work(work_id,attachment_id) VALUES (?1,?2)",
            params![work.id, attachment_id],
        )?;
        let handoff = RepairHandoff {
            generation,
            work_id: work.id,
            deadline_ms: (now_ms() + 300000)
                .min(counter(&grant.budget.deadline_ms)?)
                .to_string(),
            state: RepairHandoffState::Held,
        };
        a.handoff = Some(handoff.clone());
        a.updated_ms = now_ms().to_string();
        self.save_attachment(&a)?;
        tx.commit()?;
        Ok(handoff)
    }
}

impl Runtime {
    pub fn release_attachment_repair(&self, request: &AttachmentRelease) -> Result<()> {
        let mut a = self.store.attachment(&request.attachment_id)?;
        let h = a
            .handoff
            .as_mut()
            .ok_or_else(|| Error::missing("attachment has no writer handoff"))?;
        if h.generation != request.generation {
            return Err(Error::conflict("stale writer handoff generation"));
        }
        if h.state == RepairHandoffState::Released {
            return Ok(());
        }
        let running: bool = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1 AND status='running')",
            [&h.work_id],
            |r| r.get(0),
        )?;
        if running {
            return Err(Error::conflict("maintenance still owns the writer handoff"));
        }
        // Release can settle after cancellation/expiry. It never dispatches an
        // effect; uncertain receipts keep the external writer stopped.
        let started: bool = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1)",
            [&h.work_id],
            |r| r.get(0),
        )?;
        if started {
            let receipts = self.reconcile_run(&h.work_id)?;
            if receipts.iter().any(ActionReceipt::requires_reconciliation) {
                return Err(Error::conflict(
                    "unknown effects require owner reconciliation; writer handoff remains blocked",
                ));
            }
        }
        let tx = self.store.write_transaction()?;
        tx.execute("UPDATE work SET status='cancelled',body=json_set(body,'$.status','cancelled') WHERE id=?1 AND status IN ('queued','running','interrupted')",[&h.work_id])?;
        h.state = RepairHandoffState::Released;
        a.updated_ms = now_ms().to_string();
        self.store.save_attachment(&a)?;
        tx.commit()?;
        Ok(())
    }
}
