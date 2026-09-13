use crate::{
    contracts::*,
    experiments::{AdmissionPolicy, EvaluationCase, metric},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn controls(experiment: &Experiment) -> Vec<&'static str> {
    if experiment.study_objective == Some(ExperimentStudyObjective::SystemBenefit) {
        vec!["baseline", "retry", "critique", "care"]
    } else {
        vec!["baseline"]
    }
}

/// Repetitions are averaged within cases before paired case resampling.
/// The fixed resampling seed only makes the report reproducible; it says
/// nothing about provider determinism or generalization beyond these cases.
fn comparison(
    evaluations: &[Evaluation],
    treatment: &str,
    control: &str,
    metric_name: &str,
) -> Value {
    let mut pairs: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for e in evaluations.iter().filter(|e| e.arm == treatment) {
        if let Some(other) = evaluations
            .iter()
            .find(|c| c.arm == control && c.case_id == e.case_id && c.repetition == e.repetition)
            && let (Some(a), Some(b)) = (metric(e, metric_name), metric(other, metric_name))
        {
            pairs.entry(&e.case_id).or_default().push(a - b);
        }
    }
    let values: Vec<f64> = pairs
        .values()
        .map(|p| p.iter().sum::<f64>() / p.len() as f64)
        .collect();
    let mean = (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
    let interval = if values.len() < 2 {
        Value::Null
    } else {
        let mut seed = 0x7269626f736f6d65_u64;
        let mut samples = Vec::with_capacity(2000);
        for _ in 0..2000 {
            let mut sum = 0.0;
            for _ in &values {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                sum += values[seed as usize % values.len()];
            }
            samples.push(sum / values.len() as f64);
        }
        samples.sort_by(f64::total_cmp);
        json!({"lower":samples[49],"upper":samples[1949]})
    };
    json!({"treatment":treatment,"control":control,"paired_cases":values.len(),"mean_difference":mean,"interval_95":interval})
}

pub(crate) fn report(
    experiment: &Experiment,
    policy: &AdmissionPolicy,
    cases: &[EvaluationCase],
    evaluations: &[Evaluation],
    complete: bool,
    usage: &BudgetUsage,
) -> serde_json::Map<String, Value> {
    let arms: BTreeSet<_> = evaluations.iter().map(|e| e.arm.as_str()).collect();
    let comparisons: Vec<_> = arms
        .iter()
        .filter(|arm| **arm != "baseline")
        .map(|arm| comparison(evaluations, arm, "baseline", &policy.metric))
        .chain(
            controls(experiment)
                .into_iter()
                .filter(|c| *c != "baseline")
                .map(|c| comparison(evaluations, "candidate", c, &policy.metric)),
        )
        .collect();
    let aggregates: Vec<_> = arms.iter().map(|arm| {
        let selected: Vec<_> = evaluations.iter().filter(|e| e.arm == *arm).collect();
        let values: Vec<_> = selected.iter().filter_map(|e| metric(e,&policy.metric)).collect();
        let mut measurements: BTreeMap<&str,Vec<f64>> = BTreeMap::new();
        for evaluation in &selected {
            for m in &evaluation.measurements {
                if let Some(value) = m.value.filter(|v| v.is_finite()) { measurements.entry(&m.name).or_default().push(value); }
            }
        }
        let means: BTreeMap<_,_> = measurements.iter().map(|(name,values)| (*name,json!({"mean":values.iter().sum::<f64>()/values.len() as f64,"observations":values.len()}))).collect();
        json!({"mean_measurements":means,"arm":arm,"planned":selected.len(),"observed":values.len(),"verified_successes":selected.iter().filter(|e| e.passed == Some(true)).count(),
            "verified_success_rate":selected.iter().filter(|e| e.passed==Some(true)).count() as f64 / selected.len() as f64,"observed_mean_quality":(!values.is_empty()).then(|| values.iter().sum::<f64>()/values.len() as f64)})
    }).collect();
    json!({"objective":experiment.study_objective,"complete":complete,"metric":policy.metric,
        "cases":cases.len(),"families":cases.iter().map(|c| &c.family).collect::<BTreeSet<_>>().len(),"repetitions":policy.repetitions,
        "order":"rotated and reversed arm order by repetition; within-arm case order fixed",
        "uncertainty_method":"paired case percentile bootstrap, 2000 resamples, 95%; repetitions averaged within cases; fewer than two cases has no interval",
        "comparisons":comparisons,"arms":aggregates,"online_usage":usage,"learning_cost":experiment.learning_cost,
        "amortized_learning_cost_microusd":experiment.learning_cost.as_ref().filter(|c|c.complete).and_then(|c| c.cost_microusd.parse::<f64>().ok().map(|cost|cost/c.reuse_count as f64)),
        "limitations":["Intervals describe these recipient cases, not universal reliability or independent families.","Incomplete matrices and unknown usage do not support system benefit.","Learning cost is an explicit owner-supplied measurement; absent cost is unknown, not zero."]}).as_object().unwrap().clone()
}

pub(crate) fn decision(policy: &AdmissionPolicy, evaluations: &[Evaluation]) -> AdmissionDecision {
    let candidate: Vec<_> = evaluations
        .iter()
        .filter(|e| e.arm == "candidate")
        .collect();
    let quality = candidate
        .iter()
        .filter_map(|e| metric(e, &policy.metric))
        .sum::<f64>()
        / candidate.len() as f64;
    if quality < policy.min_quality {
        return AdmissionDecision::Rejected;
    }
    if policy.study_objective == Some(ExperimentStudyObjective::Function) {
        return if candidate.iter().all(|e| e.passed == Some(true)) {
            AdmissionDecision::Accepted
        } else {
            AdmissionDecision::Rejected
        };
    }
    if policy
        .learning_cost
        .as_ref()
        .is_some_and(|cost| !cost.complete)
        || evaluations.iter().any(|e| e.usage_complete != Some(true))
    {
        return AdmissionDecision::Inconclusive;
    }
    let mut uncertain = false;
    for control in ["baseline", "retry", "critique", "care"] {
        let paired = comparison(evaluations, "candidate", control, &policy.metric);
        let Some(lower) = paired["interval_95"]["lower"].as_f64() else {
            return AdmissionDecision::Inconclusive;
        };
        let upper = paired["interval_95"]["upper"].as_f64().unwrap();
        if upper < policy.min_improvement {
            return AdmissionDecision::Rejected;
        }
        uncertain |= lower < policy.min_improvement;
    }
    if uncertain {
        AdmissionDecision::Inconclusive
    } else {
        AdmissionDecision::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::decode;
    fn evaluations() -> Vec<Evaluation> {
        let mut result = vec![];
        for case in ["one", "two", "three"] {
            for repetition in 0..2 {
                for arm in ["baseline", "retry", "critique", "care", "candidate"] {
                    result.push(decode("Evaluation",json!({"experiment_ref":"study","implementation":{"id":arm,"version":"1"},"case_id":case,"arm":arm,"repetition":repetition,"evaluator":"judge","evaluator_version":"1","policy_id":"policy","passed":arm=="candidate"||repetition==0,"measurements":[{"name":"quality","value":if arm=="candidate" {1.0} else {0.5},"unit":"fraction"}],"checks":["checked"],"memory_namespace":"isolated","output":"fixture","artifact_refs":[],"execution_status":"completed","usage_complete":true})).unwrap());
                }
            }
        }
        result
    }
    fn policy() -> AdmissionPolicy {
        serde_json::from_value(json!({"id":"policy","context":"task","evaluator":"judge","evaluator_version":"1","case_ids":["one","two","three"],"required_checks":["checked"],"metric":"verified_success","min_quality":0.5,"min_improvement":0.1,"repetitions":2,"allowed_cells":[],"retain_learning_memory":false,"max_evaluations":45,"study_objective":"system_benefit"})).unwrap()
    }
    #[test]
    fn repeated_runs_are_paired_at_case_level() {
        let result = comparison(&evaluations(), "candidate", "baseline", "quality");
        assert_eq!(result["paired_cases"], 3);
        assert_eq!(result["mean_difference"], 0.5);
        assert_eq!(result["interval_95"]["lower"], 0.5);
        assert_eq!(
            decision(&policy(), &evaluations()),
            AdmissionDecision::Accepted
        );
    }
    #[test]
    fn no_gain_missing_controls_and_unknown_usage_do_not_support_benefit() {
        let mut runs = evaluations();
        for e in &mut runs {
            e.measurements[0].value = Some(0.5);
            e.passed = Some(true);
        }
        assert_eq!(decision(&policy(), &runs), AdmissionDecision::Rejected);
        let runs: Vec<_> = evaluations()
            .into_iter()
            .filter(|e| e.arm != "retry")
            .collect();
        assert_eq!(decision(&policy(), &runs), AdmissionDecision::Inconclusive);
        let mut runs = evaluations();
        runs[0].usage_complete = Some(false);
        assert_eq!(decision(&policy(), &runs), AdmissionDecision::Inconclusive);
    }
    #[test]
    fn varying_case_outcomes_can_leave_a_positive_mean_inconclusive() {
        let mut runs = evaluations();
        for e in &mut runs {
            e.passed = Some(if e.arm == "candidate" {
                e.case_id != "one"
            } else {
                e.repetition == 0
            });
            e.measurements[0].value = Some(if e.arm == "candidate" && e.case_id != "one" {
                1.0
            } else if e.arm == "candidate" {
                0.0
            } else {
                0.5
            });
        }
        assert_eq!(decision(&policy(), &runs), AdmissionDecision::Inconclusive);
    }
}
