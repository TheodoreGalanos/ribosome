use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    validation::{derived_split, now_ms},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub(crate) struct CreatedBranch {
    pub id: String,
    pub path: PathBuf,
    pub versions: Vec<ArtifactRef>,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct EffectPreflight {
    pub checks: Vec<(String, String)>,
    pub intervention: Option<VersionRef>,
    #[serde(default)]
    pub properties: Vec<PropertyBinding>,
}

/// Captured adapter evidence is durable before retryable SQLite bookkeeping.
/// It is not a second public receipt and is never filled from an agent claim.
#[derive(Serialize, Deserialize)]
pub(crate) struct EffectObservation {
    pub receipt: ActionReceipt,
    pub writes: Vec<String>,
    pub validation_generation: i64,
    pub branch: Option<CreatedBranch>,
    #[serde(default)]
    pub preflight: EffectPreflight,
}

impl EffectObservation {
    pub fn new(receipt: ActionReceipt) -> Self {
        Self {
            receipt,
            writes: vec![],
            validation_generation: 0,
            branch: None,
            preflight: EffectPreflight::default(),
        }
    }
}

impl Runtime {
    pub(crate) fn pin_effect_checks(&self, observation: &EffectObservation) -> Result<()> {
        let changed = self.store.db.execute(
            "UPDATE effects SET preflight=?2 WHERE id=?1 AND phase='prepared'",
            params![
                observation.receipt.operation_id,
                serde_json::to_string(&observation.preflight)?
            ],
        )?;
        if changed != 1 {
            return Err(Error::conflict(
                "effect is no longer prepared for checker preflight",
            ));
        }
        Ok(())
    }
    pub(crate) fn start_dispatch(
        &self,
        grant: &Grant,
        observation: &mut EffectObservation,
        writes: Vec<String>,
    ) -> Result<()> {
        if self.store.require_active(&observation.receipt.run_id)? != *grant {
            return Err(Error::denied(
                "owner policy changed during effect preflight",
            ));
        }
        if self
            .cancellation(&observation.receipt.run_id)
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(Error::denied("run cancelled before adapter dispatch"));
        }
        self.store
            .require_attachment_write(&observation.receipt.run_id, &observation.receipt.action)?;
        if let Some(reference) = &observation.preflight.intervention {
            let current = self.store.record(grant, &reference.id)?;
            if current.version != reference.version {
                return Err(Error::conflict(
                    "intervention changed during effect preflight",
                ));
            }
        }
        self.require_property_bindings(grant, &observation.preflight.properties)?;
        if !writes.is_empty() {
            let unsettled: bool = self.store.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM effects WHERE id<>?1 AND (phase IN ('dispatching','observed') OR (json_extract(body,'$.status')='unknown' AND settlement IS NULL)) AND (potential_writes<>'[]' OR json_extract(body,'$.action.kind')='apply' OR (json_extract(body,'$.action.kind') IN ('edit','execute') AND json_extract(body,'$.action.branch_id') IS NULL)))",
                [&observation.receipt.operation_id], |row| row.get(0),
            )?;
            if unsettled {
                return Err(Error::denied(
                    "unsettled workspace effects retain write ownership; reconcile them before new live writes",
                ));
            }
        }
        observation.validation_generation = self.store.validity_generation(&grant.scope)?;
        observation.writes = writes;
        let changed = self.store.db.execute(
            "UPDATE effects SET phase='dispatching',body=?2,potential_writes=?3,validation_generation=?4 WHERE id=?1 AND phase='prepared'",
            params![observation.receipt.operation_id, serde_json::to_string(&observation.receipt)?, serde_json::to_string(&observation.writes)?, observation.validation_generation],
        )?;
        if changed != 1 {
            return Err(Error::conflict("effect is no longer prepared for dispatch"));
        }
        Ok(())
    }

    pub(crate) fn capture_effect(&self, observation: &EffectObservation) -> Result<()> {
        let changed = self.store.db.execute(
            "UPDATE effects SET phase='observed',observation=?2 WHERE id=?1 AND phase IN ('prepared','dispatching')",
            params![observation.receipt.operation_id, serde_json::to_string(observation)?],
        )?;
        if changed != 1 {
            return Err(Error::conflict("effect observation was already captured"));
        }
        Ok(())
    }

    pub(crate) fn finalize_effect(&self, operation: &str) -> Result<ActionReceipt> {
        let (phase, body, authority, captured): (String, String, String, Option<String>) =
            self.store.db.query_row(
                "SELECT phase,body,coalesce(authority,(SELECT body FROM grants WHERE grants.id=effects.grant_id)),observation FROM effects WHERE id=?1",
                [operation],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        if phase == "finalized" {
            return Ok(serde_json::from_str(&body)?);
        }
        if phase != "observed" {
            return Err(Error::conflict(
                "effect has no captured outcome to finalize",
            ));
        }
        let grant: Grant = serde_json::from_str(&authority)?;
        let observation: EffectObservation = serde_json::from_str(
            &captured
                .ok_or_else(|| Error::internal("observed effect lacks its adapter outcome"))?,
        )?;
        let mut receipt = observation.receipt;
        // Filesystem reads do not need the SQLite writer. Cooperative workspace
        // ownership excludes another host effect here; the transaction below
        // still checks the operation phase and invalidation generations.
        let validated = self.transferable_validation(&grant, &receipt, &observation.preflight)?;
        let mut previous = std::collections::BTreeMap::new();
        for (artifact, _) in &validated {
            let properties = self
                .assess_properties(&grant, artifact)?
                .into_iter()
                .filter(|assessment| assessment.state == PropertyAssessmentState::Validated)
                .map(|assessment| ValidatedProperty {
                    obligation: assessment.obligation,
                    artifact: artifact.clone(),
                })
                .collect::<Vec<_>>();
            previous.insert(artifact.path.clone(), properties);
        }
        let transaction = self.store.write_transaction()?;
        let (phase, body): (String, String) = self.store.db.query_row(
            "SELECT phase,body FROM effects WHERE id=?1",
            [operation],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if phase == "finalized" {
            return Ok(serde_json::from_str(&body)?);
        }
        if phase != "observed" {
            return Err(Error::conflict(
                "effect outcome changed before finalization",
            ));
        }
        if let Some(branch) = observation.branch {
            self.store.db.execute(
                "INSERT INTO branches(id,grant_id,path,versions) VALUES(?1,?2,?3,?4)",
                params![
                    branch.id,
                    grant.id,
                    branch.path.to_string_lossy(),
                    serde_json::to_string(&branch.versions)?
                ],
            )?;
        }
        for path in &observation.writes {
            receipt
                .side_effects
                .extend(self.store.invalidate_dependents(&grant.scope, path)?);
        }
        receipt.side_effects.sort();
        receipt.side_effects.dedup();
        // Only a captured successful check can clear known invalidations. Its
        // complete observed input set must still match, and a later invalidation
        // generation must survive even when the bytes have changed back.
        receipt.restored_validity = Some(vec![]);
        receipt.restored_properties = Some(vec![]);
        for (artifact, generation) in validated {
            let latest = self
                .store
                .artifact_invalidation_generation(&grant.scope, &artifact.path)?;
            if latest > generation {
                continue;
            }
            let mut covered = previous.remove(&artifact.path).unwrap_or_default();
            let properties = receipt
                .validations
                .iter()
                .flatten()
                .flat_map(|check| check.properties.iter().flatten())
                .filter(|property| property.artifact == artifact)
                .cloned()
                .collect::<Vec<_>>();
            for property in properties {
                let binding = PropertyBinding {
                    obligation: property.obligation.clone(),
                    path: artifact.path.clone(),
                };
                match self.store.require_property_binding(&grant, &binding) {
                    Ok(()) => {}
                    Err(error) if matches!(error.code, -32001 | -32002 | -32004) => continue,
                    Err(error) => return Err(error),
                }
                self.store.db.execute("INSERT INTO property_validations(client,project,path,obligation_id,obligation_version,artifact_version,operation_id,generation) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(client,project,path,obligation_id) DO UPDATE SET obligation_version=excluded.obligation_version,artifact_version=excluded.artifact_version,operation_id=excluded.operation_id,generation=excluded.generation WHERE property_validations.generation<=excluded.generation",params![grant.scope.client,grant.scope.project,artifact.path,property.obligation.id,property.obligation.version,artifact.version,operation,generation])?;
                if !covered.contains(&property) {
                    covered.push(property.clone());
                }
                if !receipt
                    .restored_properties
                    .as_ref()
                    .unwrap()
                    .contains(&property)
                {
                    receipt.restored_properties.as_mut().unwrap().push(property);
                }
            }
            if self
                .store
                .properties_cover_artifact(&grant, &artifact, &covered)?
            {
                self.store.db.execute("DELETE FROM invalidated WHERE client=?1 AND project=?2 AND path=?3 AND generation<=?4", params![grant.scope.client,grant.scope.project,artifact.path,generation])?;
                receipt.restored_validity.as_mut().unwrap().push(artifact);
            }
        }
        if receipt.action.kind == ActionKind::Apply
            && receipt.status == EffectStatus::Succeeded
            && receipt.restored_validity.as_ref().is_none_or(Vec::is_empty)
        {
            receipt.output.push_str(if receipt.restored_properties.as_ref().is_some_and(|properties| !properties.is_empty()) {
                " Only the listed properties were restored; other declared properties remain unproven."
            } else { " Current validity was not restored; fresh authorized checks are required." });
        }
        let evidence_ref = format!("receipt:{}", crate::host::hash(operation.as_bytes()));
        let sources = receipt.evidence_ref.iter().cloned().collect();
        receipt.evidence_ref = Some(evidence_ref.clone());
        let sequence: i64 = self.store.db.query_row(
            "SELECT rowid FROM effects WHERE id=?1",
            [operation],
            |row| row.get(0),
        )?;
        let event = Event {
            id: evidence_ref,
            scope: grant.scope,
            run_id: receipt.run_id.clone(),
            producer: "ribosome-host".into(),
            sequence: sequence.to_string(),
            kind: if receipt.reconciled {
                "action_reconciliation"
            } else if receipt.action.kind == ActionKind::Check {
                "check_completion"
            } else {
                "action_completion"
            }
            .into(),
            timestamp_ms: receipt
                .finished_ms
                .clone()
                .unwrap_or_else(|| now_ms().to_string()),
            parents: vec![],
            correlation: operation.into(),
            artifacts: receipt.after.clone(),
            payload: serde_json::to_value(&receipt)?.as_object().unwrap().clone(),
            provenance: Provenance {
                origin: Origin::Observed,
                source_refs: sources,
                scenario_family: "host-effects".into(),
                split: derived_split(&grant.visible_splits),
                limitations: vec![],
            },
        };
        self.store.ingest(&event)?;
        self.store.db.execute(
            "UPDATE effects SET phase='finalized',body=?2 WHERE id=?1",
            params![operation, serde_json::to_string(&receipt)?],
        )?;
        transaction.commit()?;
        Ok(receipt)
    }
}
