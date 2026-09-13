use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    store::Store,
    validation::{derived_split, id, now_ms},
};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;

impl Store {
    pub(crate) fn obligation_ids(&self, scope: &Scope, path: &str) -> Result<Vec<String>> {
        let ids = self.db.prepare("SELECT id FROM records WHERE client=?1 AND project=?2 AND kind='obligation' AND json_extract(body,'$.retired')=0 AND EXISTS(SELECT 1 FROM json_each(body,'$.body.affected_outputs') WHERE value=?3) ORDER BY id LIMIT 1001")?
            .query_map(params![scope.client,scope.project,path],|row|row.get(0))?.collect::<std::result::Result<Vec<String>,_>>()?;
        if ids.len() > 1000 {
            return Err(Error::exhausted(
                "artifact has more than 1000 declared obligations; narrow its property set",
            ));
        }
        Ok(ids)
    }

    pub(crate) fn require_property_binding(
        &self,
        grant: &Grant,
        binding: &PropertyBinding,
    ) -> Result<()> {
        let record = self.record(grant, &binding.obligation.id)?;
        if record.kind != RecordKind::Obligation || record.version != binding.obligation.version {
            return Err(Error::conflict(
                "registered property no longer matches its obligation version",
            ));
        }
        let obligation: Obligation =
            serde_json::from_value(serde_json::Value::Object(record.body))?;
        if !grant.paths.contains(&binding.path)
            || !obligation.affected_outputs.contains(&binding.path)
        {
            return Err(Error::denied(
                "registered property target is outside the obligation or granted artifacts",
            ));
        }
        Ok(())
    }

    pub(crate) fn properties_cover_artifact(
        &self,
        grant: &Grant,
        artifact: &ArtifactRef,
        candidates: &[ValidatedProperty],
    ) -> Result<bool> {
        let ids = self.obligation_ids(&grant.scope, &artifact.path)?;
        if ids.is_empty() {
            return Ok(false);
        }
        let generation = self.artifact_invalidation_generation(&grant.scope, &artifact.path)?;
        for id in ids {
            let record = match self.record(grant, &id) {
                Ok(record) => record,
                Err(error) if matches!(error.code, -32001 | -32004) => return Ok(false),
                Err(error) => return Err(error),
            };
            let property = ValidatedProperty {
                obligation: VersionRef {
                    id,
                    version: record.version,
                },
                artifact: artifact.clone(),
            };
            if !candidates.contains(&property) {
                return Ok(false);
            }
            let supported:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM property_validations WHERE client=?1 AND project=?2 AND path=?3 AND obligation_id=?4 AND obligation_version=?5 AND artifact_version=?6 AND generation>=?7)",params![grant.scope.client,grant.scope.project,artifact.path,property.obligation.id,property.obligation.version,artifact.version,generation],|row|row.get(0))?;
            if !supported {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Runtime {
    pub(crate) fn require_property_bindings(
        &self,
        grant: &Grant,
        bindings: &[PropertyBinding],
    ) -> Result<()> {
        for binding in bindings {
            self.store.require_property_binding(grant, binding)?;
        }
        Ok(())
    }

    /// A fresh assessment of declared, visible properties. Retained tool
    /// results are historical observations and must not authorize a later edit.
    pub fn artifact_validity(
        &self,
        grant: &Grant,
        request: &ArtifactValidityRequest,
    ) -> Result<ArtifactValidity> {
        let artifact = self.host.version(grant, &request.path, None)?;
        if artifact.version == "absent" {
            return Err(Error::missing("artifact does not exist"));
        }
        let properties = self.assess_properties(grant, &artifact)?;
        let snapshot = id();
        let generation = self
            .store
            .artifact_invalidation_generation(&grant.scope, &artifact.path)?;
        let body =
            serde_json::json!({"artifact":artifact,"snapshot_id":snapshot,"version_only":true});
        self.store.db.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,available,body,version_only) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,'historical',1,?8,1)",params![snapshot,grant.scope.client,grant.scope.project,grant.id,artifact.path,serde_json::to_value(derived_split(&grant.visible_splits))?.as_str(),artifact.version,body.to_string()])?;
        Ok(ArtifactValidity {
            artifact,
            snapshot_id: snapshot,
            observed_ms: now_ms().to_string(),
            generation: generation.to_string(),
            properties,
        })
    }

    pub(crate) fn assess_properties(
        &self,
        grant: &Grant,
        artifact: &ArtifactRef,
    ) -> Result<Vec<PropertyAssessment>> {
        let mut receipts: BTreeMap<String, Option<ActionReceipt>> = BTreeMap::new();
        let generation = self
            .store
            .artifact_invalidation_generation(&grant.scope, &artifact.path)?;
        let mut assessments = vec![];
        for id in self.store.obligation_ids(&grant.scope, &artifact.path)? {
            let record = match self.store.record(grant, &id) {
                Ok(record) => record,
                Err(error) if matches!(error.code, -32001 | -32004) => continue,
                Err(error) => return Err(error),
            };
            let reference = VersionRef {
                id: record.id,
                version: record.version,
            };
            let fact:Option<(String,String,String,i64)>=self.store.db.query_row("SELECT obligation_version,artifact_version,operation_id,generation FROM property_validations WHERE client=?1 AND project=?2 AND path=?3 AND obligation_id=?4",params![grant.scope.client,grant.scope.project,artifact.path,reference.id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let mut assessment = PropertyAssessment {
                obligation: reference.clone(),
                state: PropertyAssessmentState::Unproven,
                evidence_refs: vec![],
            };
            if let Some((property_version, artifact_version, operation, validated_generation)) =
                fact
            {
                if !receipts.contains_key(&operation) {
                    let body: String = self.store.db.query_row(
                        "SELECT body FROM effects WHERE id=?1",
                        [&operation],
                        |row| row.get(0),
                    )?;
                    let receipt: ActionReceipt = serde_json::from_str(&body)?;
                    let eligible = self.property_receipt_current(grant, &receipt)?;
                    receipts.insert(operation.clone(), eligible.then_some(receipt));
                }
                assessment.state = PropertyAssessmentState::Stale;
                if let Some(receipt) = receipts.get(&operation).and_then(Option::as_ref) {
                    let property = ValidatedProperty {
                        obligation: reference,
                        artifact: artifact.clone(),
                    };
                    if property_version == property.obligation.version
                        && artifact_version == artifact.version
                        && validated_generation >= generation
                        && receipt
                            .restored_properties
                            .as_ref()
                            .is_some_and(|values| values.contains(&property))
                    {
                        assessment.state = PropertyAssessmentState::Validated;
                        if let Some(reference) = &receipt.evidence_ref {
                            assessment.evidence_refs.push(reference.clone());
                        }
                    }
                }
            }
            assessments.push(assessment);
        }
        Ok(assessments)
    }

    fn property_receipt_current(&self, grant: &Grant, receipt: &ActionReceipt) -> Result<bool> {
        if receipt.status != EffectStatus::Succeeded
            || receipt.outcome_basis != Some(EffectOutcomeBasis::ExecutionEstablished)
        {
            return Ok(false);
        }
        let Some(source) = &receipt.evidence_ref else {
            return Ok(false);
        };
        if self.store.require_source(grant, "event", source).is_err() {
            return Ok(false);
        }
        let Some(checks) = receipt
            .validations
            .as_ref()
            .filter(|values| !values.is_empty())
        else {
            return Ok(false);
        };
        let policy = crate::effect_validation::policy_version(grant)?;
        Ok(checks.iter().all(|check| {
            check.outcome == ValidationEvidenceOutcome::Passed
                && check.policy_version == policy
                && self
                    .store
                    .require_source(grant, "event", &check.receipt_ref)
                    .is_ok()
                && self
                    .host
                    .checker_version(&check.check_ref)
                    .is_ok_and(|version| version == check.checker_version)
                && check.inputs.iter().chain(&check.targets).all(|input| {
                    self.host
                        .version(grant, &input.path, None)
                        .is_ok_and(|current| current == *input)
                })
        }))
    }
}
