use crate::{
    error::{Error, Result},
    store::Store,
    validation::counter,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Copy)]
pub(crate) enum TimingPhase {
    WorkerQueue,
    WorkerStartup,
    ExecutorQueue,
    BlockingQueue,
    ExecutorService,
    StateQueue,
    StateService,
    SqliteWriteBegin,
    ChildWait,
}

impl TimingPhase {
    fn name(self) -> &'static str {
        match self {
            Self::WorkerQueue => "worker_queue",
            Self::WorkerStartup => "worker_startup",
            Self::ExecutorQueue => "executor_queue",
            Self::BlockingQueue => "blocking_queue",
            Self::ExecutorService => "executor_service",
            Self::StateQueue => "state_queue",
            Self::StateService => "state_service",
            Self::SqliteWriteBegin => "sqlite_write_begin",
            Self::ChildWait => "child_wait",
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Aggregate {
    samples: u32,
    total_us: String,
    max_us: String,
}

impl Store {
    pub(crate) fn record_timings(
        &self,
        run: &str,
        samples: &[(TimingPhase, Duration)],
    ) -> Result<()> {
        let tx = self.write_transaction()?;
        let body: String =
            tx.query_row("SELECT timings FROM runs WHERE id=?1", [run], |r| r.get(0))?;
        let mut timings: BTreeMap<String, Aggregate> = serde_json::from_str(&body)?;
        for (phase, duration) in samples {
            let micros = u64::try_from(duration.as_micros())
                .map_err(|_| Error::invalid("timing duration overflow"))?;
            let aggregate = timings
                .entry(phase.name().into())
                .or_insert_with(|| Aggregate {
                    samples: 0,
                    total_us: "0".into(),
                    max_us: "0".into(),
                });
            aggregate.samples = aggregate
                .samples
                .checked_add(1)
                .ok_or_else(|| Error::invalid("timing sample count overflow"))?;
            aggregate.total_us = counter(&aggregate.total_us)?
                .checked_add(micros)
                .ok_or_else(|| Error::invalid("timing total overflow"))?
                .to_string();
            aggregate.max_us = counter(&aggregate.max_us)?.max(micros).to_string();
        }
        tx.execute(
            "UPDATE runs SET timings=?2 WHERE id=?1",
            params![run, serde_json::to_string(&timings)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn observe_timings(&self, run: &str, samples: &[(TimingPhase, Duration)]) {
        // A missing performance sample must not replace an authoritative tool
        // result or cause a completed external effect to be retried.
        if let Err(error) = self.record_timings(run, samples) {
            eprintln!("Run timing could not be persisted: {}", error.message);
        }
    }

    pub(crate) fn inspect_timings(&self, run: &str) -> Result<Value> {
        let body: String =
            self.db
                .query_row("SELECT timings FROM runs WHERE id=?1", [run], |r| r.get(0))?;
        let (observed, elapsed, pending, missing, skew): (u32,i64,u32,u32,u32) = self.db.query_row(
            "SELECT coalesce(sum(dispatched_ms IS NOT NULL AND observed_ms IS NOT NULL),0),
                coalesce(sum(CASE WHEN dispatched_ms IS NOT NULL AND observed_ms IS NOT NULL THEN max(CAST(observed_ms AS INTEGER)-CAST(dispatched_ms AS INTEGER),0) ELSE 0 END),0),
                coalesce(sum(dispatched_ms IS NOT NULL AND observed_ms IS NULL AND state='dispatched'),0),
                coalesce(sum(dispatched_ms IS NULL AND state NOT IN ('reserved','released')),0),
                coalesce(sum(dispatched_ms IS NOT NULL AND observed_ms IS NOT NULL AND CAST(observed_ms AS INTEGER)<CAST(dispatched_ms AS INTEGER)),0)
             FROM permits WHERE run_id=?1", [run], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        )?;
        Ok(
            json!({"spans":serde_json::from_str::<Value>(&body)?, "provider_round_trip":{"observed_calls":observed,"total_ms":elapsed.to_string(),"unobserved_dispatched_calls":pending,"missing_dispatch_time_calls":missing,"clock_reversals":skew}}),
        )
    }
}

#[cfg(test)]
#[path = "timing_tests.rs"]
mod tests;
