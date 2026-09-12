use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, now_ms},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Mechanical host routing. Kinds and thresholds do not diagnose the events.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub id: String,
    pub grant_id: String,
    pub kinds: Vec<String>,
    pub profile: Profile,
    pub operator: String,
    pub subject: String,
    pub batch_size: u32,
    pub max_delay_ms: u32,
}

impl Store {
    pub fn subscribe(&self, subscription: &Subscription) -> Result<()> {
        self.subscribe_from(subscription, None, "0", None)
    }

    pub(crate) fn subscribe_from(
        &self,
        subscription: &Subscription,
        source_run: Option<&str>,
        cursor: &str,
        attachment_id: Option<&str>,
    ) -> Result<()> {
        counter(cursor)?;
        let grant = self.grant(&subscription.grant_id)?;
        if subscription.id.is_empty()
            || subscription.subject.len() > 100
            || subscription.kinds.is_empty()
            || !(1..=100).contains(&subscription.batch_size)
            || !(1..=300000).contains(&subscription.max_delay_ms)
            || !grant.profiles.contains(&subscription.profile)
        {
            return Err(Error::invalid("invalid subscription or profile grant"));
        }
        let body = serde_json::to_string(subscription)?;
        let previous: Option<String> = self
            .db
            .query_row(
                "SELECT body FROM subscriptions WHERE id=?1",
                [&subscription.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            if previous != body {
                return Err(Error::conflict("subscription IDs are immutable"));
            }
            return Ok(());
        }
        self.db.execute("INSERT INTO subscriptions(id,body,cursor,pending,first_pending_ms,source_run,attachment_id) VALUES (?1,?2,?3,'[]','0',?4,?5)",params![subscription.id,body,cursor,source_run,attachment_id])?;
        Ok(())
    }

    /// Hosts call this from their existing event loop/timer. Importing Ribosome
    /// never starts a background thread or daemon.
    pub fn poll_subscription(&self, subscription_id: &str) -> Result<Option<WorkItem>> {
        self.poll_subscription_with_flush(subscription_id, false)
    }

    pub(crate) fn poll_subscription_with_flush(
        &self,
        subscription_id: &str,
        flush: bool,
    ) -> Result<Option<WorkItem>> {
        let (body, cursor, pending, first): (String, String, String, String) = self
            .db
            .query_row(
                "SELECT body,cursor,pending,first_pending_ms FROM subscriptions WHERE id=?1",
                [subscription_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("subscription not found"))?;
        let subscription: Subscription = serde_json::from_str(&body)?;
        let (source_run, attachment_id): (Option<String>, Option<String>) = self.db.query_row(
            "SELECT source_run,attachment_id FROM subscriptions WHERE id=?1",
            [subscription_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let grant = self.grant(&subscription.grant_id)?;
        if counter(&grant.budget.deadline_ms)? <= now_ms() {
            return Err(Error::exhausted("subscription grant expired"));
        }
        let mut pending: Vec<String> = serde_json::from_str(&pending)?;
        let mut first = counter(&first)?;
        let page = self.evidence(
            &grant,
            &EvidenceRequest {
                cursor: cursor.clone(),
                limit: 100,
                run_id: source_run,
            },
        )?;
        for event in page.events {
            if subscription.kinds.contains(&event.kind) {
                if pending.is_empty() {
                    first = now_ms();
                }
                pending.push(event.id);
            }
        }
        let ready = (!pending.is_empty() && flush)
            || pending.len() >= subscription.batch_size as usize
            || (!pending.is_empty()
                && now_ms().saturating_sub(first) >= u64::from(subscription.max_delay_ms));
        if ready {
            for reference in &pending {
                self.require_reference(&grant, reference)?;
            }
        }
        let tx = self.write_transaction()?;
        let actual_cursor: String = tx.query_row(
            "SELECT cursor FROM subscriptions WHERE id=?1",
            [subscription_id],
            |r| r.get(0),
        )?;
        if actual_cursor != cursor {
            return Err(Error::conflict("subscription advanced concurrently"));
        }
        let work = if ready {
            let request = WorkRequest {
                subject: format!("{}:{}", subscription.subject, page.cursor),
                profile: subscription.profile,
                operator: subscription.operator,
                reason: format!(
                    "Inspect a host-routed batch of {} events of kinds {}",
                    pending.len(),
                    subscription.kinds.join(", ")
                ),
                evidence_refs: pending.clone(),
            };
            let work = self.enqueue_work_in(&tx, &grant, subscription_id, &request)?;
            if let Some(attachment) = &attachment_id {
                tx.execute(
                    "INSERT OR IGNORE INTO attachment_work(work_id,attachment_id) VALUES (?1,?2)",
                    params![work.id, attachment],
                )?;
            }
            pending.clear();
            first = 0;
            Some(work)
        } else {
            None
        };
        tx.execute(
            "UPDATE subscriptions SET cursor=?2,pending=?3,first_pending_ms=?4 WHERE id=?1",
            params![
                subscription_id,
                page.cursor,
                serde_json::to_string(&pending)?,
                first.to_string()
            ],
        )?;
        tx.commit()?;
        Ok(work)
    }
}
