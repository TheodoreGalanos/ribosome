use crate::{
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
}

/// A host-owned evaluator executes measurements; an experimenter cannot supply
/// this object or alter its protected checks through the worker bridge.
pub trait Evaluator: Send {
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
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
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
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

#[derive(Default)]
pub struct Laboratory {
    policies: BTreeMap<String, AdmissionPolicy>,
    cases: BTreeMap<String, EvaluationCase>,
    evaluators: BTreeMap<String, Box<dyn Evaluator>>,
}

impl Laboratory {
    pub fn register_evaluator(&mut self, id: String, evaluator: Box<dyn Evaluator>) -> Result<()> {
        if self.evaluators.contains_key(&id) {
            return Err(Error::conflict("evaluator already registered"));
        }
        self.evaluators.insert(id, evaluator);
        Ok(())
    }
    pub fn register_case(&mut self, case: EvaluationCase) -> Result<()> {
        if self.cases.contains_key(&case.id) {
            return Err(Error::conflict("case already registered"));
        }
        self.cases.insert(case.id.clone(), case);
        Ok(())
    }
    pub fn register_policy(&mut self, policy: AdmissionPolicy) -> Result<()> {
        if self.policies.contains_key(&policy.id) {
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
        {
            return Err(Error::invalid("invalid evaluation policy"));
        }
        if !self.evaluators.contains_key(&policy.evaluator)
            || policy
                .case_ids
                .iter()
                .any(|id| !self.cases.contains_key(id))
        {
            return Err(Error::invalid(
                "policy references an unregistered evaluator or case",
            ));
        }
        self.policies.insert(policy.id.clone(), policy);
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
                .map(|id| self.cases[id].clone())
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
            if !seen.insert(&case.id) || self.cases.contains_key(&case.id) {
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
            .policies
            .get(&experiment.policy_id)
            .ok_or_else(|| Error::denied("no registered evaluation policy"))?;
        if policy.context != grant.context || policy.repetitions != experiment.repetitions {
            return Err(Error::denied(
                "experiment changes fixed context or repetitions",
            ));
        }
        let cases = self.laboratory.study_cases(policy, &experiment)?;
        let memory_start = self.experiment_memory(&grant, &experiment)?;
        let pinned = serde_json::to_string(
            &json!({"policy":policy,"cases":cases,"development_cases":experiment.development_cases,"memory_start":memory_start}),
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
            if previous != pinned {
                return Err(Error::conflict("frozen evaluator policy or cases changed"));
            }
            return result.map(|r|Ok(serde_json::from_str(&r)?)).unwrap_or_else(||Err(Error::conflict("experiment interrupted; owner must inspect partial evidence before further protected evaluation")));
        }
        let protected = cases.iter().any(|case| case.split == Split::Holdout);
        if protected {
            let prior: bool = self.store.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM experiments WHERE grant_id=?1 AND policy_id=?2)",
                params![grant.id, policy.id],
                |r| r.get(0),
            )?;
            if prior {
                return Err(Error::denied(
                    "protected selection already evaluated under this grant and policy",
                ));
            }
        }
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
        let total = variants.len() * cases.len() * policy.repetitions as usize;
        if total > policy.max_evaluations as usize {
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
        self.store.db.execute(
            "INSERT INTO experiments(id,grant_id,policy_id,policy) VALUES (?1,?2,?3,?4)",
            params![experiment_id, grant.id, policy.id, pinned],
        )?;
        let directory = tempfile::tempdir_in(&self.state_dir)?;
        let mut evaluation_refs = Vec::new();
        let mut evaluations = Vec::new();
        for repetition in 0..policy.repetitions {
            for variant in &variants {
                // Arm and repetition receive separate stores. A learning study
                // may retain within its arm across cases, never across arms.
                let arm_namespace = id();
                let arm_dir = directory.path().join(&arm_namespace);
                std::fs::create_dir(&arm_dir)?;
                for case in &cases {
                    self.store.require_active(run_id)?;
                    if self
                        .cancellation(run_id)
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err(Error::denied("experiment cancelled"));
                    }
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
                    let task = EvaluationTask {
                        experiment_id: experiment_id.into(),
                        implementation: implementations[&variant.arm].clone(),
                        case_id: case.id.clone(),
                        case_input: case.input.clone(),
                        arm: variant.arm.clone(),
                        repetition,
                        memory_namespace,
                        budget: experiment.budget.clone(),
                        memory_start: memory_start.clone(),
                    };
                    let observation = self.laboratory.evaluators[&policy.evaluator].evaluate(
                        &task,
                        &case_dir,
                        &self.cancellation(run_id),
                    );
                    let observation = match observation {
                        Ok(value) => value,
                        Err(error) => EvaluationObservation {
                            measurements: vec![],
                            checks: vec![],
                            output: format!("Evaluation unavailable: {}", error.message),
                            passed: None,
                            descriptor: None,
                        },
                    };
                    validate(
                        "EvaluationObservation",
                        &serde_json::to_value(&observation)?,
                    )?;
                    let evaluation = Evaluation {
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
                    let provenance=Provenance{origin:Origin::Observed,source_refs:vec![experiment_id.into()],scenario_family:case.family.clone(),split:case.split.clone(),limitations:vec!["Evaluator execution is observed; scenario origin follows the host study definition.".into()]};
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
                    evaluation_refs.push(saved.id);
                    evaluations.push(evaluation);
                }
            }
        }
        let decision = assess(policy, &evaluations, total);
        let result = ExperimentResult {
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
            &json!({"policy":policy,"cases":self.laboratory.study_cases(policy, &experiment)?,"development_cases":experiment.development_cases,"memory_start":self.experiment_memory(&grant, &experiment)?}),
        )?;
        if frozen != current {
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
            for (evaluation, reference) in evaluations.iter().zip(&recommendation.evaluation_refs) {
                if evaluation.arm != "candidate" {
                    continue;
                }
                if let (Some(cell), Some(quality)) =
                    (&evaluation.descriptor, metric(evaluation, &policy.metric))
                    && policy.allowed_cells.contains(cell)
                {
                    self.store.db.execute("INSERT INTO archive(client,project,context,policy_id,cell,implementation_id,quality,evaluation_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(client,project,context,policy_id,cell) DO UPDATE SET implementation_id=excluded.implementation_id,quality=excluded.quality,evaluation_id=excluded.evaluation_id WHERE excluded.quality>archive.quality",params![grant.scope.client,grant.scope.project,grant.context,policy.id,cell,recommendation.implementation.id,quality,reference])?;
                }
            }
        }
        Ok(saved)
    }
}

fn metric(evaluation: &Evaluation, name: &str) -> Option<f64> {
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
            e.passed.is_none()
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
