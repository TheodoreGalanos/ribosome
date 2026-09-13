use crate::{
    contracts::*,
    error::{Error, Result},
    host::hash,
    store::Store,
    validation::{counter, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};

pub(crate) fn allocation_id(kind: &str, cause: &str) -> String {
    format!("{kind}:{}", hash(cause.as_bytes()))
}

impl Store {
    pub fn root_budget_status(&self, grant: &Grant) -> Result<BudgetStatus> {
        if self.grant(&grant.id)? != *grant {
            return Err(Error::denied(
                "budget inspection requires the stored owner grant",
            ));
        }
        let tx = self.write_transaction()?;
        let root = self.root_allocation_in(grant)?;
        let status = self.budget_status(grant, &root.id)?;
        tx.commit()?;
        Ok(status)
    }

    pub(crate) fn root_allocation_in(&self, grant: &Grant) -> Result<BudgetAllocation> {
        let root = BudgetAllocation {
            id: allocation_id("root", &grant.id),
            grant_id: grant.id.clone(),
            parent_id: None,
            cause_id: grant.id.clone(),
            purpose: "root".into(),
            budget: grant.budget.clone(),
            disposition: None,
        };
        self.db.execute("INSERT OR IGNORE INTO budget_allocations(id,grant_id,parent_id,body) VALUES (?1,?2,NULL,?3)", params![root.id,grant.id,serde_json::to_string(&root)?])?;
        self.allocation(grant, &root.id)
    }

    pub fn allocation(&self, grant: &Grant, id: &str) -> Result<BudgetAllocation> {
        let body: String = self
            .db
            .query_row(
                "SELECT body FROM budget_allocations WHERE id=?1 AND grant_id=?2",
                params![id, grant.id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::missing("allocation not found in grant"))?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Host-owned allocation creation. Workers cannot select another scope's
    /// funding source through a provider permit payload.
    pub fn allocate(
        &self,
        grant: &Grant,
        request: &BudgetAllocationRequest,
    ) -> Result<BudgetAllocation> {
        if self.grant(&grant.id)? != *grant {
            return Err(Error::denied("allocation requires the stored owner grant"));
        }
        let tx = self.write_transaction()?;
        let result = self.allocate_in(grant, request)?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn allocate_in(
        &self,
        grant: &Grant,
        request: &BudgetAllocationRequest,
    ) -> Result<BudgetAllocation> {
        validate("BudgetAllocationRequest", &serde_json::to_value(request)?)?;
        let parent = self.allocation(grant, &request.parent_id)?;
        let allocation = BudgetAllocation {
            id: request.id.clone(),
            grant_id: grant.id.clone(),
            parent_id: Some(parent.id.clone()),
            cause_id: request.cause_id.clone(),
            purpose: request.purpose.clone(),
            budget: request.budget.clone(),
            disposition: None,
        };
        if let Some(body) = self
            .db
            .query_row(
                "SELECT body FROM budget_allocations WHERE id=?1",
                [&request.id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            let existing: BudgetAllocation = serde_json::from_str(&body)?;
            let mut identity = existing.clone();
            identity.disposition = None;
            return if identity == allocation {
                Ok(existing)
            } else {
                Err(Error::conflict("allocation identity or ceilings changed"))
            };
        }
        self.require_allocation_open(grant, &parent.id)?;
        if self.allocation_ancestry(grant, &parent.id)?.len() >= 64 {
            return Err(Error::invalid("allocation ancestry exceeds 64 levels"));
        }
        let budget = &request.budget;
        let ceiling = &parent.budget;
        if budget.max_calls > ceiling.max_calls
            || budget.max_actions > ceiling.max_actions
            || budget.max_work_items > ceiling.max_work_items
            || budget.max_depth > ceiling.max_depth
            || counter(&budget.max_tokens)? > counter(&ceiling.max_tokens)?
            || counter(&budget.max_cost_microusd)? > counter(&ceiling.max_cost_microusd)?
            || counter(&budget.deadline_ms)? > counter(&ceiling.deadline_ms)?
        {
            return Err(Error::denied("child allocation exceeds parent ceilings"));
        }
        self.db.execute(
            "INSERT INTO budget_allocations(id,grant_id,parent_id,body) VALUES (?1,?2,?3,?4)",
            params![
                allocation.id,
                grant.id,
                parent.id,
                serde_json::to_string(&allocation)?
            ],
        )?;
        Ok(allocation)
    }

    pub(crate) fn bind_run_allocation_in(
        &self,
        grant: &Grant,
        request: &AgentRunRequest,
    ) -> Result<()> {
        let existing: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM run_allocations WHERE run_id=?1)",
            [&request.run_id],
            |r| r.get(0),
        )?;
        if existing {
            self.db.execute("UPDATE budget_allocations SET body=json_remove(body,'$.disposition') WHERE id=(SELECT allocation_id FROM run_allocations WHERE run_id=?1)", [&request.run_id])?;
            return Ok(());
        }
        let root = self.root_allocation_in(grant)?;
        let work_parent: Option<String> = self
            .db
            .query_row(
                "SELECT allocation_id FROM work WHERE id=?1",
                [&request.run_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if request.parent_allocation_id.is_some()
            && work_parent.is_some()
            && request.parent_allocation_id != work_parent
        {
            return Err(Error::denied("scheduled work cannot change its allocation"));
        }
        let parent = request
            .parent_allocation_id
            .clone()
            .or(work_parent)
            .unwrap_or(root.id);
        let parent = self.allocation(grant, &parent)?;
        let allocation = self.allocate_in(
            grant,
            &BudgetAllocationRequest {
                id: allocation_id("run", &request.run_id),
                parent_id: parent.id,
                cause_id: request.run_id.clone(),
                purpose: serde_json::to_value(&request.profile)?
                    .as_str()
                    .unwrap()
                    .into(),
                budget: parent.budget,
            },
        )?;
        self.db.execute(
            "INSERT INTO run_allocations(run_id,allocation_id) VALUES (?1,?2)",
            params![request.run_id, allocation.id],
        )?;
        Ok(())
    }

    pub(crate) fn run_allocation_in(&self, run: &str) -> Result<BudgetAllocation> {
        let grant = self.run_grant(run)?;
        let allocation: Option<String> = self
            .db
            .query_row(
                "SELECT allocation_id FROM run_allocations WHERE run_id=?1",
                [run],
                |row| row.get(0),
            )
            .optional()?;
        match allocation {
            Some(id) => self.allocation(&grant, &id),
            None => self.root_allocation_in(&grant),
        }
    }

    pub(crate) fn allocation_ancestry(
        &self,
        grant: &Grant,
        id: &str,
    ) -> Result<Vec<BudgetAllocation>> {
        let mut result = vec![];
        let mut next = Some(id.to_owned());
        while let Some(id) = next {
            if result.len() >= 64 {
                return Err(Error::invalid("allocation ancestry exceeds 64 levels"));
            }
            let allocation = if result.is_empty() {
                self.allocation(grant, &id)?
            } else {
                // Only host-created parent edges may cross grants. The initial
                // allocation remains capability checked against the caller.
                let body: String = self.db.query_row(
                    "SELECT body FROM budget_allocations WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                serde_json::from_str(&body)?
            };
            next = allocation.parent_id.clone();
            result.push(allocation);
        }
        Ok(result)
    }

    pub(crate) fn require_allocation_open(
        &self,
        grant: &Grant,
        id: &str,
    ) -> Result<Vec<BudgetAllocation>> {
        let ancestry = self.allocation_ancestry(grant, id)?;
        for owner in &ancestry {
            // Successful parents may leave already-requested work behind.
            // Stopped parents deny all new dispatch through their descendants.
            if (owner.id == id && owner.disposition.is_some())
                || matches!(
                    owner.disposition,
                    Some(Disposition::Cancelled | Disposition::Exhausted | Disposition::Failed)
                )
            {
                return Err(Error::denied("budget allocation is closed"));
            }
            if now_ms() >= counter(&owner.budget.deadline_ms)? {
                return Err(Error::exhausted("budget allocation expired"));
            }
        }
        Ok(ancestry)
    }

    pub fn budget_status(&self, grant: &Grant, id: &str) -> Result<BudgetStatus> {
        let mut status = self.allocation_ledger_status(grant, id)?;
        status.remaining = self.allocation_remaining(grant, id)?;
        Ok(status)
    }

    /// Counts this subtree once. The public remaining allowance also includes
    /// spending in sibling allocations through their common ancestors.
    pub(crate) fn allocation_ledger_status(&self, grant: &Grant, id: &str) -> Result<BudgetStatus> {
        let allocation = self.allocation(grant, id)?;
        let root = allocation.parent_id.is_none();
        let (calls,undispatched,unknown,settled_tokens,reserved_tokens,settled_cost,reserved_cost): (u32,u32,u32,i64,i64,i64,i64) = self.db.query_row(
            "WITH RECURSIVE family(id) AS (SELECT ?1 UNION ALL SELECT a.id FROM budget_allocations a JOIN family f ON a.parent_id=f.id)
             SELECT count(*),coalesce(sum(state='reserved'),0),coalesce(sum(state='dispatched'),0),
               coalesce(sum(CASE WHEN state='settled' THEN reserved_tokens ELSE 0 END),0),
               coalesce(sum(CASE WHEN state!='settled' THEN reserved_tokens ELSE 0 END),0),
               coalesce(sum(CASE WHEN state='settled' THEN reserved_cost ELSE 0 END),0),
               coalesce(sum(CASE WHEN state!='settled' THEN reserved_cost ELSE 0 END),0)
             FROM permits WHERE ((grant_id=?2 AND ?3) OR allocation_id IN family) AND state!='released'",
            params![id,grant.id,root], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)),
        )?;
        let count = |table: &str| -> Result<u32> {
            Ok(self.db.query_row(&format!("WITH RECURSIVE family(id) AS (SELECT ?1 UNION ALL SELECT a.id FROM budget_allocations a JOIN family f ON a.parent_id=f.id) SELECT count(*) FROM {table} WHERE ((grant_id=?2 AND ?3) OR allocation_id IN family)"), params![id,grant.id,root], |r| r.get(0))?)
        };
        let actions = count("effects")?;
        let work_items = count("work")?;
        let mut remaining = allocation.budget.clone();
        remaining.max_calls = remaining.max_calls.saturating_sub(calls);
        remaining.max_actions = remaining.max_actions.saturating_sub(actions);
        remaining.max_work_items = remaining.max_work_items.saturating_sub(work_items);
        remaining.max_tokens = counter(&remaining.max_tokens)?
            .saturating_sub(settled_tokens as u64)
            .saturating_sub(reserved_tokens as u64)
            .to_string();
        remaining.max_cost_microusd = counter(&remaining.max_cost_microusd)?
            .saturating_sub(settled_cost as u64)
            .saturating_sub(reserved_cost as u64)
            .to_string();
        Ok(BudgetStatus {
            allocation,
            remaining,
            usage: BudgetUsage {
                model_calls: calls,
                undispatched_calls: undispatched,
                unknown_calls: unknown,
                settled_tokens: settled_tokens.to_string(),
                reserved_tokens: reserved_tokens.to_string(),
                settled_cost_microusd: settled_cost.to_string(),
                reserved_cost_microusd: reserved_cost.to_string(),
                actions,
                work_items,
            },
        })
    }

    pub fn run_budget_status(&self, run: &str) -> Result<BudgetStatus> {
        let tx = self.write_transaction()?;
        let allocation = self.run_allocation_in(run)?;
        let grant = self.run_grant(run)?;
        let status = self.budget_status(&grant, &allocation.id)?;
        tx.commit()?;
        Ok(status)
    }

    pub(crate) fn allocation_remaining(&self, grant: &Grant, id: &str) -> Result<Budget> {
        let mut remaining = self.allocation(grant, id)?.budget;
        for owner in self.allocation_ancestry(grant, id)? {
            let other = self
                .allocation_ledger_status(&self.grant(&owner.grant_id)?, &owner.id)?
                .remaining;
            remaining.max_calls = remaining.max_calls.min(other.max_calls);
            remaining.max_actions = remaining.max_actions.min(other.max_actions);
            remaining.max_work_items = remaining.max_work_items.min(other.max_work_items);
            remaining.max_depth = remaining.max_depth.min(other.max_depth);
            remaining.max_tokens = counter(&remaining.max_tokens)?
                .min(counter(&other.max_tokens)?)
                .to_string();
            remaining.max_cost_microusd = counter(&remaining.max_cost_microusd)?
                .min(counter(&other.max_cost_microusd)?)
                .to_string();
            remaining.deadline_ms = counter(&remaining.deadline_ms)?
                .min(counter(&other.deadline_ms)?)
                .to_string();
        }
        Ok(remaining)
    }

    pub(crate) fn finish_allocation(
        &self,
        grant: &Grant,
        id: &str,
        disposition: Disposition,
    ) -> Result<()> {
        let tx = self.write_transaction()?;
        let mut allocation = self.allocation(grant, id)?;
        if let Some(existing) = &allocation.disposition {
            return if *existing == disposition {
                Ok(())
            } else {
                Err(Error::conflict("allocation already finished"))
            };
        }
        allocation.disposition = Some(disposition);
        self.db.execute(
            "UPDATE budget_allocations SET body=?2 WHERE id=?1",
            params![id, serde_json::to_string(&allocation)?],
        )?;
        tx.commit()?;
        Ok(())
    }
}
