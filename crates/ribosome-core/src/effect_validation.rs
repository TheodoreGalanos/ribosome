use crate::{
    contracts::*,
    effect_finalization::EffectPreflight,
    effects::Runtime,
    error::Result,
    host::{CheckOutput, hash},
    validation::{counter, now_ms},
};
use std::collections::BTreeMap;

pub(crate) fn policy_version(grant: &Grant) -> Result<String> {
    let mut policy = grant.clone();
    // Run-local delivery selection does not change the owner's checker policy.
    policy.prepared_run = None;
    Ok(hash(&serde_json::to_vec(&policy)?))
}

pub(crate) fn check_evidence(
    grant: &Grant,
    receipt: &ActionReceipt,
    result: &CheckOutput,
    generation: i64,
) -> Result<ValidationEvidence> {
    Ok(ValidationEvidence {
        check_ref: receipt
            .action
            .tool
            .clone()
            .expect("checked action has a registered tool"),
        checker_version: result.checker_version.clone(),
        policy_version: policy_version(grant)?,
        authority: "ribosome-host".into(),
        receipt_ref: format!("receipt:{}", hash(receipt.operation_id.as_bytes())),
        inputs: result.before.clone(),
        targets: result
            .after
            .iter()
            .filter(|artifact| result.validates.contains(&artifact.path))
            .cloned()
            .collect(),
        outcome: result.validation_outcome.clone(),
        branch_id: receipt.action.branch_id.clone(),
        generation: generation.to_string(),
        properties: Some(
            result
                .properties
                .iter()
                .filter_map(|binding| {
                    result
                        .after
                        .iter()
                        .find(|artifact| artifact.path == binding.path)
                        .map(|artifact| ValidatedProperty {
                            obligation: binding.obligation.clone(),
                            artifact: artifact.clone(),
                        })
                })
                .collect(),
        ),
    })
}

impl Runtime {
    pub(crate) fn transferable_validation(
        &self,
        grant: &Grant,
        receipt: &ActionReceipt,
        preflight: &EffectPreflight,
    ) -> Result<Vec<(ArtifactRef, i64)>> {
        if receipt.status != EffectStatus::Succeeded
            || receipt.outcome_basis != Some(EffectOutcomeBasis::ExecutionEstablished)
            || (receipt.action.kind != ActionKind::Apply && receipt.action.branch_id.is_some())
        {
            return Ok(vec![]);
        }
        let Some(validations) = receipt
            .validations
            .as_ref()
            .filter(|values| !values.is_empty())
        else {
            return Ok(vec![]);
        };
        let current_grant = self.store.run_grant(&receipt.run_id)?;
        if current_grant != *grant || counter(&grant.budget.deadline_ms)? <= now_ms() {
            return Ok(vec![]);
        }
        if receipt.action.kind == ActionKind::Apply {
            let Some(reference) = &preflight.intervention else {
                return Ok(vec![]);
            };
            if !self
                .store
                .record(grant, &reference.id)
                .is_ok_and(|record| record.version == reference.version)
            {
                return Ok(vec![]);
            }
            if self
                .store
                .require_attachment_write(&receipt.run_id, &receipt.action)
                .is_err()
            {
                return Ok(vec![]);
            }
        }
        let policy = policy_version(grant)?;
        let mut targets: BTreeMap<String, (ArtifactRef, i64)> = BTreeMap::new();
        for check in validations {
            if check.outcome != ValidationEvidenceOutcome::Passed
                || check.policy_version != policy
                || !self
                    .host
                    .checker_version(&check.check_ref)
                    .is_ok_and(|version| version == check.checker_version)
                || !check.inputs.iter().chain(&check.targets).all(|input| {
                    self.host
                        .version(grant, &input.path, None)
                        .is_ok_and(|current| current == *input)
                })
            {
                return Ok(vec![]);
            }
            let generation = counter(&check.generation)?.min(i64::MAX as u64) as i64;
            for target in &check.targets {
                if receipt.action.kind != ActionKind::Apply
                    || receipt.action.path.as_ref() == Some(&target.path)
                {
                    targets
                        .entry(target.path.clone())
                        .and_modify(|(_, cutoff)| *cutoff = (*cutoff).min(generation))
                        .or_insert((target.clone(), generation));
                }
            }
        }
        if preflight.checks.len() != validations.len()
            || preflight.checks.iter().any(|(name, version)| {
                !validations
                    .iter()
                    .any(|check| check.check_ref == *name && check.checker_version == *version)
            })
        {
            return Ok(vec![]);
        }
        // Every required check participates in transfer. A check performed
        // before a newer invalidation cannot be made current by a later check.
        let oldest = validations
            .iter()
            .map(|check| counter(&check.generation))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .min()
            .unwrap_or(0)
            .min(i64::MAX as u64) as i64;
        Ok(targets
            .into_values()
            .map(|(target, generation)| (target, generation.min(oldest)))
            .collect())
    }
}
