use crate::{
    allocations::allocation_id,
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    process,
    validation::{counter, decode, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationCase {
    pub id: String,
    pub family: String,
    pub split: Split,
    pub input: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionPolicy {
    pub id: String,
    pub context: String,
    pub evaluator: String,
    pub evaluator_version: String,
    pub case_ids: Vec<String>,
    pub required_checks: Vec<String>,
    pub metric: String,
    pub min_quality: f64,
    pub min_improvement: f64,
    pub repetitions: u32,
    pub allowed_cells: Vec<String>,
    pub retain_learning_memory: bool,
    pub max_evaluations: u32,
    /// A development-only policy accepts authored cases and cannot admit an
    /// implementation. Its evaluator and required checks remain host-owned.
    #[serde(default)]
    pub allow_generated_development_cases: bool,
    #[serde(default)]
    pub case_budget: Option<Budget>,
    #[serde(default)]
    pub study_objective: Option<ExperimentStudyObjective>,
    #[serde(default)]
    pub learning_cost: Option<LearningCost>,
}

/// Scoped accounting for a host-owned evaluator case. The worker bridge
/// cannot supply this object or choose another case's allocation.
pub struct EvaluationAccount<'a> {
    store: &'a crate::store::Store,
    pub(crate) runtime: &'a Runtime,
    pub(crate) run_id: &'a str,
    pub(crate) allocation_id: &'a str,
    cancellation: &'a std::sync::atomic::AtomicBool,
}

impl EvaluationAccount<'_> {
    fn require_not_cancelled(&self) -> Result<()> {
        if self.cancellation.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::denied("experiment cancelled"));
        }
        Ok(())
    }
    pub fn permit(&self, request: &PermitRequest) -> Result<Permit> {
        self.require_not_cancelled()?;
        if request.call_id.is_none() {
            return Err(Error::invalid(
                "metered evaluators require a stable provider call ID",
            ));
        }
        self.store
            .permit_in_allocation(self.run_id, request, Some(self.allocation_id))
    }
    fn owns(&self, permit: &str) -> Result<()> {
        let owned: bool = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM permits WHERE id=?1 AND run_id=?2 AND allocation_id=?3)",
            params![permit, self.run_id, self.allocation_id],
            |r| r.get(0),
        )?;
        if owned {
            Ok(())
        } else {
            Err(Error::denied("permit belongs to another evaluation"))
        }
    }
    pub fn dispatch(&self, permit: &Permit) -> Result<()> {
        self.require_not_cancelled()?;
        self.owns(&permit.id)?;
        self.store.dispatch_permit(self.run_id, &permit.id)
    }
    pub fn usage(&self, usage: &Usage) -> Result<()> {
        self.owns(&usage.permit_id)?;
        self.store.usage(self.run_id, usage)
    }
    pub fn release(&self, permit: &Permit) -> Result<()> {
        self.owns(&permit.id)?;
        self.store.release_permit(self.run_id, &permit.id)
    }
    pub fn status(&self) -> Result<BudgetStatus> {
        self.store
            .budget_status(&self.store.run_grant(self.run_id)?, self.allocation_id)
    }
}

pub trait Evaluator: Send + Sync {
    /// True only when every provider call uses EvaluationAccount, or this
    /// trusted adapter makes no provider calls. Process output is not billing evidence.
    fn provider_usage_is_metered(&self) -> bool {
        false
    }
    fn configuration(&self) -> Result<Value> {
        Ok(Value::Null)
    }
    fn material_refs(&self) -> Vec<VersionRef> {
        vec![]
    }
    fn validate_study(&self, _experiment: &Experiment, _policy: &AdmissionPolicy) -> Result<()> {
        Ok(())
    }
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
        account: &EvaluationAccount<'_>,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEvaluator {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl Evaluator for CommandEvaluator {
    fn configuration(&self) -> Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
        _account: &EvaluationAccount<'_>,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        if !self.program.is_absolute() || !(1..=300000).contains(&self.timeout_ms) {
            return Err(Error::invalid(
                "evaluator requires an absolute program and bounded timeout",
            ));
        }
        let timeout = self
            .timeout_ms
            .min(counter(&task.budget.deadline_ms)?.saturating_sub(now_ms()));
        let output = process::execute(
            &self.program,
            &self.args,
            workspace,
            Some(serde_json::to_vec(task)?),
            &BTreeMap::new(),
            Duration::from_millis(timeout),
            Some(cancellation),
        )?;
        if !output.success {
            return Err(Error::internal(if output.timed_out {
                "evaluator deadline exceeded"
            } else {
                "evaluator process failed; no passing evaluation recorded"
            }));
        }
        let mut result: EvaluationObservation = decode(
            "EvaluationObservation",
            serde_json::from_str(&output.stdout)?,
        )?;
        result.measurements.push(Measurement {
            name: "evaluator_latency".into(),
            value: Some(output.elapsed_ms as f64),
            unit: "ms".into(),
        });
        Ok(result)
    }
}

#[derive(Clone, Default)]
pub struct Laboratory {
    registry: std::sync::Arc<LaboratoryRegistry>,
}

#[derive(Default)]
struct LaboratoryRegistry {
    policies: BTreeMap<String, AdmissionPolicy>,
    cases: BTreeMap<String, EvaluationCase>,
    evaluators: BTreeMap<String, Box<dyn Evaluator>>,
}

impl Laboratory {
    fn registry_mut(&mut self) -> Result<&mut LaboratoryRegistry> {
        std::sync::Arc::get_mut(&mut self.registry).ok_or_else(|| {
            Error::conflict("laboratory configuration is shared by an active supervisor")
        })
    }

    pub fn register_evaluator(&mut self, id: String, evaluator: Box<dyn Evaluator>) -> Result<()> {
        if self.registry.evaluators.contains_key(&id) {
            return Err(Error::conflict("evaluator already registered"));
        }
        self.registry_mut()?.evaluators.insert(id, evaluator);
        Ok(())
    }
    pub fn register_case(&mut self, case: EvaluationCase) -> Result<()> {
        if self.registry.cases.contains_key(&case.id) {
            return Err(Error::conflict("case already registered"));
        }
        self.registry_mut()?.cases.insert(case.id.clone(), case);
        Ok(())
    }
    pub fn register_policy(&mut self, policy: AdmissionPolicy) -> Result<()> {
        if self.registry.policies.contains_key(&policy.id) {
            return Err(Error::conflict("policy already registered"));
        }
        if (policy.case_ids.is_empty() != policy.allow_generated_development_cases)
            || policy.case_ids.len() > 100
            || policy.repetitions == 0
            || policy.repetitions > 100
            || policy.max_evaluations == 0
            || !policy.min_quality.is_finite()
            || !policy.min_improvement.is_finite()
            || policy.required_checks.is_empty()
            || policy
                .case_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != policy.case_ids.len()
        {
            return Err(Error::invalid("invalid evaluation policy"));
        }
        if policy.study_objective == Some(ExperimentStudyObjective::SystemBenefit)
            && policy.metric != "verified_success"
        {
            return Err(Error::invalid(
                "system-benefit studies use verified_success as their primary metric",
            ));
        }
        if !self.registry.evaluators.contains_key(&policy.evaluator)
            || policy
                .case_ids
                .iter()
                .any(|id| !self.registry.cases.contains_key(id))
        {
            return Err(Error::invalid(
                "policy references an unregistered evaluator or case",
            ));
        }
        self.registry_mut()?
            .policies
            .insert(policy.id.clone(), policy);
        Ok(())
    }

    fn study_cases(
        &self,
        policy: &AdmissionPolicy,
        experiment: &Experiment,
    ) -> Result<Vec<EvaluationCase>> {
        if !policy.allow_generated_development_cases {
            if experiment.development_cases.is_some() || policy.case_ids != experiment.case_ids {
                return Err(Error::denied("experiment changes fixed evaluation cases"));
            }
            return Ok(policy
                .case_ids
                .iter()
                .map(|id| self.registry.cases[id].clone())
                .collect());
        }
        let generated = experiment
            .development_cases
            .as_ref()
            .ok_or_else(|| Error::invalid("development policy requires generated cases"))?;
        if generated.iter().map(|case| &case.id).collect::<Vec<_>>()
            != experiment.case_ids.iter().collect::<Vec<_>>()
        {
            return Err(Error::invalid(
                "development case IDs must match the declared study order",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        generated.iter().map(|case| {
            if !seen.insert(&case.id) || self.registry.cases.contains_key(&case.id) {
                return Err(Error::denied("generated case IDs must be distinct from registered cases"));
            }
            if case.provenance.origin != Origin::Synthetic || case.provenance.split != Split::Development || case.provenance.source_refs.is_empty() {
                return Err(Error::denied("generated cases require synthetic development provenance and source lineage"));
            }
            if !experiment.scenario_families.contains(&case.provenance.scenario_family) {
                return Err(Error::invalid("generated case family is absent from the experiment lineage"));
            }
            Ok(EvaluationCase { id: case.id.clone(), family: case.provenance.scenario_family.clone(), split: Split::Development, input: case.input.clone() })
        }).collect()
    }
}

impl Runtime {
    pub fn run_experiment(&self, run_id: &str, experiment_id: &str) -> Result<ExperimentResult> {
        let grant = self.store.require_active(run_id)?;
        if grant.mode == Mode::Observe {
            return Err(Error::denied(
                "evaluator execution requires sandbox or apply mode",
            ));
        }
        let record = self.store.record(&grant, experiment_id)?;
        if record.kind != RecordKind::Experiment {
            return Err(Error::invalid("record is not an experiment"));
        }
        let experiment: Experiment = serde_json::from_value(Value::Object(record.body.clone()))?;
        if !experiment.selection_frozen {
            return Err(Error::denied("freeze selection before evaluation"));
        }
        let policy = self
            .laboratory
            .registry
            .policies
            .get(&experiment.policy_id)
            .ok_or_else(|| Error::denied("no registered evaluation policy"))?;
        if policy.context != grant.context || policy.repetitions != experiment.repetitions {
            return Err(Error::denied(
                "experiment changes fixed context or repetitions",
            ));
        }
        let evaluator = &self.laboratory.registry.evaluators[&policy.evaluator];
        evaluator.validate_study(&experiment, policy)?;
        if policy.study_objective != experiment.study_objective
            || policy.learning_cost != experiment.learning_cost
        {
            return Err(Error::denied(
                "study objective or learning cost differs from the host policy",
            ));
        }
        let cases = self.laboratory.study_cases(policy, &experiment)?;
        let memory_start = self.experiment_memory(&grant, &experiment)?;
        let pinned = serde_json::to_string(
            &json!({"experiment":experiment,"materials":self.study_materials(&grant,&experiment)?,"evaluator_config":evaluator.configuration()?,"policy":policy,"cases":cases,"development_cases":experiment.development_cases,"memory_start":memory_start}),
        )?;
        let existing: Option<(String, Option<String>)> = self
            .store
            .db
            .query_row(
                "SELECT policy,result FROM experiments WHERE id=?1",
                [experiment_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((previous, result)) = existing {
            if study_inputs(&previous)? != study_inputs(&pinned)? {
                return Err(Error::conflict("frozen evaluator policy or cases changed"));
            }
            return result.map(|r|Ok(serde_json::from_str(&r)?)).unwrap_or_else(||Err(Error::conflict("experiment interrupted; owner must inspect partial evidence before further protected evaluation")));
        }
        let protected = cases.iter().any(|case| case.split == Split::Holdout);
        let mut variants = vec![
            Variant {
                arm: "baseline".into(),
                implementation: experiment.baseline.clone(),
            },
            Variant {
                arm: "candidate".into(),
                implementation: experiment.candidate.clone(),
            },
        ];
        for variant in &experiment.variants {
            if variants.iter().any(|v| v.arm == variant.arm) {
                return Err(Error::invalid("duplicate experiment arm"));
            }
            variants.push(variant.clone());
        }
        if experiment.study_objective == Some(ExperimentStudyObjective::SystemBenefit)
            && crate::study_report::controls(&experiment)
                .iter()
                .any(|arm| !variants.iter().any(|v| v.arm == *arm))
        {
            return Err(Error::invalid(
                "system-benefit study requires baseline, retry, critique, care and candidate arms",
            ));
        }
        let total = variants.len() * cases.len() * policy.repetitions as usize;
        if total > policy.max_evaluations as usize || total > 1000 {
            return Err(Error::exhausted("study evaluation budget exceeded"));
        }
        if counter(&experiment.budget.deadline_ms)? > counter(&grant.budget.deadline_ms)?
            || experiment.budget.max_calls > grant.budget.max_calls
            || counter(&experiment.budget.max_tokens)? > counter(&grant.budget.max_tokens)?
            || counter(&experiment.budget.max_cost_microusd)?
                > counter(&grant.budget.max_cost_microusd)?
            || experiment.budget.max_actions > grant.budget.max_actions
            || experiment.budget.max_work_items > grant.budget.max_work_items
            || experiment.budget.max_depth > grant.budget.max_depth
        {
            return Err(Error::denied("experiment budget exceeds grant"));
        }
        let mut implementations = BTreeMap::new();
        for variant in &variants {
            let material = self.store.record(&grant, &variant.implementation.id)?;
            if material.kind != RecordKind::Implementation {
                return Err(Error::invalid("arm must reference an implementation"));
            }
            let implementation: Implementation =
                serde_json::from_value(Value::Object(material.body))?;
            if implementation.version != variant.implementation.version {
                return Err(Error::conflict("implementation version mismatch"));
            }
            if !implementation
                .required_capabilities
                .iter()
                .all(|t| grant.tools.contains(t))
            {
                return Err(Error::denied("candidate requires ungranted capabilities"));
            }
            implementations.insert(variant.arm.clone(), implementation);
        }
        if protected {
            for source in variants
                .iter()
                .map(|v| &v.implementation.id)
                .chain(experiment.memory_start_refs.iter())
                .chain(evaluator.material_refs().iter().map(|r| &r.id))
            {
                let families = self.store.db.prepare("WITH RECURSIVE ancestry(kind,id) AS (SELECT 'record',?1 UNION SELECT e.source_kind,e.source_id FROM source_edges e JOIN ancestry a ON e.subject_kind=a.kind AND e.subject_id=a.id) SELECT json_extract(r.body,'$.provenance.scenario_family') FROM records r JOIN ancestry a ON a.kind='record' AND a.id=r.id UNION SELECT json_extract(e.body,'$.provenance.scenario_family') FROM events e JOIN ancestry a ON a.kind='event' AND a.id=e.id")?.query_map([source],|r|r.get::<_,String>(0))?.collect::<std::result::Result<std::collections::HashSet<_>,_>>()?;
                if cases
                    .iter()
                    .any(|case| case.split == Split::Holdout && families.contains(&case.family))
                {
                    return Err(Error::denied(
                        "protected recipient family overlaps candidate or memory source lineage",
                    ));
                }
            }
        }
        let preparation = self.store.write_transaction()?;
        if protected {
            for case in cases.iter().filter(|case| case.split == Split::Holdout) {
                let prior: Option<String> = self.store.db.query_row(
                    "SELECT experiment_id FROM protected_exposures WHERE client=?1 AND project=?2 AND family=?3",
                    params![grant.scope.client,grant.scope.project,case.family], |r| r.get(0)).optional()?;
                if prior.as_ref().is_some_and(|prior| prior != experiment_id) {
                    return Err(Error::denied(
                        "protected family already exposed in this project; use a new independent recipient family",
                    ));
                }
                self.store.db.execute(
                    "INSERT OR IGNORE INTO protected_exposures VALUES(?1,?2,?3,?4,?5)",
                    params![
                        grant.scope.client,
                        grant.scope.project,
                        case.family,
                        experiment_id,
                        policy.id
                    ],
                )?;
            }
        }
        self.store.db.execute(
            "INSERT INTO experiments(id,grant_id,policy_id,policy) VALUES (?1,?2,?3,?4)",
            params![experiment_id, grant.id, policy.id, pinned],
        )?;
        let run_allocation = self.store.run_allocation_in(run_id)?;
        let study_allocation = self.store.allocate_in(
            &grant,
            &BudgetAllocationRequest {
                id: allocation_id("study", experiment_id),
                parent_id: run_allocation.id,
                cause_id: experiment_id.into(),
                purpose: "experiment".into(),
                budget: experiment.budget.clone(),
            },
        )?;
        // Declare the complete matrix before dispatch. These are ceilings,
        // not independent wallets; permits charge every ancestor atomically.
        let mut case_allocations = BTreeMap::new();
        for repetition in 0..policy.repetitions {
            for variant in &variants {
                let arm = self.store.allocate_in(
                    &grant,
                    &BudgetAllocationRequest {
                        id: allocation_id(
                            "arm",
                            &serde_json::to_string(&(experiment_id, &variant.arm, repetition))?,
                        ),
                        parent_id: study_allocation.id.clone(),
                        cause_id: experiment_id.into(),
                        purpose: "arm".into(),
                        budget: experiment.budget.clone(),
                    },
                )?;
                for case in &cases {
                    let allocation = self.store.allocate_in(
                        &grant,
                        &BudgetAllocationRequest {
                            id: allocation_id(
                                "case",
                                &serde_json::to_string(&(
                                    experiment_id,
                                    &variant.arm,
                                    repetition,
                                    &case.id,
                                ))?,
                            ),
                            parent_id: arm.id.clone(),
                            cause_id: experiment_id.into(),
                            purpose: "evaluator".into(),
                            budget: policy
                                .case_budget
                                .clone()
                                .unwrap_or_else(|| experiment.budget.clone()),
                        },
                    )?;
                    case_allocations.insert(
                        (repetition, variant.arm.clone(), case.id.clone()),
                        allocation.id,
                    );
                }
            }
        }
        preparation.commit()?;
        let mut stopped: Option<(Disposition, String)> = None;
        let mut study_disposition = Disposition::Completed;
        let mut usage_complete = true;
        let cancellation = self.cancellation(run_id);
        let directory = tempfile::Builder::new()
            .prefix("ribosome-study-")
            .tempdir_in(&self.state_dir)?;
        // Register ownership before an evaluator can write copied inputs or
        // output. A later host can remove files left by a terminated process.
        self.store.db.execute(
            "UPDATE experiments SET policy=json_set(policy,'$.workspace',json(?2)) WHERE id=?1",
            params![
                experiment_id,
                serde_json::to_string(&json!({"path":directory.path(),"run_id":run_id}))?
            ],
        )?;
        let mut evaluation_refs = Vec::new();
        let mut evaluations = Vec::new();
        for repetition in 0..policy.repetitions {
            let mut order = variants.clone();
            order.rotate_left(repetition as usize % variants.len());
            if (repetition as usize / variants.len()) % 2 == 1 {
                order.reverse();
            }
            for variant in &order {
                let mut arm_disposition = Disposition::Completed;
                // Arm and repetition receive separate stores. A learning study
                // may retain within its arm across cases, never across arms.
                let arm_namespace = id();
                let arm_dir = directory.path().join(&arm_namespace);
                std::fs::create_dir(&arm_dir)?;
                for case in &cases {
                    let memory_namespace = if policy.retain_learning_memory {
                        arm_namespace.clone()
                    } else {
                        id()
                    };
                    let case_dir = if policy.retain_learning_memory {
                        arm_dir.clone()
                    } else {
                        let path = arm_dir.join(&memory_namespace);
                        std::fs::create_dir(&path)?;
                        path
                    };
                    let allocation = case_allocations
                        [&(repetition, variant.arm.clone(), case.id.clone())]
                        .clone();
                    let task = EvaluationTask {
                        allocation_id: allocation.clone(),
                        experiment_id: experiment_id.into(),
                        implementation: implementations[&variant.arm].clone(),
                        implementation_ref: Some(variant.implementation.clone()),
                        case_id: case.id.clone(),
                        case_input: case.input.clone(),
                        arm: variant.arm.clone(),
                        repetition,
                        memory_namespace,
                        budget: self.store.allocation_remaining(&grant, &allocation)?,
                        memory_start: memory_start.clone(),
                    };
                    let evaluator = &self.laboratory.registry.evaluators[&policy.evaluator];
                    let account = EvaluationAccount {
                        store: &self.store,
                        runtime: self,
                        run_id,
                        allocation_id: &allocation,
                        cancellation: &cancellation,
                    };
                    let mut status = EvaluationExecutionStatus::Completed;
                    let observed = if let Some((_, reason)) = &stopped {
                        status = EvaluationExecutionStatus::NotStarted;
                        Err(Error::exhausted(reason.clone()))
                    } else if cancellation.load(std::sync::atomic::Ordering::SeqCst) {
                        status = EvaluationExecutionStatus::Cancelled;
                        Err(Error::denied("experiment cancelled"))
                    } else {
                        self.store.require_active(run_id).and_then(|_| {
                            evaluator.evaluate(&task, &case_dir, &account, &cancellation)
                        })
                    }
                    .and_then(|observation| {
                        validate(
                            "EvaluationObservation",
                            &serde_json::to_value(&observation)?,
                        )?;
                        Ok(observation)
                    });
                    let observation = match observed {
                        Ok(value) => value,
                        Err(error) => {
                            if status == EvaluationExecutionStatus::Completed {
                                status = if cancellation.load(std::sync::atomic::Ordering::SeqCst) {
                                    EvaluationExecutionStatus::Cancelled
                                } else if error.code == Error::exhausted("").code {
                                    EvaluationExecutionStatus::Exhausted
                                } else {
                                    EvaluationExecutionStatus::Failed
                                };
                            }
                            let remaining = self
                                .store
                                .allocation_remaining(&grant, &study_allocation.id)?;
                            let shared_exhausted = (experiment.budget.max_calls > 0
                                && remaining.max_calls == 0)
                                || (counter(&experiment.budget.max_tokens)? > 0
                                    && counter(&remaining.max_tokens)? == 0)
                                || (counter(&experiment.budget.max_cost_microusd)? > 0
                                    && counter(&remaining.max_cost_microusd)? == 0)
                                || now_ms() >= counter(&remaining.deadline_ms)?;
                            if status == EvaluationExecutionStatus::Cancelled
                                || (status == EvaluationExecutionStatus::Exhausted
                                    && shared_exhausted)
                            {
                                stopped = Some((
                                    if status == EvaluationExecutionStatus::Cancelled {
                                        Disposition::Cancelled
                                    } else {
                                        Disposition::Exhausted
                                    },
                                    error.message.clone(),
                                ));
                            }
                            EvaluationObservation {
                                measurements: vec![],
                                checks: vec![],
                                output: format!("Evaluation unavailable: {}", error.message),
                                passed: None,
                                descriptor: None,
                            }
                        }
                    };
                    let usage = account.status()?.usage;
                    let case_usage_complete = evaluator.provider_usage_is_metered()
                        && usage.unknown_calls == 0
                        && usage.undispatched_calls == 0;
                    usage_complete &= case_usage_complete;
                    let run_refs = self.store.db.prepare("WITH RECURSIVE family(id) AS (SELECT ?1 UNION ALL SELECT a.id FROM budget_allocations a JOIN family f ON a.parent_id=f.id) SELECT run_id FROM run_allocations WHERE allocation_id IN family ORDER BY run_id")?.query_map([&allocation],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
                    let evaluation = Evaluation {
                        run_refs: Some(run_refs),
                        allocation_id: Some(allocation.clone()),
                        execution_status: Some(status.clone()),
                        usage_complete: Some(case_usage_complete),
                        experiment_ref: experiment_id.into(),
                        implementation: variant.implementation.clone(),
                        case_id: case.id.clone(),
                        arm: variant.arm.clone(),
                        repetition,
                        evaluator: policy.evaluator.clone(),
                        evaluator_version: policy.evaluator_version.clone(),
                        policy_id: policy.id.clone(),
                        passed: observation.passed,
                        measurements: observation.measurements,
                        checks: observation.checks,
                        memory_namespace: task.memory_namespace,
                        output: observation.output,
                        artifact_refs: vec![],
                        descriptor: observation.descriptor,
                    };
                    let provenance=Provenance{origin:Origin::Observed,source_refs:std::iter::once(experiment_id.to_owned()).chain(evaluator.material_refs().into_iter().map(|r|r.id)).collect(),scenario_family:case.family.clone(),split:case.split.clone(),limitations:vec!["Evaluator execution is observed; scenario origin follows the host study definition.".into()]};
                    let mut evaluator_grant = grant.clone();
                    evaluator_grant.visible_splits =
                        vec![Split::Development, Split::Evaluation, Split::Holdout];
                    let submission = RecordSubmission {
                        kind: RecordKind::Evaluation,
                        provenance,
                        body: serde_json::to_value(&evaluation)?
                            .as_object()
                            .unwrap()
                            .clone(),
                        id: None,
                        expected_version: None,
                    };
                    let saved = self.store.submit(&evaluator_grant, &submission, true)?;
                    let disposition = match status {
                        EvaluationExecutionStatus::Completed => Disposition::Completed,
                        EvaluationExecutionStatus::Cancelled => Disposition::Cancelled,
                        EvaluationExecutionStatus::Exhausted => Disposition::Exhausted,
                        EvaluationExecutionStatus::NotStarted => stopped
                            .as_ref()
                            .ok_or_else(|| {
                                Error::internal("unstarted evaluation has no stop cause")
                            })?
                            .0
                            .clone(),
                        EvaluationExecutionStatus::Failed => Disposition::Failed,
                    };
                    if disposition != Disposition::Completed {
                        // Preserve ordinary failures even if later cases succeed.
                        // Cancellation or exhaustion ends dispatch and becomes
                        // the terminal cause for the unfinished matrix.
                        arm_disposition = disposition.clone();
                        study_disposition = disposition.clone();
                    }
                    self.store
                        .finish_allocation(&grant, &allocation, disposition)?;
                    evaluation_refs.push(saved.id);
                    evaluations.push(evaluation);
                }
                self.store.finish_allocation(
                    &grant,
                    &allocation_id(
                        "arm",
                        &serde_json::to_string(&(experiment_id, &variant.arm, repetition))?,
                    ),
                    arm_disposition,
                )?;
            }
        }
        let complete = study_disposition == Disposition::Completed;
        let decision = if complete {
            assess(policy, &evaluations, total)
        } else {
            AdmissionDecision::Inconclusive
        };
        self.store
            .finish_allocation(&grant, &study_allocation.id, study_disposition)?;
        let result = ExperimentResult {
            report: Some(crate::study_report::report(
                &experiment,
                policy,
                &cases,
                &evaluations,
                complete,
                &self
                    .store
                    .budget_status(&grant, &study_allocation.id)?
                    .usage,
            )),
            allocation_id: Some(study_allocation.id),
            complete: Some(complete),
            usage_complete: Some(usage_complete),
            planned_evaluations: Some(total as u32),
            evaluation_refs,
            decision: decision.clone(),
            summary: format!(
                "{} attributed evaluations. Decision: {}. {}",
                evaluations.len(),
                serde_json::to_value(decision)?.as_str().unwrap(),
                if policy.allow_generated_development_cases {
                    "Generated development cases and candidate checks do not establish admission."
                } else {
                    "Protected observations are only available to the evaluator authority."
                }
            ),
        };
        self.store.db.execute(
            "UPDATE experiments SET result=?2 WHERE id=?1",
            params![experiment_id, serde_json::to_string(&result)?],
        )?;
        Ok(result)
    }

    fn study_materials(
        &self,
        grant: &Grant,
        experiment: &Experiment,
    ) -> Result<Vec<RecordEnvelope>> {
        let extra = self.laboratory.registry.evaluators
            [&self.laboratory.registry.policies[&experiment.policy_id].evaluator]
            .material_refs();
        std::iter::once(&experiment.baseline)
            .chain(std::iter::once(&experiment.candidate))
            .chain(experiment.variants.iter().map(|v| &v.implementation))
            .chain(extra.iter())
            .map(|reference| self.store.record(grant, &reference.id))
            .collect()
    }

    fn experiment_memory(
        &self,
        grant: &Grant,
        experiment: &Experiment,
    ) -> Result<Vec<RecordEnvelope>> {
        let memories = experiment
            .memory_start_refs
            .iter()
            .map(|id| {
                let memory = self.store.record(grant, id).map_err(|error| Error {
                    code: error.code,
                    message: format!("memory_start_refs entry {id}: {}. Use saved Memory record IDs, or [] for no starting memory", error.message),
                })?;
                self.store.require_reference(grant, id)?;
                if memory.kind != RecordKind::Memory {
                    return Err(Error::invalid(
                        "experiment starting memory must reference memory records",
                    ));
                }
                Ok(memory)
            })
            .collect::<Result<Vec<_>>>()?;
        if serde_json::to_vec(&memories)?.len() > crate::validation::MAX_FRAME / 4 {
            return Err(Error::exhausted(
                "experiment starting memory exceeds 256 KiB",
            ));
        }
        Ok(memories)
    }

    pub fn request_admission(
        &self,
        run_id: &str,
        recommendation_id: &str,
    ) -> Result<RecordEnvelope> {
        let grant = self.store.require_active(run_id)?;
        let record = self.store.record(&grant, recommendation_id)?;
        if record.kind != RecordKind::Recommendation {
            return Err(Error::invalid("record is not a recommendation"));
        }
        let recommendation: Recommendation = serde_json::from_value(Value::Object(record.body))?;
        if recommendation.context != grant.context {
            return Err(Error::denied("recommendation context differs from grant"));
        }
        let mut evaluator_grant = grant.clone();
        evaluator_grant.visible_splits =
            vec![Split::Development, Split::Evaluation, Split::Holdout];
        let mut evaluations = Vec::new();
        let mut experiment_id = None;
        for reference in &recommendation.evaluation_refs {
            let record = self.store.record(&evaluator_grant, reference)?;
            if record.kind != RecordKind::Evaluation {
                return Err(Error::invalid("support is not an evaluation"));
            }
            let evaluation: Evaluation = serde_json::from_value(Value::Object(record.body))?;
            if experiment_id
                .as_ref()
                .is_some_and(|id| id != &evaluation.experiment_ref)
            {
                return Err(Error::denied("cannot combine unrelated experiments"));
            }
            experiment_id = Some(evaluation.experiment_ref.clone());
            evaluations.push(evaluation);
        }
        let experiment_id =
            experiment_id.ok_or_else(|| Error::denied("admission requires evaluation evidence"))?;
        let result: String = self
            .store
            .db
            .query_row(
                "SELECT result FROM experiments WHERE id=?1 AND grant_id=?2",
                params![experiment_id, grant.id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::denied("experiment was not executed under this grant"))?;
        let result: ExperimentResult = serde_json::from_str(&result)?;
        let mut expected = result.evaluation_refs.clone();
        expected.sort();
        let mut supplied = recommendation.evaluation_refs.clone();
        supplied.sort();
        if expected != supplied {
            return Err(Error::denied(
                "admission requires complete evidence, including control and failures",
            ));
        }
        let experiment = self.store.record(&grant, &experiment_id)?;
        let experiment: Experiment = serde_json::from_value(Value::Object(experiment.body))?;
        if experiment.candidate != recommendation.implementation {
            return Err(Error::denied(
                "recommendation candidate does not match tested candidate",
            ));
        }
        let policy = self
            .laboratory
            .registry
            .policies
            .get(&experiment.policy_id)
            .ok_or_else(|| Error::denied("admission policy unavailable"))?;
        if policy.allow_generated_development_cases {
            return Err(Error::denied(
                "generated development cases cannot establish admission; use a host-owned acceptance study",
            ));
        }
        let frozen: String = self.store.db.query_row(
            "SELECT policy FROM experiments WHERE id=?1",
            [&experiment_id],
            |r| r.get(0),
        )?;
        let current = serde_json::to_string(
            &json!({"experiment":experiment,"materials":self.study_materials(&grant,&experiment)?,"evaluator_config":self.laboratory.registry.evaluators[&policy.evaluator].configuration()?,"policy":policy,"cases":self.laboratory.study_cases(policy, &experiment)?,"development_cases":experiment.development_cases,"memory_start":self.experiment_memory(&grant, &experiment)?}),
        )?;
        if study_inputs(&frozen)? != study_inputs(&current)? {
            return Err(Error::conflict(
                "admission policy or evaluation cases changed after the study",
            ));
        }
        let decision = assess(policy, &evaluations, expected.len());
        let admission = Admission {
            implementation: recommendation.implementation.clone(),
            context: grant.context.clone(),
            decision: decision.clone(),
            evaluation_refs: expected,
            restrictions: recommendation.restrictions,
            authority: format!("policy:{}", policy.id),
            policy_id: policy.id.clone(),
            supersedes: vec![],
        };
        let saved = self.store.submit(
            &grant,
            &RecordSubmission {
                kind: RecordKind::Admission,
                provenance: record.provenance,
                body: serde_json::to_value(admission)?
                    .as_object()
                    .unwrap()
                    .clone(),
                id: None,
                expected_version: None,
            },
            true,
        )?;
        if decision == AdmissionDecision::Accepted {
            for cell in &policy.allowed_cells {
                let members: Vec<_> = evaluations
                    .iter()
                    .filter(|e| e.arm == "candidate" && e.descriptor.as_ref() == Some(cell))
                    .collect();
                if members.is_empty() {
                    continue;
                }
                // Every repetition of each member case must be assigned to the
                // same measured cell. A lucky descriptor cannot select a trial.
                let case_ids: std::collections::BTreeSet<_> =
                    members.iter().map(|e| &e.case_id).collect();
                if case_ids.iter().any(|case| {
                    members.iter().filter(|e| &e.case_id == *case).count()
                        != policy.repetitions as usize
                }) {
                    continue;
                }
                let quality = members
                    .iter()
                    .map(|e| metric(e, &policy.metric).unwrap())
                    .sum::<f64>()
                    / members.len() as f64;
                let evidence = json!({"admission_ref":saved.id,"evaluation_refs":recommendation.evaluation_refs,
                    "implementation_version":recommendation.implementation.version,"limitations":saved.body["restrictions"]});
                // An unavailable elite cannot prevent a new supported entry.
                let old: Option<String> = self.store.db.query_row("SELECT evidence FROM archive WHERE client=?1 AND project=?2 AND context=?3 AND policy_id=?4 AND cell=?5",params![grant.scope.client,grant.scope.project,grant.context,policy.id,cell],|r|r.get(0)).optional()?.flatten();
                let available = if let Some(body) = old {
                    let evidence: Value = serde_json::from_str(&body)?;
                    if let Some(reference) = evidence["admission_ref"].as_str() {
                        self.store.source_available(&grant, "record", reference)?
                    } else {
                        false
                    }
                } else {
                    false
                };
                self.store.db.execute("INSERT INTO archive(client,project,context,policy_id,cell,implementation_id,quality,evaluation_id,evidence) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(client,project,context,policy_id,cell) DO UPDATE SET implementation_id=excluded.implementation_id,quality=excluded.quality,evaluation_id=excluded.evaluation_id,evidence=excluded.evidence WHERE NOT ?10 OR excluded.quality>archive.quality",params![grant.scope.client,grant.scope.project,grant.context,policy.id,cell,recommendation.implementation.id,quality,recommendation.evaluation_refs[0],serde_json::to_string(&evidence)?,available])?;
            }
        }
        Ok(saved)
    }
}

fn study_inputs(body: &str) -> Result<Value> {
    let mut configuration: Value = serde_json::from_str(body)?;
    let fields = configuration
        .as_object_mut()
        .ok_or_else(|| Error::internal("stored study configuration is not an object"))?;
    fields.remove("workspace");
    fields.remove("workspace_cleanup_confirmed");
    Ok(configuration)
}

pub(crate) fn metric(evaluation: &Evaluation, name: &str) -> Option<f64> {
    if name == "verified_success" {
        return evaluation
            .passed
            .map(|passed| if passed { 1.0 } else { 0.0 });
    }
    evaluation
        .measurements
        .iter()
        .find(|m| m.name == name)
        .and_then(|m| m.value)
        .filter(|v| v.is_finite())
}
fn assess(
    policy: &AdmissionPolicy,
    evaluations: &[Evaluation],
    expected: usize,
) -> AdmissionDecision {
    if evaluations.len() != expected
        || evaluations.iter().any(|e| {
            e.execution_status
                .as_ref()
                .is_some_and(|status| *status != EvaluationExecutionStatus::Completed)
                || e.passed.is_none()
                || metric(e, &policy.metric).is_none()
                || !policy.required_checks.iter().all(|c| e.checks.contains(c))
        })
    {
        return AdmissionDecision::Inconclusive;
    }
    let candidates = evaluations
        .iter()
        .filter(|e| e.arm == "candidate")
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return AdmissionDecision::Inconclusive;
    }
    if policy.study_objective.is_some() {
        return crate::study_report::decision(policy, evaluations);
    }
    for candidate in candidates {
        let Some(baseline) = evaluations.iter().find(|e| {
            e.arm == "baseline"
                && e.case_id == candidate.case_id
                && e.repetition == candidate.repetition
        }) else {
            return AdmissionDecision::Inconclusive;
        };
        let quality = metric(candidate, &policy.metric).unwrap();
        if candidate.passed != Some(true)
            || quality < policy.min_quality
            || quality - metric(baseline, &policy.metric).unwrap() < policy.min_improvement
        {
            return AdmissionDecision::Rejected;
        }
    }
    AdmissionDecision::Accepted
}
