use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::validate,
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::collections::HashSet;

fn terminal(status: &WorkItemStatus) -> bool {
    !matches!(
        status,
        WorkItemStatus::Queued | WorkItemStatus::Running | WorkItemStatus::Interrupted
    )
}

impl Store {
    fn child_work(&self, parent: &str, id: &str) -> Result<WorkItem> {
        let grant = self.run_grant(parent)?;
        let body: Option<String> = self.db.query_row(
            "SELECT body FROM work WHERE id=?1 AND grant_id=?2 AND json_extract(body,'$.parent_id')=?3",
            params![id, grant.id, parent], |r| r.get(0),
        ).optional()?;
        serde_json::from_str(&body.ok_or_else(|| {
            Error::denied("waiting and status require this run's direct child work")
        })?)
        .map_err(Into::into)
    }

    pub fn wait_for_work(&self, run: &str, request: &WorkWaitRequest) -> Result<WorkWaitResult> {
        self.require_active(run)?;
        self.work_wait_status(run, request)
    }

    pub(crate) fn work_wait_status(
        &self,
        run: &str,
        request: &WorkWaitRequest,
    ) -> Result<WorkWaitResult> {
        validate("WorkWaitRequest", &serde_json::to_value(request)?)?;
        let mut waiting = false;
        for id in &request.work_ids {
            waiting |= !terminal(&self.child_work(run, id)?.status);
        }
        Ok(WorkWaitResult {
            work_ids: request.work_ids.clone(),
            wait_required: waiting,
        })
    }

    pub fn work_status(&self, run: &str, id: &str) -> Result<WorkStatus> {
        let work = self.child_work(run, id)?;
        let mut status = WorkStatus {
            work_id: id.into(),
            status: work.status,
            result_available: false,
            result: None,
            source: None,
        };
        let saved: Option<(Option<String>, Option<String>)> = self
            .db
            .query_row(
                "SELECT result,result_source FROM runs WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((Some(result), Some(source))) = saved {
            let grant = self.run_grant(run)?;
            if self.source_available(&grant, "event", &source)? {
                status.result = Some(serde_json::from_str(&result)?);
                status.result_available = true;
                status.source = Some(ContextSource {
                    kind: ContextSourceKind::Event,
                    version: self.db.query_row(
                        "SELECT sequence FROM events WHERE id=?1",
                        [&source],
                        |r| r.get(0),
                    )?,
                    id: source,
                });
            }
        }
        Ok(status)
    }

    /// Only the supervisor may accept this handoff, after all issued RPCs settle.
    pub(crate) fn park_run(&self, run: &str, request: &SessionParkRequest) -> Result<()> {
        validate("SessionParkRequest", &serde_json::to_value(request)?)?;
        let tx = self.write_transaction()?;
        let grant = self.require_active(run)?;
        if self.run_attachment(run)?.is_some_and(|a| {
            a.handoff
                .is_some_and(|h| h.state != RepairHandoffState::Released)
        }) {
            return Err(Error::denied(
                "coordinated repair must finish within its current work item",
            ));
        }
        self.work_wait_status(
            run,
            &WorkWaitRequest {
                work_ids: request.work_ids.clone(),
            },
        )?;
        let checkpoint = &request.checkpoint;
        if checkpoint.format != "pi-0.85.1/2" || !checkpoint.pending_operations.is_empty() {
            return Err(Error::denied(
                "parking requires a bounded checkpoint without pending effects",
            ));
        }
        self.checkpoint(run, checkpoint)?;
        let context = checkpoint
            .context
            .as_ref()
            .ok_or_else(|| Error::invalid("parking requires retained context"))?;
        if !self.context_available(&grant, &context.segment_id)? {
            return Err(Error::denied("parking context is no longer authorized"));
        }
        let unsettled: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM effects WHERE run_id=?1 AND (phase!='finalized' OR (json_extract(body,'$.status')='unknown' AND settlement IS NULL)))", [run], |r| r.get(0),
        )?;
        if unsettled {
            return Err(Error::conflict("issued effects must settle before parking"));
        }
        let assistant: Option<(i64,String)> = tx.query_row(
            "SELECT sequence,body FROM context_items WHERE segment_id=?1 AND json_extract(body,'$.role')='assistant' ORDER BY sequence DESC LIMIT 1", [&context.segment_id], |r| Ok((r.get(0)?,r.get(1)?)),
        ).optional()?;
        let (sequence, body) = assistant
            .ok_or_else(|| Error::invalid("parking requires a completed tool exchange"))?;
        let assistant: Value = serde_json::from_str(&body)?;
        let completed: HashSet<String> = tx.prepare("SELECT json_extract(body,'$.toolCallId') FROM context_items WHERE segment_id=?1 AND sequence>?2 AND json_extract(body,'$.role')='toolResult'")?
            .query_map(params![context.segment_id,sequence], |r| r.get(0))?.collect::<std::result::Result<_,_>>()?;
        for part in assistant["content"]
            .as_array()
            .ok_or_else(|| Error::invalid("assistant content is not a tool exchange"))?
        {
            if part["type"] == "toolCall"
                && !part["id"].as_str().is_some_and(|id| completed.contains(id))
            {
                return Err(Error::conflict(
                    "parking cannot discard an unfinished tool exchange",
                ));
            }
        }
        tx.execute(
            "INSERT INTO run_waits(run_id,work_ids) VALUES(?1,?2)",
            params![run, serde_json::to_string(&request.work_ids)?],
        )?;
        tx.execute("UPDATE runs SET status='waiting' WHERE id=?1", [run])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn waiting_work(&self, run: &str) -> Result<Option<WorkWaitRequest>> {
        let ids: Option<String> = self
            .db
            .query_row(
                "SELECT work_ids FROM run_waits WHERE run_id=?1",
                [run],
                |r| r.get(0),
            )
            .optional()?;
        ids.map(|s| {
            Ok(WorkWaitRequest {
                work_ids: serde_json::from_str(&s)?,
            })
        })
        .transpose()
    }

    pub(crate) fn resume_waiting(&self, run: &str) -> Result<()> {
        let tx = self.write_transaction()?;
        let wait = self
            .waiting_work(run)?
            .ok_or_else(|| Error::conflict("run has no waiting continuation"))?;
        if self.work_wait_status(run, &wait)?.wait_required {
            return Err(Error::conflict("child work has not settled"));
        }
        tx.execute("DELETE FROM run_waits WHERE run_id=?1", [run])?;
        tx.execute(
            "UPDATE runs SET status='running' WHERE id=?1 AND status='waiting'",
            [run],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn work_source(&self, grant: &Grant, work: &str) -> Result<Option<ContextSource>> {
        let source: Option<Option<String>> = self
            .db
            .query_row(
                "SELECT source_ref FROM work WHERE id=?1 AND grant_id=?2",
                params![work, grant.id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(source) = source else {
            return Ok(None);
        };
        let source = source.ok_or_else(|| {
            Error::denied("legacy work has no verified task lineage; owner review is required")
        })?;
        self.require_source(grant, "event", &source)?;
        Ok(Some(ContextSource {
            kind: ContextSourceKind::Event,
            version: self.db.query_row(
                "SELECT sequence FROM events WHERE id=?1",
                [&source],
                |r| r.get(0),
            )?,
            id: source,
        }))
    }

    pub(crate) fn stop_waiting_children(
        &self,
        run: &str,
        disposition: &Disposition,
    ) -> Result<Vec<String>> {
        if !matches!(disposition, Disposition::Cancelled | Disposition::Exhausted) {
            return Err(Error::invalid(
                "child stopping requires cancellation or exhaustion",
            ));
        }
        let Some(wait) = self.waiting_work(run)? else {
            return Ok(vec![]);
        };
        let tx = self.write_transaction()?;
        let status = serde_json::to_value(disposition)?;
        let status = status.as_str().unwrap();
        let mut running = vec![];
        for id in wait.work_ids {
            let child = self.child_work(run, &id)?;
            if terminal(&child.status) {
                continue;
            }
            // Close the allowance first. The actual owner still settles issued
            // work; changing an allocation is not evidence that an effect stopped.
            tx.execute("UPDATE budget_allocations SET body=json_set(body,'$.disposition',?2) WHERE id=(SELECT allocation_id FROM work WHERE id=?1)", params![id,status])?;
            if matches!(
                child.status,
                WorkItemStatus::Queued | WorkItemStatus::Interrupted
            ) {
                tx.execute(
                    "UPDATE work SET status=?2,body=json_set(body,'$.status',?2) WHERE id=?1",
                    params![id, status],
                )?;
            } else {
                running.push(id);
            }
        }
        tx.commit()?;
        Ok(running)
    }
}
