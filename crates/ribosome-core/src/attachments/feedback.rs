use super::*;
use crate::{effects::Runtime, validation::id};
use serde_json::Value;

impl Store {
    pub(crate) fn publish_attachment_result(
        &self,
        work: &WorkItem,
        result: &AgentResult,
        policy: &AttachmentPolicy,
    ) -> Result<()> {
        let Some(a) = self.run_attachment(&work.id)? else {
            return Ok(());
        };
        let refs = self
            .db
            .prepare("SELECT record_id FROM attachment_records WHERE run_id=?1 ORDER BY record_id")?
            .query_map([&work.id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let grant = self.grant(&a.grant_id)?;
        let mut versions = std::collections::BTreeMap::new();
        for reference in &work.evidence_refs {
            if let Some(body) = self
                .db
                .query_row("SELECT body FROM events WHERE id=?1", [reference], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
            {
                let event: Event = serde_json::from_str(&body)?;
                for artifact in event.artifacts {
                    versions.insert(artifact.path, artifact.version);
                }
            }
        }
        let mut proposal = false;
        for reference in &refs {
            if let Ok(record) = self.record(&grant, reference)
                && record.kind == RecordKind::Intervention
            {
                proposal = true;
                let intervention: Intervention =
                    serde_json::from_value(Value::Object(record.body))?;
                for artifact in intervention.read_versions {
                    versions.insert(artifact.path, artifact.version);
                }
            }
        }
        // Successful effects supersede the input versions for feedback validity.
        let mut effects = self
            .db
            .prepare("SELECT body FROM effects WHERE run_id=?1 ORDER BY rowid")?;
        for row in effects.query_map([&work.id], |r| r.get::<_, String>(0))? {
            let receipt: ActionReceipt = serde_json::from_str(&row?)?;
            if receipt.status == EffectStatus::Succeeded
                && (receipt.action.kind == ActionKind::Apply || receipt.action.branch_id.is_none())
            {
                for artifact in receipt.after {
                    versions.insert(artifact.path, artifact.version);
                }
            }
        }
        let feedback = AttachmentFeedback {
            id: work.id.clone(),
            attachment_id: a.id.clone(),
            run_id: work.id.clone(),
            kind: if proposal {
                AttachmentFeedbackKind::Proposal
            } else {
                AttachmentFeedbackKind::Finding
            },
            state: FeedbackState::Pending,
            summary: result.summary.clone(),
            disposition: result.disposition.clone(),
            record_refs: refs,
            evidence_refs: work.evidence_refs.clone(),
            artifact_versions: versions
                .into_iter()
                .map(|(path, version)| ArtifactRef { path, version })
                .collect(),
            expires_ms: (now_ms() + u64::from(policy.feedback_ttl_ms))
                .min(counter(&grant.budget.deadline_ms)?)
                .to_string(),
            attempts: 0,
            detail: String::new(),
        };
        validate("AttachmentFeedback", &serde_json::to_value(&feedback)?)?;
        if serde_json::to_vec(&feedback)?.len() > crate::validation::MAX_FRAME - 2048 {
            return Err(Error::exhausted(
                "feedback exceeds frame capacity; inspect the persisted run directly",
            ));
        }
        self.db.execute(
            "INSERT OR IGNORE INTO attachment_feedback(id,attachment_id,body) VALUES (?1,?2,?3)",
            params![feedback.id, a.id, serde_json::to_string(&feedback)?],
        )?;
        Ok(())
    }

    pub(crate) fn feedback(&self, attachment: &str, id: &str) -> Result<AttachmentFeedback> {
        let body = self
            .db
            .query_row(
                "SELECT body FROM attachment_feedback WHERE attachment_id=?1 AND id=?2",
                params![attachment, id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| Error::missing("feedback not found in attachment"))?;
        Ok(serde_json::from_str(&body)?)
    }

    fn save_feedback(&self, feedback: &AttachmentFeedback) -> Result<()> {
        self.db.execute(
            "UPDATE attachment_feedback SET body=?2 WHERE id=?1",
            params![feedback.id, serde_json::to_string(feedback)?],
        )?;
        Ok(())
    }

    pub fn acknowledge_feedback(&self, request: &FeedbackAcknowledgement) -> Result<()> {
        validate("FeedbackAcknowledgement", &serde_json::to_value(request)?)?;
        let mut f = self.feedback(&request.attachment_id, &request.feedback_id)?;
        let state = match request.outcome {
            FeedbackAcknowledgementOutcome::Acknowledged => FeedbackState::Acknowledged,
            FeedbackAcknowledgementOutcome::Rejected => FeedbackState::Rejected,
            FeedbackAcknowledgementOutcome::Unknown => FeedbackState::Unknown,
        };
        if f.state == state {
            return Ok(());
        }
        if !matches!(f.state, FeedbackState::Delivered | FeedbackState::Unknown) {
            return Err(Error::conflict(
                "feedback has not been delivered or is already terminal",
            ));
        }
        f.state = state;
        f.detail = request.detail.clone();
        self.save_feedback(&f)
    }

    pub fn attachment_record(
        &self,
        attachment_id: &str,
        record_id: &str,
    ) -> Result<RecordEnvelope> {
        let a = self.attachment(attachment_id)?;
        let exists: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM attachment_records r JOIN attachment_work w ON w.work_id=r.run_id WHERE w.attachment_id=?1 AND r.record_id=?2)",params![attachment_id,record_id], |r|r.get(0))?;
        if !exists {
            return Err(Error::missing("record not published by this attachment"));
        }
        self.record(&self.grant(&a.grant_id)?, record_id)
    }

    pub(crate) fn settle_attachment_work(
        &self,
        grant_id: &str,
        policy: &AttachmentPolicy,
    ) -> Result<()> {
        let rows = self.db.prepare("SELECT w.body,r.result,r.status FROM work w JOIN attachment_work aw ON aw.work_id=w.id JOIN runs r ON r.id=w.id WHERE w.grant_id=?1 AND r.status NOT IN ('running','interrupted') AND (w.status IN ('running','interrupted') OR NOT EXISTS(SELECT 1 FROM attachment_feedback f WHERE f.id=w.id))")?.query_map([grant_id], |r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for (body, result, status) in rows {
            let work: WorkItem = serde_json::from_str(&body)?;
            if let Some(result) = result {
                let result: AgentResult = serde_json::from_str(&result)?;
                self.publish_attachment_result(&work, &result, policy)?;
                self.db.execute(
                    "UPDATE work SET status=?2,body=json_set(body,'$.status',?2) WHERE id=?1",
                    params![
                        work.id,
                        if status == "abstained" {
                            "completed"
                        } else {
                            &status
                        }
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn recover_attachment_deliveries(&self, grant_id: &str) -> Result<()> {
        for id in self.attachment_ids(grant_id)? {
            self.interrupt_attachment(
                &id,
                "Host connection lost; reconnect and report any missing source observations.",
            )?;
        }
        // Advisory deliveries may repeat. Control intents remain unknown until
        // the host explicitly acknowledges their outcome.
        self.db.execute("UPDATE attachment_feedback SET body=json_set(body,'$.state','pending') WHERE attachment_id IN (SELECT id FROM attachments WHERE grant_id=?1) AND json_extract(body,'$.state')='delivered'",[grant_id])?;
        Ok(())
    }
}

impl Runtime {
    fn valid_attachment_feedback(&self, f: &AttachmentFeedback) -> Result<bool> {
        let a = self.store.attachment(&f.attachment_id)?;
        let grant = self.store.grant(&a.grant_id)?;
        if a.state != AttachmentState::Active || now_ms() >= counter(&f.expires_ms)? {
            return Ok(false);
        }
        for reference in &f.record_refs {
            if self.store.record(&grant, reference).is_err() {
                return Ok(false);
            }
        }
        for reference in &f.evidence_refs {
            if self.store.require_reference(&grant, reference).is_err() {
                return Ok(false);
            }
        }
        for artifact in &f.artifact_versions {
            match self.host.version(&grant, &artifact.path, None) {
                Ok(current) if current == *artifact => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub fn attachment_feedback(&self, attachment_id: &str) -> Result<AttachmentFeedbackPage> {
        self.store.active_attachment(attachment_id)?;
        let rows = self.store.db.prepare("SELECT body FROM attachment_feedback WHERE attachment_id=?1 AND json_extract(body,'$.state') IN ('pending','delivered') ORDER BY rowid LIMIT 20")?.query_map([attachment_id], |r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut items = vec![];
        let mut bytes = 1024;
        for row in rows {
            let mut f: AttachmentFeedback = serde_json::from_str(&row)?;
            if !self.valid_attachment_feedback(&f)? {
                f.state = FeedbackState::Expired;
                f.detail="Feedback expired, source versions changed, or supporting records were withdrawn.".into();
            } else if f.attempts >= 3 {
                f.state = FeedbackState::Unknown;
                f.detail = "Delivery attempts exhausted without acknowledgement.".into();
            } else {
                let size = serde_json::to_vec(&f)?.len() + 128;
                if bytes + size > crate::validation::MAX_FRAME - 1024 {
                    break;
                }
                bytes += size;
                f.state = FeedbackState::Delivered;
                f.attempts += 1;
                items.push(f.clone());
            }
            self.store.save_feedback(&f)?;
        }
        Ok(AttachmentFeedbackPage { items })
    }

    pub fn begin_attachment_steering(&self, request: &FeedbackRequest) -> Result<()> {
        let (a, _) = self.store.active_attachment(&request.attachment_id)?;
        if !a.capabilities.contains(&AttachmentCapability::Steer) {
            return Err(Error::denied("steering not granted to this connector"));
        }
        let mut f = self.store.feedback(&a.id, &request.feedback_id)?;
        if f.state != FeedbackState::Delivered || !self.valid_attachment_feedback(&f)? {
            return Err(Error::conflict(
                "steering requires current, delivered feedback",
            ));
        }
        f.state = FeedbackState::Unknown;
        f.detail = format!(
            "Control intent {} persisted; external acceptance has not been acknowledged.",
            id()
        );
        self.store.save_feedback(&f)
    }
}
