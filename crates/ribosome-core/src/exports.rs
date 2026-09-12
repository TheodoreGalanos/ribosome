use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    validation::id,
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{collections::HashSet, io::Write};

impl Runtime {
    pub fn export_training(&self, run_id: &str, request: &ExportRequest) -> Result<ExportResult> {
        let grant = self.store.require_active(run_id)?;
        if !grant.allow_export {
            return Err(Error::denied("training export is not granted"));
        }
        if request.record_ids.is_empty() {
            return Err(Error::invalid("export selection is empty"));
        }
        let mut records = Vec::new();
        let mut checked = HashSet::new();
        for record_id in &request.record_ids {
            let record = self.store.record(&grant, record_id)?;
            if record.provenance.origin != request.product {
                return Err(Error::denied(
                    "export cannot relabel observed, reexecuted or synthetic material",
                ));
            }
            self.check_export_lineage(&grant, record_id, &mut checked)?;
            if request.product != Origin::Synthetic && record.provenance.source_refs.is_empty() {
                return Err(Error::denied(
                    "observed or reexecuted training material requires source evidence",
                ));
            }
            records.push(record);
        }
        let directory = self.state_dir.join("exports");
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        for record in &records {
            serde_json::to_writer(
                &mut file,
                &json!({"product":request.product,"scope":grant.scope,"record":record}),
            )?;
            file.write_all(b"\n")?;
        }
        file.as_file().sync_all()?;
        let path = directory.join(format!("{}.jsonl", id()));
        file.persist(&path)
            .map_err(|e| Error::internal(e.to_string()))?;
        Ok(ExportResult {
            path: path.to_string_lossy().into(),
            count: records.len() as u32,
        })
    }

    fn check_export_lineage(
        &self,
        grant: &Grant,
        reference: &str,
        checked: &mut HashSet<String>,
    ) -> Result<()> {
        if !checked.insert(reference.to_owned()) {
            return Ok(());
        }
        if checked.len() > 1000 {
            return Err(Error::exhausted(
                "export lineage exceeds bounded review size",
            ));
        }
        if let Ok(record) = self.store.record(grant, reference) {
            if record.provenance.split != Split::Development
                || matches!(
                    record.kind,
                    RecordKind::Evaluation
                        | RecordKind::Experiment
                        | RecordKind::Admission
                        | RecordKind::Recommendation
                )
            {
                return Err(Error::denied(
                    "evaluation and holdout material cannot enter training exports",
                ));
            }
            let mut references = record.provenance.source_refs.clone();
            collect_references(&Value::Object(record.body), &mut references);
            for source in references {
                self.check_export_lineage(grant, &source, checked)?;
            }
            return Ok(());
        }
        let event: Option<String> = self
            .store
            .db
            .query_row(
                "SELECT body FROM events WHERE id=?1 AND client=?2 AND project=?3",
                params![reference, grant.scope.client, grant.scope.project],
                |r| r.get(0),
            )
            .optional()?;
        let event: Event = serde_json::from_str(
            &event.ok_or_else(|| Error::denied("export lineage is missing or inaccessible"))?,
        )?;
        if event.provenance.split != Split::Development {
            return Err(Error::denied(
                "protected source event cannot enter training export",
            ));
        }
        for source in event.provenance.source_refs {
            self.check_export_lineage(grant, &source, checked)?;
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
            ) {
                if let Some(array) = value.as_array() {
                    references.extend(array.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            } else if key == "finding_ref" {
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
            } else if key == "motifs" {
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
            } else if key == "obligations"
                && let Some(array) = value.as_array()
            {
                for value in array {
                    collect_references(value, references);
                }
            }
        }
    }
}
