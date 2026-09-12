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
            if old.kind != submission.kind
                || submission.expected_version.as_ref() != Some(&old.version)
            {
                return Err(Error::conflict("record kind or version mismatch"));
            }
            if matches!(
                old.kind,
                RecordKind::Implementation
                    | RecordKind::Definition
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

    fn save_record_for_run(
        &self,
        record: &RecordEnvelope,
        expected: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<()> {
        let tx = self.write_transaction()?;
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
            if tx.execute(
                "UPDATE records SET version=?2,body=?3 WHERE id=?1 AND version=?4",
                params![record.id, record.version, body, expected],
            )? != 1
            {
                return Err(Error::conflict("record changed concurrently"));
            }
            tx.execute("DELETE FROM record_search WHERE id=?1", [&record.id])?;
        } else {
            tx.execute("INSERT INTO records(id,client,project,kind,version,split,body) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![record.id,record.scope.client,record.scope.project,kind,record.version,split,body])?;
        }
        if !record.retired {
            tx.execute(
                "INSERT INTO record_search(id,content) VALUES (?1,?2)",
                params![record.id, serde_json::to_string(&record.body)?],
            )?;
        }
        if let Some(run_id) = run_id {
            tx.execute("INSERT OR IGNORE INTO attachment_records(run_id,record_id) SELECT ?1,?2 WHERE EXISTS(SELECT 1 FROM attachment_work WHERE work_id=?1)", params![run_id,record.id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record(&self, grant: &Grant, id: &str) -> Result<RecordEnvelope> {
        self.expire_memories(grant)?;
        let body: Option<String> = self
            .db
            .query_row(
                "SELECT body FROM records WHERE id=?1 AND client=?2 AND project=?3",
                params![id, grant.scope.client, grant.scope.project],
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
        Ok(record)
    }

    pub fn require_reference(&self, grant: &Grant, id: &str) -> Result<()> {
        self.expire_memories(grant)?;
        let mut pending = vec![id.to_owned()];
        let mut visited = std::collections::HashSet::new();
        while let Some(reference) = pending.pop() {
            if !visited.insert(reference.clone()) {
                continue;
            }
            if visited.len() > 1000 {
                return Err(Error::exhausted(
                    "source lineage exceeds the local reference bound",
                ));
            }
            let record: Option<String> = self
                .db
                .query_row(
                    "SELECT body FROM records WHERE id=?1 AND client=?2 AND project=?3",
                    params![reference, grant.scope.client, grant.scope.project],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(body) = record {
                let record: RecordEnvelope = serde_json::from_str(&body)?;
                if record.retired
                    || expired(&record)?
                    || !grant.visible_splits.contains(&record.provenance.split)
                {
                    return Err(Error::denied(format!(
                        "source reference {id:?} is absent or inaccessible"
                    )));
                }
                pending.extend(record.provenance.source_refs);
                continue;
            }
            let event: Option<String> = self
                .db
                .query_row(
                    "SELECT body FROM events WHERE id=?1 AND client=?2 AND project=?3",
                    params![reference, grant.scope.client, grant.scope.project],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(body) = event {
                let event: Event = serde_json::from_str(&body)?;
                if !grant.visible_splits.contains(&event.provenance.split) {
                    return Err(Error::denied(format!(
                        "source reference {id:?} is absent or inaccessible"
                    )));
                }
                pending.extend(event.provenance.source_refs);
                continue;
            }
            return Err(Error::denied(format!(
                "source reference {id:?} is absent or inaccessible"
            )));
        }
        Ok(())
    }

    pub fn search(&self, grant: &Grant, request: &SearchRequest) -> Result<RecordPage> {
        self.expire_memories(grant)?;
        validate("SearchRequest", &serde_json::to_value(request)?)?;
        // Quote tokens as literals: user/model text is never FTS query syntax.
        let query = request
            .query
            .split_whitespace()
            .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let kind = request
            .kind
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?
            .and_then(|v| v.as_str().map(str::to_owned));
        let mut stmt=self.db.prepare("SELECT body FROM records WHERE client=?1 AND project=?2 AND (?3 IS NULL OR kind=?3) AND (?4='' OR id IN (SELECT id FROM record_search WHERE record_search MATCH ?4)) ORDER BY id")?;
        let rows = stmt.query_map(
            params![grant.scope.client, grant.scope.project, kind, query],
            |r| r.get::<_, String>(0),
        )?;
        let mut records = Vec::new();
        let mut skipped = 0;
        let mut bytes = 0;
        for row in rows {
            let body = row?;
            let r: RecordEnvelope = serde_json::from_str(&body)?;
            if r.retired || expired(&r)? || !grant.visible_splits.contains(&r.provenance.split) {
                continue;
            }
            if request.inventory == SearchRequestInventory::Usable
                && (r.kind != RecordKind::Implementation || !self.is_admitted(grant, &r)?)
            {
                continue;
            }
            if skipped < request.offset {
                skipped += 1;
                continue;
            }
            if bytes + body.len() > crate::validation::MAX_FRAME / 2
                || records.len() >= request.limit as usize
            {
                break;
            }
            bytes += body.len();
            records.push(r);
        }
        Ok(RecordPage {
            next_offset: request.offset + records.len() as u32,
            records,
        })
    }

    pub fn is_admitted(&self, grant: &Grant, implementation: &RecordEnvelope) -> Result<bool> {
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
        let mut record = self.record(grant, &request.id)?;
        if record.version != request.expected_version {
            return Err(Error::conflict("record version mismatch"));
        }
        if matches!(record.kind, RecordKind::Admission | RecordKind::Evaluation) {
            return Err(Error::denied("protected record"));
        }
        record.retired = true;
        record.updated_ms = now_ms().to_string();
        record.version = (counter(&record.version)? + 1).to_string();
        if request.delete {
            record.body = serde_json::Map::new();
            record.provenance.source_refs.clear();
        }
        self.save_record(&record, Some(&request.expected_version))?;
        self.invalidate_derived(grant, &record.id, request.delete)?;
        Ok(())
    }

    fn invalidate_derived(&self, grant: &Grant, source: &str, delete: bool) -> Result<()> {
        let mut pending = vec![source.to_owned()];
        let mut visited = std::collections::HashSet::new();
        while let Some(source) = pending.pop() {
            if !visited.insert(source.clone()) {
                continue;
            }
            self.db
                .execute("DELETE FROM archive WHERE implementation_id=?1", [&source])?;
            let mut activity = self
                .db
                .prepare("SELECT id,body FROM events WHERE client=?1 AND project=?2")?;
            for row in activity
                .query_map(params![grant.scope.client, grant.scope.project], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
            {
                let (id, body) = row?;
                let event: Event = serde_json::from_str(&body)?;
                if event.provenance.source_refs.contains(&source) {
                    pending.push(id);
                }
            }
            let mut stmt = self
                .db
                .prepare("SELECT body FROM records WHERE client=?1 AND project=?2")?;
            let rows = stmt
                .query_map(params![grant.scope.client, grant.scope.project], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for row in rows {
                let mut record: RecordEnvelope = serde_json::from_str(&row)?;
                if record.retired {
                    continue;
                }
                let mut references = record.provenance.source_refs.clone();
                crate::exports::collect_references(
                    &Value::Object(record.body.clone()),
                    &mut references,
                );
                if references.contains(&source) {
                    let expected = record.version.clone();
                    record.version = (counter(&expected)? + 1).to_string();
                    record.retired = true;
                    record.updated_ms = now_ms().to_string();
                    if delete {
                        record.body.clear();
                        record.provenance.source_refs.clear();
                    }
                    self.save_record(&record, Some(&expected))?;
                    pending.push(record.id);
                }
            }
        }
        Ok(())
    }

    fn expire_memories(&self, grant: &Grant) -> Result<()> {
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
                self.invalidate_derived(grant, &record.id, false)?;
            }
        }
        Ok(())
    }
}

fn expired(record: &RecordEnvelope) -> Result<bool> {
    Ok(record.kind == RecordKind::Memory
        && record
            .body
            .get("expires_ms")
            .and_then(Value::as_str)
            .map(counter)
            .transpose()?
            .is_some_and(|expiry| expiry <= now_ms()))
}
