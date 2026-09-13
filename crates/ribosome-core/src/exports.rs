use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    export_files::{EXPORT_PREFIX, MAX_EXPORT_BYTES},
    host::hash,
    validation::{id, validate},
};
use rusqlite::params;
use serde_json::{Value, json};
use std::{collections::HashSet, io::Write};

impl Runtime {
    pub fn export_training(&self, run_id: &str, request: &ExportRequest) -> Result<ExportResult> {
        validate("ExportRequest", &serde_json::to_value(request)?)?;
        let grant = self.store.require_active(run_id)?;
        if !grant.allow_export {
            return Err(Error::denied("training export is not granted"));
        }
        if request.record_ids.is_empty() {
            return Err(Error::invalid("export selection is empty"));
        }
        self.refresh_artifact_sources(&grant)?;
        let tx = self.store.write_transaction()?;
        let mut records = Vec::new();
        let mut checked = HashSet::new();
        for record_id in &request.record_ids {
            let record = self.store.record(&grant, record_id)?;
            if record.provenance.origin != request.product {
                return Err(Error::denied(
                    "export cannot relabel observed, reexecuted or synthetic material",
                ));
            }
            self.check_export_lineage(&grant, "record", record_id, &mut checked)?;
            if request.product != Origin::Synthetic && record.provenance.source_refs.is_empty() {
                return Err(Error::denied(
                    "observed or reexecuted training material requires source evidence",
                ));
            }
            records.push(record);
        }
        let mut content = Vec::new();
        for record in &records {
            serde_json::to_writer(
                &mut content,
                &json!({"product":request.product,"scope":grant.scope,"record":record}),
            )?;
            content.write_all(b"\n")?;
            if content.len() > MAX_EXPORT_BYTES {
                return Err(Error::exhausted(
                    "training export exceeds the 16 MiB artifact limit; select fewer records",
                ));
            }
        }
        let snapshot = id();
        let filename = format!("{snapshot}.jsonl");
        let path = self.store.export_path(&snapshot, &filename)?;
        let artifact = ArtifactRef {
            path: format!("{EXPORT_PREFIX}{snapshot}"),
            version: hash(&content),
        };
        let result = ExportResult {
            path: path.to_string_lossy().into(),
            count: records.len() as u32,
            artifact: artifact.clone(),
        };
        validate("ExportResult", &serde_json::to_value(&result)?)?;
        tx.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,available,body,export_file) VALUES(?1,?2,?3,?4,?5,'development',?6,?6,'historical',0,'{}',?7)",params![snapshot,grant.scope.client,grant.scope.project,grant.id,artifact.path,artifact.version,filename])?;
        // Preserve the reviewed closure itself. A later record revision must
        // not remove the ancestry of bytes already written into this product.
        for (kind, source) in &checked {
            let query = match kind.as_str() {
                "record" => "SELECT version FROM records WHERE id=?1",
                "event" => "SELECT sequence FROM events WHERE id=?1",
                "artifact" => "SELECT version FROM artifact_snapshots WHERE id=?1",
                _ => return Err(Error::denied("unknown export source kind")),
            };
            let version: String = tx.query_row(query, [source], |row| row.get(0))?;
            tx.execute("INSERT INTO source_edges(subject_kind,subject_id,source_kind,source_id,source_version) VALUES('artifact',?1,?2,?3,?4)",params![snapshot,kind,source,version])?;
        }
        // This manifest survives a crash before or during the file write.
        tx.commit()?;
        let publish = (|| -> Result<()> {
            let tx = self.store.write_transaction()?;
            let mut rechecked = HashSet::new();
            for (kind, source) in &checked {
                self.check_export_lineage(&grant, kind, source, &mut rechecked)?;
            }
            let withdrawn: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM source_tombstones WHERE kind='artifact' AND id=?1)",
                [&snapshot],
                |r| r.get(0),
            )?;
            if withdrawn {
                return Err(Error::denied("export was withdrawn before publication"));
            }
            // Source retirement cannot interleave with publication. The path
            // is disclosed only after bytes and the ready flag have committed.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&content)?;
            file.sync_all()?;
            std::fs::File::open(path.parent().unwrap())?.sync_all()?;
            tx.execute(
                "UPDATE artifact_snapshots SET available=1 WHERE id=?1",
                [&snapshot],
            )?;
            self.store.require_source(&grant, "artifact", &snapshot)?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(error) = publish {
            // Unavailable manifests are recoverable even if this cleanup also
            // encounters a persistence fault; startup retries the same file.
            self.store.withdraw_export(&snapshot)?;
            self.store.cleanup_sources(&grant, 1)?;
            return Err(error);
        }
        Ok(result)
    }

    fn check_export_lineage(
        &self,
        grant: &Grant,
        kind: &str,
        reference: &str,
        checked: &mut HashSet<(String, String)>,
    ) -> Result<()> {
        if !checked.insert((kind.to_owned(), reference.to_owned())) {
            return Ok(());
        }
        if checked.len() > 1000 {
            return Err(Error::exhausted(
                "export lineage exceeds bounded review size",
            ));
        }
        self.store.require_source(grant, kind, reference)?;
        let query = match kind {
            "record" => {
                "SELECT split,kind IN ('evaluation','experiment','admission','recommendation') FROM records WHERE id=?1 AND client=?2 AND project=?3"
            }
            "event" => "SELECT split,0 FROM events WHERE id=?1 AND client=?2 AND project=?3",
            "artifact" => {
                "SELECT split,0 FROM artifact_snapshots WHERE id=?1 AND client=?2 AND project=?3"
            }
            _ => return Err(Error::denied("export lineage is missing or inaccessible")),
        };
        let (split, protected): (String, bool) = self.store.db.query_row(
            query,
            params![reference, grant.scope.client, grant.scope.project],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if split != "development" || protected {
            return Err(Error::denied(
                "evaluation and holdout material cannot enter training exports",
            ));
        }
        // Export uses the retained graph, including artifact snapshots and
        // sender-derived observations. Released aggregates do not release raw
        // protected ancestry into a training product.
        let sources=self.store.db.prepare("SELECT source_kind,source_id FROM source_edges WHERE subject_kind=?1 AND subject_id=?2")?.query_map(params![kind,reference],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for (kind, source) in sources {
            self.check_export_lineage(grant, &kind, &source, checked)?;
        }
        Ok(())
    }
}

pub(crate) fn collect_references(value: &Value, references: &mut Vec<String>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if matches!(
                key.as_str(),
                "evidence_refs"
                    | "event_refs"
                    | "evaluation_refs"
                    | "source_refs"
                    | "memory_start_refs"
                    | "conflicts"
                    | "supersedes"
                    | "incoming_context_refs"
                    | "support_refs"
                    | "contradiction_refs"
                    | "occurrence_refs"
                    | "recipient_entry_refs"
                    | "result_refs"
            ) {
                if let Some(array) = value.as_array() {
                    references.extend(array.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            } else if matches!(
                key.as_str(),
                "finding_ref" | "source_event_ref" | "target_event_ref"
            ) {
                if let Some(reference) = value.as_str() {
                    references.push(reference.into());
                }
            } else if matches!(
                key.as_str(),
                "definition" | "implementation" | "donor" | "candidate" | "baseline"
            ) {
                if let Some(reference) = value["id"].as_str() {
                    references.push(reference.into());
                }
            } else if matches!(
                key.as_str(),
                "motifs" | "definition_refs" | "discovery_refs"
            ) {
                if let Some(array) = value.as_array() {
                    references.extend(
                        array
                            .iter()
                            .filter_map(|v| v["id"].as_str())
                            .map(str::to_owned),
                    );
                }
            } else if key == "development_cases" {
                if let Some(cases) = value.as_array() {
                    for case in cases {
                        // Only declared lineage is authoritative. Arbitrary
                        // scenario input is data, not a reference contract.
                        collect_references(&case["provenance"], references);
                    }
                }
            } else if matches!(
                key.as_str(),
                "functional_contract" | "grounding" | "local_outcome" | "instruction_contract"
            ) {
                collect_references(value, references);
            } else if matches!(
                key.as_str(),
                "obligations"
                    | "relations"
                    | "role_bindings"
                    | "dependency_evidence"
                    | "source_windows"
                    | "hypotheses"
                    | "alternatives"
            ) && let Some(array) = value.as_array()
            {
                for value in array {
                    collect_references(value, references);
                }
            }
        }
    }
}
