use crate::{
    allocations::allocation_id,
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn permit(&self, run_id: &str, request: &PermitRequest) -> Result<Permit> {
        self.permit_in_allocation(run_id, request, None)
    }

    pub(crate) fn permit_in_allocation(
        &self,
        run_id: &str,
        request: &PermitRequest,
        allocation: Option<&str>,
    ) -> Result<Permit> {
        validate("PermitRequest", &serde_json::to_value(request)?)?;
        let grant = self.require_active(run_id)?;
        let reserved = counter(&request.input_tokens_bound)?
            .checked_add(u64::from(request.max_output_tokens))
            .ok_or_else(|| Error::invalid("token bound overflow"))?;
        let cost = counter(&request.cost_microusd_bound)?;
        if reserved > i64::MAX as u64 || cost > i64::MAX as u64 {
            return Err(Error::invalid("reservation exceeds local accounting range"));
        }
        let tx = self.write_transaction()?;
        let run_allocation = self.run_allocation_in(run_id)?;
        let mut allocation = allocation.unwrap_or(&run_allocation.id).to_owned();
        if let Some(summary) = &request.compaction_id {
            self.require_compaction(run_id, summary)?;
            let parent = self.allocation(&grant, &allocation)?;
            allocation = self
                .allocate_in(
                    &grant,
                    &BudgetAllocationRequest {
                        id: allocation_id("compaction", &format!("{run_id}:{summary}")),
                        parent_id: parent.id,
                        cause_id: summary.clone(),
                        purpose: "compaction".into(),
                        budget: parent.budget,
                    },
                )?
                .id;
        }
        if let Some(call) = &request.call_id {
            let previous: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT id,request,allocation_id FROM permits WHERE run_id=?1 AND call_id=?2",
                    params![run_id, call],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some((id, body, owner)) = previous {
                if body != serde_json::to_string(request)? || owner != allocation {
                    return Err(Error::conflict(
                        "provider call identity or allocation changed",
                    ));
                }
                return Ok(Permit {
                    id,
                    max_output_tokens: request.max_output_tokens,
                });
            }
        }
        let ancestry = self.require_allocation_open(&grant, &allocation)?;
        if !ancestry.iter().any(|a| a.id == run_allocation.id) {
            return Err(Error::denied("provider allocation is outside this run"));
        }
        for owner in &ancestry {
            let status = self.allocation_ledger_status(&self.grant(&owner.grant_id)?, &owner.id)?;
            if status.remaining.max_calls == 0
                || reserved > counter(&status.remaining.max_tokens)?
                || cost > counter(&status.remaining.max_cost_microusd)?
            {
                return Err(Error::exhausted("root or child model budget exhausted"));
            }
        }
        let permit = Permit {
            id: id(),
            max_output_tokens: request.max_output_tokens,
        };
        // Legacy callers cannot prove that transport has not started. Stable
        // call IDs opt into explicit dispatch and safe undispatched release.
        let state = if request.call_id.is_some() {
            "reserved"
        } else {
            "dispatched"
        };
        tx.execute("INSERT INTO permits(id,grant_id,run_id,reserved_tokens,reserved_cost,compaction_id,allocation_id,call_id,request,state) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![permit.id,grant.id,run_id,reserved as i64,cost as i64,request.compaction_id,allocation,request.call_id,serde_json::to_string(request)?,state])?;
        tx.commit()?;
        Ok(permit)
    }

    pub fn lookup_permit(&self, run: &str, call: &str) -> Result<Permit> {
        let (id, request): (String, String) = self
            .db
            .query_row(
                "SELECT id,request FROM permits WHERE run_id=?1 AND call_id=?2",
                params![run, call],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("provider call not found for run"))?;
        let request: PermitRequest = serde_json::from_str(&request)?;
        Ok(Permit {
            id,
            max_output_tokens: request.max_output_tokens,
        })
    }

    pub fn dispatch_permit(&self, run: &str, permit: &str) -> Result<()> {
        let tx = self.write_transaction()?;
        let grant = self.require_active(run)?;
        let allocation: Option<String> = tx
            .query_row(
                "SELECT allocation_id FROM permits WHERE id=?1 AND run_id=?2 AND state='reserved'",
                params![permit, run],
                |r| r.get(0),
            )
            .optional()?;
        let allocation = allocation.ok_or_else(|| {
            Error::conflict("permit is not undispatched; do not repeat provider transport")
        })?;
        self.require_allocation_open(&grant, &allocation)?;
        tx.execute(
            "UPDATE permits SET state='dispatched',dispatched_ms=?2 WHERE id=?1",
            params![permit, now_ms().to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn release_permit(&self, run: &str, permit: &str) -> Result<()> {
        let tx = self.write_transaction()?;
        let state: String = tx
            .query_row(
                "SELECT state FROM permits WHERE id=?1 AND run_id=?2",
                params![permit, run],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::missing("permit not found for run"))?;
        if state != "reserved" && state != "released" {
            return Err(Error::conflict(
                "dispatched or settled usage cannot be released",
            ));
        }
        tx.execute("UPDATE permits SET state='released' WHERE id=?1", [permit])?;
        tx.commit()?;
        Ok(())
    }

    pub fn usage(&self, run_id: &str, usage: &Usage) -> Result<()> {
        validate("Usage", &serde_json::to_value(usage)?)?;
        let tx = self.write_transaction()?;
        let (previous, state): (Option<String>, String) = tx
            .query_row(
                "SELECT usage,state FROM permits WHERE id=?1 AND run_id=?2",
                params![usage.permit_id, run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("permit not found for run"))?;
        let body = serde_json::to_string(usage)?;
        if let Some(previous) = previous {
            return if previous == body {
                Ok(())
            } else {
                Err(Error::conflict("usage already recorded"))
            };
        }
        if state != "dispatched" {
            return Err(Error::conflict("usage requires a dispatched permit"));
        }
        let tokens = counter(&usage.input_tokens)?
            .checked_add(counter(&usage.output_tokens)?)
            .ok_or_else(|| Error::invalid("usage overflow"))?;
        let cost = counter(&usage.cost_microusd)?;
        if tokens > i64::MAX as u64 || cost > i64::MAX as u64 {
            return Err(Error::invalid("usage exceeds local accounting range"));
        }
        if usage.complete {
            tx.execute("UPDATE permits SET usage=?2,reserved_tokens=?3,reserved_cost=?4,state='settled',observed_ms=?5 WHERE id=?1", params![usage.permit_id,body,tokens as i64,cost as i64,now_ms().to_string()])?;
        } else {
            tx.execute(
                "UPDATE permits SET usage=?2,observed_ms=?3 WHERE id=?1",
                params![usage.permit_id, body, now_ms().to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
