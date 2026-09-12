use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, validate},
};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn permit(&self, run_id: &str, request: &PermitRequest) -> Result<Permit> {
        let grant = self.require_active(run_id)?;
        validate("PermitRequest", &serde_json::to_value(request)?)?;
        let reserved = counter(&request.input_tokens_bound)?
            .checked_add(u64::from(request.max_output_tokens))
            .ok_or_else(|| Error::invalid("token bound overflow"))?;
        let cost = counter(&request.cost_microusd_bound)?;
        if reserved > i64::MAX as u64 || cost > i64::MAX as u64 {
            return Err(Error::invalid("reservation exceeds local accounting range"));
        }
        let tx = self.write_transaction()?;
        let (calls,tokens,used_cost):(u32,i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(reserved_tokens),0),coalesce(sum(reserved_cost),0) FROM permits WHERE grant_id=?1",[&grant.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if calls >= grant.budget.max_calls
            || reserved > counter(&grant.budget.max_tokens)?.saturating_sub(tokens as u64)
            || cost > counter(&grant.budget.max_cost_microusd)?.saturating_sub(used_cost as u64)
        {
            return Err(Error::exhausted("root model budget exhausted"));
        }
        let permit = Permit {
            id: id(),
            max_output_tokens: request.max_output_tokens,
        };
        tx.execute("INSERT INTO permits(id,grant_id,run_id,reserved_tokens,reserved_cost) VALUES (?1,?2,?3,?4,?5)",params![permit.id,grant.id,run_id,reserved as i64,cost as i64])?;
        tx.commit()?;
        Ok(permit)
    }

    pub fn usage(&self, run_id: &str, usage: &Usage) -> Result<()> {
        validate("Usage", &serde_json::to_value(usage)?)?;
        let previous: Option<String> = self
            .db
            .query_row(
                "SELECT usage FROM permits WHERE id=?1 AND run_id=?2",
                params![usage.permit_id, run_id],
                |r| r.get(0),
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
        let tokens = counter(&usage.input_tokens)?
            .checked_add(counter(&usage.output_tokens)?)
            .ok_or_else(|| Error::invalid("usage overflow"))?;
        let cost = counter(&usage.cost_microusd)?;
        if tokens > i64::MAX as u64 || cost > i64::MAX as u64 {
            return Err(Error::invalid("usage exceeds local accounting range"));
        }
        if usage.complete {
            self.db.execute(
                "UPDATE permits SET usage=?2,reserved_tokens=?3,reserved_cost=?4 WHERE id=?1",
                params![usage.permit_id, body, tokens as i64, cost as i64],
            )?;
        } else {
            self.db.execute(
                "UPDATE permits SET usage=?2 WHERE id=?1",
                params![usage.permit_id, body],
            )?;
        }
        Ok(())
    }
}
