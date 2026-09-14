use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

impl Store {
    pub fn submit(
        &self,
        grant: &Grant,
        submission: &RecordSubmission,
        protected: bool,
    ) -> Result<RecordEnvelope> {
        self.submit_for_run(grant, submission, protected, None)
    }

    pub(crate) fn submit_for_run(
        &self,
        grant: &Grant,
        submission: &RecordSubmission,
        protected: bool,
        run_id: Option<&str>,
    ) -> Result<RecordEnvelope> {
        validate("RecordSubmission", &serde_json::to_value(submission)?)?;
        if serde_json::to_vec(submission)?.len() > crate::validation::MAX_FRAME / 4 {
            return Err(Error::invalid(
                "record exceeds 256 KiB; reference large artifacts instead",
            ));
        }
        let kind = serde_json::to_value(&submission.kind)?
            .as_str()
            .unwrap()
            .to_owned();
        let contract = crate::validation::schema()["x-records"][&kind]
            .as_str()
            .ok_or_else(|| Error::invalid("unsupported record kind"))?;
        validate(contract, &Value::Object(submission.body.clone()))?;
        if !protected
            && matches!(
                submission.kind,
                RecordKind::Admission | RecordKind::Evaluation
            )
        {
            return Err(Error::denied(
                "only the host evaluator or admission authority may write this record",
            ));
        }
        if !grant.visible_splits.contains(&submission.provenance.split) {
            return Err(Error::denied("split not granted"));
        }
        // Having access to protected evidence does not authorize an agent to
        // publish a derived record under a less restricted label.
        let mut lineage_grant = grant.clone();
        lineage_grant.visible_splits.retain(|split| {
            crate::validation::split_level(split)
                <= crate::validation::split_level(&submission.provenance.split)
        });
        for reference in &submission.provenance.source_refs {
            self.require_reference(&lineage_grant, reference)?;
        }
        let mut references = Vec::new();
        crate::exports::collect_references(
            &Value::Object(submission.body.clone()),
            &mut references,
        );
        for reference in references {
            // An experimenter may cite opaque evaluation IDs returned by the
            // laboratory, but cannot read their protected observations.
            if submission.kind == RecordKind::Recommendation
                && submission.body["evaluation_refs"]
                    .as_array()
                    .is_some_and(|refs| refs.iter().any(|v| v.as_str() == Some(&reference)))
            {
                let evaluation_scope: Option<(String, String)> = self
                    .db
                    .query_row(
                        "SELECT client,project FROM records WHERE id=?1 AND kind='evaluation'",
                        [&reference],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if evaluation_scope
                    != Some((grant.scope.client.clone(), grant.scope.project.clone()))
                {
                    return Err(Error::denied("evaluation reference is outside scope"));
                }
            } else if protected {
                let mut authority = grant.clone();
                authority.visible_splits =
                    vec![Split::Development, Split::Evaluation, Split::Holdout];
                self.require_reference(&authority, &reference)?;
            } else {
                self.require_reference(&lineage_grant, &reference)?;
            }
        }
        self.validate_motif_submission(&lineage_grant, submission, run_id)?;
        self.validate_transplant(&lineage_grant, submission)?;
        let now = now_ms().to_string();
        let mut record = RecordEnvelope {
            schema_version: "1".into(),
            id: submission.id.clone().unwrap_or_else(id),
            scope: grant.scope.clone(),
            kind: submission.kind.clone(),
            version: "1".into(),
            created_ms: now.clone(),
            updated_ms: now,
            retired: false,
            provenance: submission.provenance.clone(),
            body: submission.body.clone(),
        };
        if let Some(record_id) = &submission.id {
            let old = self.record(grant, record_id)?;
            if old.scope != grant.scope {
                return Err(Error::denied(
                    "delivered prepared records are read-only; save a recipient-owned derivative",
                ));
            }
            if old.kind != submission.kind
                || submission.expected_version.as_ref() != Some(&old.version)
            {
                return Err(Error::conflict("record kind or version mismatch"));
            }
            if matches!(
                old.kind,
                RecordKind::Implementation
                    | RecordKind::Definition
                    | RecordKind::Discovery
                    | RecordKind::Experiment
                    | RecordKind::Evaluation
                    | RecordKind::Admission
            ) {
                return Err(Error::conflict(
                    "versioned material is immutable; submit a new record",
                ));
            }
            record.version = (counter(&old.version)? + 1).to_string();
            record.created_ms = old.created_ms;
        } else if submission.expected_version.is_some() {
            return Err(Error::invalid("expected_version requires id"));
        }
        self.save_record_for_run(&record, submission.expected_version.as_deref(), run_id)?;
        Ok(record)
    }

    fn save_record(&self, record: &RecordEnvelope, expected: Option<&str>) -> Result<()> {
        self.save_record_for_run(record, expected, None)
    }

    pub(crate) fn save_record_for_run(
        &self,
        record: &RecordEnvelope,
        expected: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<()> {
        let tx = self
            .db
            .is_autocommit()
            .then(|| self.write_transaction())
            .transpose()?;
        let db = &self.db;
        let body = serde_json::to_string(record)?;
        let kind = serde_json::to_value(&record.kind)?
            .as_str()
            .unwrap()
            .to_owned();
        let split = serde_json::to_value(&record.provenance.split)?
            .as_str()
            .unwrap()
            .to_owned();
        if let Some(expected) = expected {
            if db.execute(
                "UPDATE records SET version=?2,body=?3 WHERE id=?1 AND version=?4",
                params![record.id, record.version, body, expected],
            )? != 1
            {
                return Err(Error::conflict("record changed concurrently"));
            }
            db.execute("DELETE FROM record_search WHERE id=?1", [&record.id])?;
        } else {
            db.execute("INSERT INTO records(id,client,project,kind,version,split,body) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![record.id,record.scope.client,record.scope.project,kind,record.version,split,body])?;
        }
        if !record.retired {
            db.execute(
                "INSERT INTO record_search(id,content) VALUES (?1,?2)",
                params![record.id, serde_json::to_string(&record.body)?],
            )?;
        }
        crate::sources::record_sources(db, record)?;
        if let Some(run_id) = run_id {
            db.execute("INSERT OR IGNORE INTO attachment_records(run_id,record_id) SELECT ?1,?2 WHERE EXISTS(SELECT 1 FROM attachment_work WHERE work_id=?1)", params![run_id,record.id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub fn record(&self, grant: &Grant, id: &str) -> Result<RecordEnvelope> {
        self.expire_memories(grant)?;
        let effective = self.evaluation_source_grant(grant, "record", id, true)?;
        let body: Option<String> = self
            .db
            .query_row(
                "SELECT body FROM records WHERE id=?1 AND client=?2 AND project=?3",
                params![id, effective.scope.client, effective.scope.project],
                |r| r.get(0),
            )
            .optional()?;
        let record: RecordEnvelope = serde_json::from_str(
            &body.ok_or_else(|| Error::missing("record not found in granted scope"))?,
        )?;
        if record.retired
            || !grant.visible_splits.contains(&record.provenance.split)
            || expired(&record)?
        {
            return Err(Error::missing("record unavailable"));
        }
        self.require_source(grant, "record", id)?;
        Ok(record)
    }

    pub fn require_reference(&self, grant: &Grant, id: &str) -> Result<()> {
        self.expire_memories(grant)?;
        let is_record: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        let is_artifact: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_snapshots WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        self.require_source(
            grant,
            if is_record {
                "record"
            } else if is_artifact {
                "artifact"
            } else {
                "event"
            },
            id,
        )
    }

    pub fn is_admitted(&self, grant: &Grant, implementation: &RecordEnvelope) -> Result<bool> {
        if !self.source_available(grant, "record", &implementation.id)? {
            return Ok(false);
        }
        let imp: Implementation =
            serde_json::from_value(Value::Object(implementation.body.clone()))?;
        if !imp
            .required_capabilities
            .iter()
            .all(|t| grant.tools.contains(t))
        {
            return Ok(false);
        }
        if grant.mode == Mode::Observe && !imp.possible_effects.is_empty() {
            return Ok(false);
        }
        let mut stmt=self.db.prepare("SELECT body FROM records WHERE client=?1 AND project=?2 AND kind='admission' ORDER BY id DESC")?;
        for row in stmt.query_map(params![grant.scope.client, grant.scope.project], |r| {
            r.get::<_, String>(0)
        })? {
            let r: RecordEnvelope = serde_json::from_str(&row?)?;
            if r.retired || !grant.visible_splits.contains(&r.provenance.split) {
                continue;
            }
            if !self.source_available(grant, "record", &r.id)? {
                continue;
            }
            let a: Admission = serde_json::from_value(Value::Object(r.body))?;
            if a.implementation.id == implementation.id
                && a.implementation.version == imp.version
                && a.context == grant.context
            {
                return Ok(a.decision == AdmissionDecision::Accepted);
            }
        }
        Ok(false)
    }

    pub fn retire(&self, grant: &Grant, request: &RetireRequest) -> Result<()> {
        validate("RetireRequest", &serde_json::to_value(request)?)?;
        // An owner can request physical deletion after retirement or expiry.
        // This administrative path returns no unavailable record content.
        let body: Option<String> = self
            .db
            .query_row(
                "SELECT body FROM records WHERE id=?1 AND client=?2 AND project=?3",
                params![request.id, grant.scope.client, grant.scope.project],
                |r| r.get(0),
            )
            .optional()?;
        let mut record: RecordEnvelope = serde_json::from_str(
            &body.ok_or_else(|| Error::missing("record not found in granted scope"))?,
        )?;
        if !grant.visible_splits.contains(&record.provenance.split) {
            return Err(Error::denied("record unavailable"));
        }
        if record.version != request.expected_version {
            return Err(Error::conflict("record version mismatch"));
        }
        if matches!(record.kind, RecordKind::Admission | RecordKind::Evaluation) {
            return Err(Error::denied("protected record"));
        }
        if record.retired && (!request.delete || record.body.is_empty()) {
            return self.cleanup_sources(grant, 1);
        }
        record.retired = true;
        record.updated_ms = now_ms().to_string();
        record.version = (counter(&record.version)? + 1).to_string();
        if request.delete {
            record.body = serde_json::Map::new();
            record.provenance.source_refs.clear();
        }
        self.save_record(&record, Some(&request.expected_version))?;
        self.cleanup_sources(grant, 1)?;
        Ok(())
    }

    pub(crate) fn expire_memories(&self, grant: &Grant) -> Result<()> {
        let mut statement = self
            .db
            .prepare("SELECT body FROM records WHERE client=?1 AND project=?2 AND kind='memory'")?;
        let rows = statement
            .query_map(params![grant.scope.client, grant.scope.project], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for row in rows {
            let mut record: RecordEnvelope = serde_json::from_str(&row)?;
            if !record.retired && expired(&record)? {
                let expected = record.version.clone();
                record.version = (counter(&expected)? + 1).to_string();
                record.retired = true;
                record.updated_ms = now_ms().to_string();
                self.save_record(&record, Some(&expected))?;
                self.cleanup_sources(grant, 1)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn expired(record: &RecordEnvelope) -> Result<bool> {
    Ok(record.kind == RecordKind::Memory
        && record
            .body
            .get("expires_ms")
            .and_then(Value::as_str)
            .map(counter)
            .transpose()?
            .is_some_and(|expiry| expiry <= now_ms()))
}
