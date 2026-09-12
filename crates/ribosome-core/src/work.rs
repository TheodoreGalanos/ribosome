use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn request_work(&self, run_id: &str, request: &WorkRequest) -> Result<WorkItem> {
        let grant = self.require_active(run_id)?;
        if self.run_attachment(run_id)?.is_some_and(|a| {
            a.handoff
                .is_some_and(|h| h.state != RepairHandoffState::Released)
        }) {
            return Err(Error::denied(
                "coordinated repair must finish within its current work item",
            ));
        }
        self.enqueue_work(&grant, run_id, request)
    }

    pub(crate) fn enqueue_work(
        &self,
        grant: &Grant,
        run_id: &str,
        request: &WorkRequest,
    ) -> Result<WorkItem> {
        self.validate_work_request(grant, request)?;
        let tx = self.write_transaction()?;
        let work = self.enqueue_work_in(&tx, grant, run_id, request)?;
        tx.commit()?;
        Ok(work)
    }

    pub(crate) fn enqueue_work_in(
        &self,
        tx: &rusqlite::Transaction<'_>,
        grant: &Grant,
        run_id: &str,
        request: &WorkRequest,
    ) -> Result<WorkItem> {
        validate("WorkRequest", &serde_json::to_value(request)?)?;
        if !grant.profiles.contains(&request.profile) {
            return Err(Error::denied("requested profile not granted"));
        }
        if let Some(body)=tx.query_row("SELECT body FROM work WHERE grant_id=?1 AND subject=?2 AND status IN ('queued','running','interrupted')",params![grant.id,request.subject],|r|r.get::<_,String>(0)).optional()?{return Ok(serde_json::from_str(&body)?);}
        let parent: Option<String> = tx
            .query_row("SELECT body FROM work WHERE id=?1", [run_id], |r| r.get(0))
            .optional()?;
        let (depth, root_id) = if let Some(parent) = parent {
            let parent: WorkItem = serde_json::from_str(&parent)?;
            (parent.depth + 1, parent.root_id)
        } else {
            (1, run_id.to_owned())
        };
        if depth > grant.budget.max_depth {
            return Err(Error::exhausted("follow-up depth exhausted"));
        }
        let count: u32 = tx.query_row(
            "SELECT count(*) FROM work WHERE grant_id=?1",
            [&grant.id],
            |r| r.get(0),
        )?;
        if count >= grant.budget.max_work_items {
            return Err(Error::exhausted("root work budget exhausted"));
        }
        let repeated: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM work WHERE grant_id=?1 AND subject=?2)",
            params![grant.id, request.subject],
            |r| r.get(0),
        )?;
        if repeated {
            return Err(Error::conflict(
                "subject already processed in this root grant; owner must schedule a new bounded investigation",
            ));
        }
        let work = WorkItem {
            id: id(),
            scope: grant.scope.clone(),
            subject: request.subject.clone(),
            profile: request.profile.clone(),
            operator: request.operator.clone(),
            reason: request.reason.clone(),
            evidence_refs: request.evidence_refs.clone(),
            root_id,
            parent_id: run_id.into(),
            depth,
            status: WorkItemStatus::Queued,
            attempts: 0,
            lease_until_ms: "0".into(),
            owner: run_id.into(),
        };
        tx.execute("INSERT INTO work(id,grant_id,root_id,subject,status,lease_until_ms,body) VALUES (?1,?2,?3,?4,'queued',0,?5)",params![work.id,grant.id,work.root_id,work.subject,serde_json::to_string(&work)?])?;
        tx.execute("INSERT INTO attachment_work(work_id,attachment_id) SELECT ?1,attachment_id FROM attachment_work WHERE work_id=?2", params![work.id, run_id])?;
        Ok(work)
    }

    pub(crate) fn validate_work_request(&self, grant: &Grant, request: &WorkRequest) -> Result<()> {
        validate("WorkRequest", &serde_json::to_value(request)?)?;
        for reference in &request.evidence_refs {
            self.require_reference(grant, reference)?;
        }
        Ok(())
    }

    pub fn claim_work(
        &self,
        grant_id: &str,
        owner: &str,
        lease_ms: u32,
    ) -> Result<Option<WorkItem>> {
        self.claim_work_matching(grant_id, owner, lease_ms, false)
    }

    pub(crate) fn claim_work_matching(
        &self,
        grant_id: &str,
        owner: &str,
        lease_ms: u32,
        attached_only: bool,
    ) -> Result<Option<WorkItem>> {
        let grant = self.grant(grant_id)?;
        if counter(&grant.budget.deadline_ms)? <= now_ms() {
            return Err(Error::exhausted("grant expired"));
        }
        if owner.is_empty() || lease_ms == 0 || lease_ms > 300000 {
            return Err(Error::invalid("invalid lease"));
        }
        let tx = self.write_transaction()?;
        loop {
            let body: Option<String> = tx.query_row("SELECT body FROM work WHERE grant_id=?1 AND (status IN ('queued','interrupted') OR (status='running' AND lease_until_ms<?2)) AND (?3=0 OR id IN (SELECT aw.work_id FROM attachment_work aw JOIN attachments a ON a.id=aw.attachment_id WHERE json_extract(a.body,'$.state')='active' AND (json_extract(a.body,'$.handoff.state') IS NULL OR json_extract(a.body,'$.handoff.state')='released' OR (json_extract(a.body,'$.handoff.state')='held' AND json_extract(a.body,'$.handoff.work_id')=aw.work_id)))) ORDER BY id LIMIT 1", params![grant_id,now_ms() as i64,attached_only], |r|r.get(0)).optional()?;
            let Some(body) = body else {
                tx.commit()?;
                return Ok(None);
            };
            let mut work: WorkItem = serde_json::from_str(&body)?;
            if work.attempts >= 3 {
                work.status = WorkItemStatus::Failed;
                tx.execute(
                    "UPDATE work SET status='failed',body=?2 WHERE id=?1",
                    params![work.id, serde_json::to_string(&work)?],
                )?;
                continue;
            }
            work.status = WorkItemStatus::Running;
            work.attempts += 1;
            work.owner = owner.into();
            work.lease_until_ms = (now_ms() + u64::from(lease_ms))
                .min(counter(&grant.budget.deadline_ms)?)
                .to_string();
            tx.execute(
                "UPDATE work SET status='running',lease_until_ms=?2,body=?3 WHERE id=?1",
                params![
                    work.id,
                    counter(&work.lease_until_ms)? as i64,
                    serde_json::to_string(&work)?
                ],
            )?;
            tx.commit()?;
            return Ok(Some(work));
        }
    }

    pub fn finish_work(&self, work_id: &str, owner: &str, status: WorkItemStatus) -> Result<()> {
        if matches!(status, WorkItemStatus::Running | WorkItemStatus::Queued) {
            return Err(Error::invalid("completion must be terminal"));
        }
        let tx = self.write_transaction()?;
        let body: String =
            tx.query_row("SELECT body FROM work WHERE id=?1", [work_id], |r| r.get(0))?;
        let mut work: WorkItem = serde_json::from_str(&body)?;
        if work.owner != owner {
            return Err(Error::denied("work lease belongs to another owner"));
        }
        if work.status == status {
            return Ok(());
        }
        // A deadline can expire while the last effect settles. Accept that final
        // status only while this owner is still recorded; reclamation above
        // changes the owner atomically and prevents an obsolete completion.
        if work.status != WorkItemStatus::Running {
            return Err(Error::conflict("work already completed"));
        }
        work.status = status;
        tx.execute(
            "UPDATE work SET status=?2,body=?3 WHERE id=?1",
            params![
                work_id,
                serde_json::to_value(&work.status)?.as_str(),
                serde_json::to_string(&work)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn renew_work(&self, work_id: &str, owner: &str, lease_ms: u32) -> Result<()> {
        if lease_ms == 0 || lease_ms > 300000 {
            return Err(Error::invalid("invalid lease"));
        }
        let tx = self.write_transaction()?;
        let (body, grant_id): (String, String) = tx.query_row(
            "SELECT body,grant_id FROM work WHERE id=?1",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut work: WorkItem = serde_json::from_str(&body)?;
        if work.owner != owner
            || work.status != WorkItemStatus::Running
            || counter(&work.lease_until_ms)? < now_ms()
        {
            return Err(Error::conflict("work lease unavailable"));
        }
        let deadline = counter(&self.grant(&grant_id)?.budget.deadline_ms)?;
        work.lease_until_ms = (now_ms() + u64::from(lease_ms)).min(deadline).to_string();
        tx.execute(
            "UPDATE work SET lease_until_ms=?2,body=?3 WHERE id=?1",
            params![
                work_id,
                counter(&work.lease_until_ms)? as i64,
                serde_json::to_string(&work)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}
