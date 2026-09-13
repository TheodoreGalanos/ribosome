use ribosome_core::{
    contracts::*,
    effects::Runtime,
    error::Result,
    experiments::{AdmissionPolicy, EvaluationCase, Evaluator},
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

struct TestEvaluator {
    unknown: bool,
}
impl Evaluator for TestEvaluator {
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
        _account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        // This fixture checks Rust evidence attribution and arm isolation. It
        // does not establish semantic quality of a generated implementation.
        let marker = workspace.join("memory.txt");
        assert!(!marker.exists(), "case/arm memory leaked");
        std::fs::write(marker, &task.arm).unwrap();
        Ok(EvaluationObservation {
            passed: if self.unknown {
                None
            } else {
                Some(task.arm == "candidate")
            },
            measurements: if self.unknown {
                vec![]
            } else {
                vec![Measurement {
                    name: "quality".into(),
                    value: Some(if task.arm == "candidate" { 1.0 } else { 0.0 }),
                    unit: "fraction".into(),
                }]
            },
            checks: vec!["independent-check".into()],
            output: "protected evaluator observation".into(),
            descriptor: Some("units".into()),
        })
    }
}

fn fixture(unknown: bool) -> (tempfile::TempDir, Runtime, Grant) {
    fixture_with_evaluator(Box::new(TestEvaluator { unknown }), 20)
}
fn fixture_with_evaluator(
    evaluator: Box<dyn Evaluator>,
    max_calls: u32,
) -> (tempfile::TempDir, Runtime, Grant) {
    let d = match std::env::var_os("RIBOSOME_EVALUATOR_CRASH_ROOT") {
        Some(root) => tempfile::tempdir_in(root).unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    let store = Store::open(d.path().join("store.db")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"lab-grant","scope":{"client":"test","project":"lab"},"mode":"sandbox","paths":[],"tools":[],"profiles":["experimenter","caretaker","curator"],"budget":{"max_calls":max_calls,"max_tokens":"100000","max_cost_microusd":"1000000","max_actions":10,"max_work_items":4,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"measurements","visible_splits":["development"],"allow_export":true})).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(d.path(), BTreeMap::new()).unwrap();
    // Real macOS workspaces can exceed the record ID bound before the
    // evaluator's arm/case subdirectories are appended.
    let mut r = Runtime::new(
        store,
        Box::new(host),
        d.path().join("nested-evaluation-state-".repeat(8)),
    )
    .unwrap();
    r.store.begin_run("lab-run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"lab-run","profile":"experimenter","operator":"experiment@1","prompt":"Evaluate normalization","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    r.laboratory
        .register_evaluator("units".into(), evaluator)
        .unwrap();
    for id in ["case-1", "case-2"] {
        r.laboratory
            .register_case(EvaluationCase {
                id: id.into(),
                family: format!("family-{id}"),
                split: Split::Holdout,
                input: serde_json::Map::new(),
            })
            .unwrap();
    }
    r.laboratory
        .register_policy(AdmissionPolicy {
            id: "units-policy".into(),
            context: grant.context.clone(),
            evaluator: "units".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["case-1".into(), "case-2".into()],
            required_checks: vec!["independent-check".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.5,
            repetitions: 2,
            allowed_cells: vec!["units".into()],
            retain_learning_memory: false,
            max_evaluations: 8,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
    (d, r, grant)
}
fn submit(r: &Runtime, g: &Grant, kind: &str, body: Value) -> RecordEnvelope {
    r.store.submit(g,&decode("RecordSubmission",json!({"kind":kind,"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"fixture","split":"development","limitations":["test fixture"]},"body":body})).unwrap(),false).unwrap()
}
fn material(r: &Runtime, g: &Grant, name: &str) -> RecordEnvelope {
    submit(
        r,
        g,
        "implementation",
        json!({"name":name,"version":"1","motifs":[],"format":"instructions","material":"Normalize units before summing","parameters":{},"required_capabilities":[],"state_assumptions":[],"possible_effects":[],"failure_behavior":"abstain on unsupported units","evaluation_refs":[]}),
    )
}
fn experiment(r: &Runtime, g: &Grant) -> (RecordEnvelope, RecordEnvelope) {
    let baseline = material(r, g, "baseline");
    let candidate = material(r, g, "candidate");
    let e = submit(
        r,
        g,
        "experiment",
        json!({"name":"normalization","template":"transfer","candidate":{"id":candidate.id,"version":"1"},"baseline":{"id":baseline.id,"version":"1"},"hypothesis":"normalization improves consistency","case_ids":["case-1","case-2"],"scenario_families":["family-case-1","family-case-2"],"feedback":"aggregate","model_version":"fixture","tool_versions":["units@1"],"memory_start_refs":[],"repetitions":2,"budget":g.budget,"metrics":["quality"],"policy_id":"units-policy","selection_frozen":true,"variants":[]}),
    );
    (e, candidate)
}

#[test]
fn matched_evaluation_controls_admission_and_usable_inventory() {
    let (_d, r, g) = fixture(false);
    let (e, candidate) = experiment(&r, &g);
    let result = r.run_experiment("lab-run", &e.id).unwrap();
    assert_eq!(result.decision, AdmissionDecision::Accepted);
    assert_eq!(result.evaluation_refs.len(), 8);
    assert_eq!(result, r.run_experiment("lab-run", &e.id).unwrap());
    for id in &result.evaluation_refs {
        assert!(r.store.record(&g, id).is_err(), "holdout leaked");
        let mut evaluator_grant = g.clone();
        evaluator_grant.visible_splits.push(Split::Holdout);
        let evaluation = r.store.record(&evaluator_grant, id).unwrap();
        let namespace = evaluation.body["memory_namespace"].as_str().unwrap();
        assert_eq!(namespace.len(), 36, "namespace must be an opaque ID");
        assert!(!namespace.contains('/'));
    }
    let query: SearchRequest = decode(
        "SearchRequest",
        json!({"query":"","inventory":"usable","limit":20,"offset":0}),
    )
    .unwrap();
    assert!(r.store.search(&g, &query).unwrap().records.is_empty());
    let recommendation = submit(
        &r,
        &g,
        "recommendation",
        json!({"implementation":{"id":candidate.id,"version":"1"},"context":g.context,"decision":"accepted","evaluation_refs":result.evaluation_refs,"rationale":"independent matched evaluation","restrictions":[]}),
    );
    let admitted = r.request_admission("lab-run", &recommendation.id).unwrap();
    assert_eq!(admitted.body["decision"], "accepted");
    assert_eq!(r.store.search(&g, &query).unwrap().records.len(), 1);
    let mut other = g.clone();
    other.context = "unrelated".into();
    assert!(r.store.search(&other, &query).unwrap().records.is_empty());
    assert!(
        r.export_training(
            "lab-run",
            &ExportRequest {
                record_ids: vec![recommendation.id],
                product: Origin::Synthetic
            }
        )
        .is_err()
    );
}

#[test]
fn missing_measurements_are_inconclusive_and_cannot_be_invented_by_the_agent() {
    let (_d, r, g) = fixture(true);
    let (e, candidate) = experiment(&r, &g);
    let result = r.run_experiment("lab-run", &e.id).unwrap();
    assert_eq!(result.decision, AdmissionDecision::Inconclusive);
    let recommendation = submit(
        &r,
        &g,
        "recommendation",
        json!({"implementation":{"id":candidate.id,"version":"1"},"context":g.context,"decision":"accepted","evaluation_refs":result.evaluation_refs,"rationale":"agent wishes this worked","restrictions":[]}),
    );
    assert_eq!(
        r.request_admission("lab-run", &recommendation.id)
            .unwrap()
            .body["decision"],
        "inconclusive"
    );
    let forged:RecordSubmission=decode("RecordSubmission",json!({"kind":"evaluation","provenance":{"origin":"observed","source_refs":[],"scenario_family":"fake","split":"development","limitations":[]},"body":{}})).unwrap();
    assert!(r.store.submit(&g, &forged, false).is_err());
}

#[test]
fn partial_evidence_and_repeated_holdout_selection_are_rejected() {
    let (_d, r, g) = fixture(false);
    let (e, candidate) = experiment(&r, &g);
    let result = r.run_experiment("lab-run", &e.id).unwrap();
    let recommendation = submit(
        &r,
        &g,
        "recommendation",
        json!({"implementation":{"id":candidate.id,"version":"1"},"context":g.context,"decision":"accepted","evaluation_refs":[result.evaluation_refs[0]],"rationale":"selective evidence","restrictions":[]}),
    );
    assert!(r.request_admission("lab-run", &recommendation.id).is_err());
    let (second, _) = experiment(&r, &g);
    assert!(r.run_experiment("lab-run", &second.id).is_err());
}

#[test]
fn every_experiment_budget_dimension_is_bounded_by_the_grant() {
    for field in [
        "max_calls",
        "max_tokens",
        "max_cost_microusd",
        "max_actions",
        "max_work_items",
        "max_depth",
        "deadline_ms",
    ] {
        let (_d, r, g) = fixture(false);
        let (original, _) = experiment(&r, &g);
        let mut body = Value::Object(original.body);
        let limit = &body["budget"][field];
        body["budget"][field] = if let Some(value) = limit.as_str() {
            json!((value.parse::<u64>().unwrap() + 1).to_string())
        } else {
            json!(limit.as_u64().unwrap() + 1)
        };
        let exceeded = submit(&r, &g, "experiment", body);
        assert_eq!(
            r.run_experiment("lab-run", &exceeded.id)
                .unwrap_err()
                .message,
            "experiment budget exceeds grant",
            "unbounded {field}"
        );
    }
}

fn development_policy(r: &mut Runtime, checks: Vec<String>) {
    r.laboratory
        .register_policy(AdmissionPolicy {
            id: "development-policy".into(),
            context: "measurements".into(),
            evaluator: "units".into(),
            evaluator_version: "1".into(),
            case_ids: vec![],
            required_checks: checks,
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.5,
            repetitions: 2,
            allowed_cells: vec!["units".into()],
            retain_learning_memory: false,
            max_evaluations: 8,
            allow_generated_development_cases: true,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
}

fn development_body(r: &Runtime, g: &Grant) -> Value {
    let (original, _) = experiment(r, g);
    let mut body = Value::Object(original.body);
    body["policy_id"] = json!("development-policy");
    body["case_ids"] = json!(["generated-1", "generated-2"]);
    body["scenario_families"] = json!(["unit-conversion"]);
    body["development_cases"] = json!([1, 2].map(|number| json!({
        "id":format!("generated-{number}"), "source_mechanism":"Mixed units must be normalized before aggregation",
        "provenance":{"origin":"synthetic", "split":"development", "source_refs":[body["candidate"]["id"]], "scenario_family":"unit-conversion", "limitations":["Derived cases are not independent transfer evidence"]},
        "input":{"measurements":[{"value":number,"unit":"m"},{"value":200,"unit":"cm"}], "expected_m":number+2},
        "candidate_checks":["independent-check"]
    })));
    body
}

#[test]
fn generated_development_cases_keep_lineage_and_cannot_admit_their_candidate() {
    let (_d, mut r, g) = fixture(false);
    development_policy(&mut r, vec!["independent-check".into()]);
    let body = development_body(&r, &g);
    let candidate = body["candidate"].clone();
    let e = submit(&r, &g, "experiment", body.clone());
    let result = r.run_experiment("lab-run", &e.id).unwrap();
    assert_eq!(result.decision, AdmissionDecision::Accepted);
    assert_eq!(result.evaluation_refs.len(), 8);
    assert_eq!(result, r.run_experiment("lab-run", &e.id).unwrap());
    for id in &result.evaluation_refs {
        let observation = r.store.record(&g, id).unwrap();
        assert_eq!(observation.provenance.scenario_family, "unit-conversion");
        assert_eq!(observation.provenance.split, Split::Development);
        assert_eq!(observation.provenance.source_refs, vec![e.id.clone()]);
        assert!(
            observation.body["case_id"]
                .as_str()
                .unwrap()
                .starts_with("generated-")
        );
    }
    assert_eq!(
        r.store.record(&g, &e.id).unwrap().body["development_cases"],
        body["development_cases"]
    );
    let recommendation = submit(
        &r,
        &g,
        "recommendation",
        json!({"implementation":candidate,"context":g.context,"decision":"accepted","evaluation_refs":result.evaluation_refs,"rationale":"Development passed", "restrictions":[]}),
    );
    assert!(
        r.request_admission("lab-run", &recommendation.id)
            .unwrap_err()
            .message
            .contains("cannot establish admission")
    );
}

#[test]
fn generated_cases_cannot_replace_protected_cases_or_owner_checks() {
    let (_d, mut r, g) = fixture(false);
    development_policy(&mut r, vec!["owner-check-not-replaced-by-candidate".into()]);
    let body = development_body(&r, &g);
    let e = submit(&r, &g, "experiment", body.clone());
    assert_eq!(
        r.run_experiment("lab-run", &e.id).unwrap().decision,
        AdmissionDecision::Inconclusive
    );
    let mut protected = body.clone();
    protected["policy_id"] = json!("units-policy");
    let e = submit(&r, &g, "experiment", protected);
    assert!(
        r.run_experiment("lab-run", &e.id)
            .unwrap_err()
            .message
            .contains("fixed evaluation cases")
    );
    for (field, value) in [
        ("origin", json!("observed")),
        ("split", json!("holdout")),
        ("source_refs", json!([])),
    ] {
        let mut invalid = body.clone();
        invalid["development_cases"][0]["provenance"][field] = value;
        let e = submit(&r, &g, "experiment", invalid);
        assert!(
            r.run_experiment("lab-run", &e.id)
                .unwrap_err()
                .message
                .contains("synthetic development provenance")
        );
    }
    for invalid_id in ["case-1", "generated-2"] {
        let mut invalid = body.clone();
        invalid["case_ids"][0] = json!(invalid_id);
        invalid["development_cases"][0]["id"] = json!(invalid_id);
        let e = submit(&r, &g, "experiment", invalid);
        assert!(
            r.run_experiment("lab-run", &e.id)
                .unwrap_err()
                .message
                .contains("distinct from registered cases")
        );
    }
}

#[test]
fn development_source_references_must_be_accessible() {
    let (_d, r, g) = fixture(false);
    let mut body = development_body(&r, &g);
    body["development_cases"][0]["provenance"]["source_refs"] = json!(["invented-source"]);
    let request = decode("RecordSubmission", json!({"kind":"experiment", "provenance":{"origin":"synthetic", "source_refs":[], "scenario_family":"unit-conversion", "split":"development", "limitations":[]}, "body":body})).unwrap();
    assert!(
        r.store
            .submit(&g, &request, false)
            .unwrap_err()
            .message
            .contains("inaccessible")
    );
}

struct MemoryEvaluator;
impl Evaluator for MemoryEvaluator {
    fn evaluate(
        &self,
        task: &EvaluationTask,
        workspace: &Path,
        _account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        assert_eq!(task.memory_start.len(), 1);
        assert_eq!(
            task.memory_start[0].body["content"],
            "Normalize before summing"
        );
        let state = workspace.join("learned.txt");
        let seen = state.exists();
        if seen {
            assert_eq!(std::fs::read_to_string(&state).unwrap(), task.arm);
        }
        std::fs::write(state, &task.arm).unwrap();
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(1.0),
                unit: "fraction".into(),
            }],
            checks: vec!["memory-isolation".into()],
            output: format!("previous experience: {seen}"),
            descriptor: None,
        })
    }
}

#[test]
fn initial_memory_is_frozen_and_learning_persists_only_within_an_arm_and_repetition() {
    let (_d, mut r, g) = fixture(false);
    r.laboratory
        .register_evaluator("memory".into(), Box::new(MemoryEvaluator))
        .unwrap();
    r.laboratory
        .register_policy(AdmissionPolicy {
            id: "learning".into(),
            context: g.context.clone(),
            evaluator: "memory".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["case-1".into(), "case-2".into()],
            required_checks: vec!["memory-isolation".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.0,
            repetitions: 2,
            allowed_cells: vec![],
            retain_learning_memory: true,
            max_evaluations: 8,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
    let memory = submit(
        &r,
        &g,
        "memory",
        json!({"kind":"procedural","content":"Normalize before summing","applicability":"lengths","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}),
    );
    let (base, _) = experiment(&r, &g);
    let mut body = Value::Object(base.body);
    body["policy_id"] = json!("learning");
    body["memory_start_refs"] = json!([memory.id]);
    let e = submit(&r, &g, "experiment", body);
    let result = r.run_experiment("lab-run", &e.id).unwrap();
    let mut evaluator_grant = g.clone();
    evaluator_grant.visible_splits.push(Split::Holdout);
    for (index, id) in result.evaluation_refs.iter().enumerate() {
        let observed = r.store.record(&evaluator_grant, id).unwrap();
        assert_eq!(
            observed.body["output"],
            format!("previous experience: {}", index % 2 == 1)
        );
    }
    let mut changed: RecordSubmission = decode("RecordSubmission", json!({"kind":"memory","id":memory.id,"expected_version":memory.version,"provenance":memory.provenance,"body":memory.body})).unwrap();
    changed
        .body
        .insert("content".into(), json!("Changed after selection"));
    r.store.submit(&g, &changed, false).unwrap();
    assert!(
        r.run_experiment("lab-run", &e.id)
            .unwrap_err()
            .message
            .contains("frozen evaluator policy or cases changed")
    );
}

#[test]
fn observe_mode_cannot_dispatch_an_evaluator() {
    let (_d, r, mut g) = fixture(false);
    g.id = "observe-grant".into();
    g.mode = Mode::Observe;
    r.store.register_grant(&g).unwrap();
    r.store.begin_run("observe-run", &g.id, &decode("AgentRunRequest", json!({"run_id":"observe-run","profile":"experimenter","operator":"experiment@1","prompt":"Propose a study","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    let (e, _) = experiment(&r, &g);
    assert!(
        r.run_experiment("observe-run", &e.id)
            .unwrap_err()
            .message
            .contains("sandbox or apply")
    );
}

#[test]
fn missing_initial_memory_identifies_the_invalid_field() {
    let (d, r, g) = fixture(false);
    let (e, _) = experiment(&r, &g);
    // Simulate an imported proposal whose starting memory is unavailable.
    let mut stored = e.clone();
    stored
        .body
        .insert("memory_start_refs".into(), json!(["unavailable-memory"]));
    let db = rusqlite::Connection::open(d.path().join("store.db")).unwrap();
    db.execute(
        "UPDATE records SET body=?1 WHERE id=?2",
        rusqlite::params![serde_json::to_string(&stored).unwrap(), e.id],
    )
    .unwrap();
    let error = r.run_experiment("lab-run", &e.id).unwrap_err();
    assert!(
        error.message.contains("memory_start_refs"),
        "{}",
        error.message
    );
    assert!(error.message.contains("unavailable-memory"));
    assert_eq!(
        db.query_row("SELECT count(*) FROM experiments", [], |row| row
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn agent_experiment_metadata_must_match_its_configured_model() {
    let (_d, r, g) = fixture(false);
    let (e, _) = experiment(&r, &g);
    let mut submission = json!({"kind":"experiment","provenance":e.provenance,"body":e.body});
    let error = r
        .tool_call("lab-run", "record.submit", submission.clone())
        .unwrap_err();
    assert!(error.message.contains("model_version"));
    submission["body"]["model_version"] = json!("test-model");
    let saved = r.tool_call("lab-run", "record.submit", submission).unwrap();
    assert_eq!(saved["body"]["model_version"], "test-model");
}

struct MeteredEvaluator(std::sync::Arc<std::sync::atomic::AtomicU32>);
impl Evaluator for MeteredEvaluator {
    fn provider_usage_is_metered(&self) -> bool {
        true
    }
    fn evaluate(
        &self,
        task: &EvaluationTask,
        _: &Path,
        account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        let permit = account.permit(&decode("PermitRequest", json!({"call_id":format!("{}:{}:{}",task.arm,task.repetition,task.case_id),"input_tokens_bound":"10","max_output_tokens":10,"cost_microusd_bound":"20"})).unwrap())?;
        account.dispatch(&permit)?;
        // Counts dispatches at the trusted adapter boundary, without a paid provider.
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        account.usage(&Usage {
            permit_id: permit.id,
            input_tokens: "10".into(),
            output_tokens: "2".into(),
            cost_microusd: "12".into(),
            complete: true,
        })?;
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(1.0),
                unit: "fraction".into(),
            }],
            checks: vec!["independent-check".into()],
            output: "Metered infrastructure fixture".into(),
            descriptor: None,
        })
    }
}

#[test]
fn evaluator_cases_share_five_root_calls_and_exhaustion_keeps_the_complete_matrix() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let (_directory, runtime, grant) =
        fixture_with_evaluator(Box::new(MeteredEvaluator(calls.clone())), 5);
    let (experiment, _) = experiment(&runtime, &grant);
    let result = runtime.run_experiment("lab-run", &experiment.id).unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 5);
    assert_eq!(result.decision, AdmissionDecision::Inconclusive);
    assert_eq!(result.complete, Some(false));
    assert_eq!(result.planned_evaluations, Some(8));
    assert_eq!(result.evaluation_refs.len(), 8);
    assert_eq!(result.usage_complete, Some(true));
    let mut evaluator_grant = grant.clone();
    evaluator_grant.visible_splits.push(Split::Holdout);
    let statuses: Vec<_> = result
        .evaluation_refs
        .iter()
        .map(|id| {
            runtime.store.record(&evaluator_grant, id).unwrap().body["execution_status"].clone()
        })
        .collect();
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == "completed")
            .count(),
        5
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == "exhausted")
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == "not_started")
            .count(),
        2
    );
    let root = runtime.store.root_budget_status(&grant).unwrap();
    let study = runtime
        .store
        .budget_status(&grant, result.allocation_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(root.usage.model_calls, 5);
    assert_eq!(root.usage.settled_tokens, "60");
    assert_eq!(root.usage, study.usage);
    assert_eq!(
        result,
        runtime.run_experiment("lab-run", &experiment.id).unwrap()
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 5);
}

#[test]
fn metered_source_and_maintenance_leave_only_the_shared_remainder_for_evaluation() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let (_directory, runtime, grant) =
        fixture_with_evaluator(Box::new(MeteredEvaluator(calls.clone())), 5);
    let root = runtime.store.root_budget_status(&grant).unwrap().allocation;
    for (participant, profile, operator) in [
        ("source", "caretaker", "proofreading@1"),
        ("maintenance", "caretaker", "proofreading@1"),
        ("curation", "curator", "discovery@1"),
    ] {
        let allocation = runtime
            .store
            .allocate(
                &grant,
                &BudgetAllocationRequest {
                    id: participant.into(),
                    parent_id: root.id.clone(),
                    cause_id: "measured-treatment".into(),
                    purpose: participant.into(),
                    budget: grant.budget.clone(),
                },
            )
            .unwrap();
        let request = decode("AgentRunRequest", json!({"run_id":participant,"profile":profile,"operator":operator,"prompt":"Meter this participating adapter","provider":"openai","model":"test-model","parent_allocation_id":allocation.id})).unwrap();
        runtime
            .store
            .begin_run(participant, &grant.id, &request)
            .unwrap();
        // A host-controlled source transport uses the same public accounting
        // path as maintenance. Merely attaching an observer does not meter it.
        let permit = runtime.store.permit(participant, &decode("PermitRequest", json!({"call_id":"first","input_tokens_bound":"10","max_output_tokens":10,"cost_microusd_bound":"20"})).unwrap()).unwrap();
        runtime
            .store
            .dispatch_permit(participant, &permit.id)
            .unwrap();
        runtime
            .store
            .usage(
                participant,
                &Usage {
                    permit_id: permit.id,
                    input_tokens: "10".into(),
                    output_tokens: "2".into(),
                    cost_microusd: "12".into(),
                    complete: true,
                },
            )
            .unwrap();
        assert_eq!(
            runtime
                .store
                .budget_status(&grant, &allocation.id)
                .unwrap()
                .usage
                .model_calls,
            1
        );
    }
    let (experiment, _) = experiment(&runtime, &grant);
    let result = runtime.run_experiment("lab-run", &experiment.id).unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(result.complete, Some(false));
    assert_eq!(result.decision, AdmissionDecision::Inconclusive);
    assert_eq!(result.evaluation_refs.len(), 8);
    let study = runtime
        .store
        .budget_status(&grant, result.allocation_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(study.usage.model_calls, 2);
    let root = runtime.store.root_budget_status(&grant).unwrap();
    assert_eq!(root.usage.model_calls, 5);
    assert_eq!(root.usage.settled_tokens, "60");
    assert_eq!(root.remaining.max_calls, 0);
}

#[test]
fn invalid_child_study_budget_does_not_consume_a_protected_evaluation() {
    let (_directory, runtime, grant) = fixture(false);
    let root = runtime.store.root_budget_status(&grant).unwrap().allocation;
    let mut budget = grant.budget.clone();
    budget.max_calls = 1;
    let allocation = runtime
        .store
        .allocate(
            &grant,
            &BudgetAllocationRequest {
                id: "limited".into(),
                parent_id: root.id,
                cause_id: "host".into(),
                purpose: "experimenter".into(),
                budget,
            },
        )
        .unwrap();
    let request: AgentRunRequest=decode("AgentRunRequest",json!({"run_id":"limited-run","profile":"experimenter","operator":"experiment@1","prompt":"Evaluate","provider":"openai","model":"test-model","parent_allocation_id":allocation.id})).unwrap();
    runtime
        .store
        .begin_run("limited-run", &grant.id, &request)
        .unwrap();
    let (experiment, _) = experiment(&runtime, &grant);
    assert!(
        runtime
            .run_experiment("limited-run", &experiment.id)
            .unwrap_err()
            .message
            .contains("child allocation exceeds")
    );
    assert_eq!(
        runtime
            .run_experiment("lab-run", &experiment.id)
            .unwrap()
            .decision,
        AdmissionDecision::Accepted
    );
}

struct FailingEvaluator {
    cancel_during_evaluation: bool,
    calls: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

struct InvalidObservationEvaluator;
impl Evaluator for InvalidObservationEvaluator {
    fn evaluate(
        &self,
        _: &EvaluationTask,
        _: &Path,
        _: &ribosome_core::experiments::EvaluationAccount<'_>,
        _: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(1.0),
                unit: String::new(),
            }],
            checks: vec!["independent-check".into()],
            output: "Malformed adapter observation".into(),
            descriptor: None,
        })
    }
}

struct CancelledAccountEvaluator;
impl Evaluator for CancelledAccountEvaluator {
    fn provider_usage_is_metered(&self) -> bool {
        true
    }
    fn evaluate(
        &self,
        _: &EvaluationTask,
        _: &Path,
        account: &ribosome_core::experiments::EvaluationAccount<'_>,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        let request = |id| {
            decode("PermitRequest", json!({"call_id":id,"input_tokens_bound":"10","max_output_tokens":10,"cost_microusd_bound":"20"})).unwrap()
        };
        let dispatched = account.permit(&request("dispatched")).unwrap();
        account.dispatch(&dispatched).unwrap();
        let reserved = account.permit(&request("reserved")).unwrap();
        cancellation.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            account.permit(&request("after-cancellation")).is_err(),
            "cancelled evaluator reserved another call"
        );
        assert!(
            account.dispatch(&reserved).is_err(),
            "cancelled evaluator dispatched its queued call"
        );
        account
            .usage(&Usage {
                permit_id: dispatched.id,
                input_tokens: "10".into(),
                output_tokens: "2".into(),
                cost_microusd: "12".into(),
                complete: true,
            })
            .unwrap();
        account.release(&reserved).unwrap();
        Err(ribosome_core::error::Error::denied("experiment cancelled"))
    }
}

#[test]
fn cancelled_evaluator_cannot_reserve_or_dispatch_but_can_settle_and_release() {
    let (_directory, runtime, grant) =
        fixture_with_evaluator(Box::new(CancelledAccountEvaluator), 20);
    let (experiment, _) = experiment(&runtime, &grant);
    let result = runtime.run_experiment("lab-run", &experiment.id).unwrap();
    assert_eq!(result.complete, Some(false));
    assert_eq!(result.usage_complete, Some(true));
    assert_eq!(result.evaluation_refs.len(), 8);
    let root = runtime.store.root_budget_status(&grant).unwrap();
    assert_eq!(root.usage.model_calls, 1);
    assert_eq!(root.usage.settled_tokens, "12");
    assert_eq!(root.usage.reserved_tokens, "0");
    assert_eq!(root.usage.unknown_calls, 0);
    assert_eq!(root.usage.undispatched_calls, 0);
    assert_eq!(
        runtime
            .store
            .allocation(&grant, result.allocation_id.as_ref().unwrap())
            .unwrap()
            .disposition,
        Some(Disposition::Cancelled)
    );
}

#[test]
fn malformed_evaluator_observations_remain_failed_cases_in_the_matrix() {
    let (_directory, runtime, grant) =
        fixture_with_evaluator(Box::new(InvalidObservationEvaluator), 20);
    let (experiment, _) = experiment(&runtime, &grant);
    let result = runtime.run_experiment("lab-run", &experiment.id).unwrap();
    assert_eq!(result.complete, Some(false));
    assert_eq!(result.decision, AdmissionDecision::Inconclusive);
    assert_eq!(result.evaluation_refs.len(), 8);
    let mut evaluator_grant = grant.clone();
    evaluator_grant.visible_splits.push(Split::Holdout);
    for id in &result.evaluation_refs {
        let record = runtime.store.record(&evaluator_grant, id).unwrap();
        assert_eq!(record.body["execution_status"], "failed");
        assert!(!record.body.contains_key("passed"));
        assert_eq!(record.body["measurements"], json!([]));
        assert!(record.body["output"].as_str().unwrap().contains("unit"));
    }
}
impl Evaluator for FailingEvaluator {
    fn evaluate(
        &self,
        _: &EvaluationTask,
        _: &Path,
        _: &ribosome_core::experiments::EvaluationAccount<'_>,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.cancel_during_evaluation {
            cancellation.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Err(ribosome_core::error::Error::internal("evaluator stopped"))
    }
}

#[test]
fn failed_and_cancelled_evaluations_retain_their_actual_disposition() {
    for mode in ["failed", "cancel_before", "cancel_during"] {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let (_directory, runtime, grant) = fixture_with_evaluator(
            Box::new(FailingEvaluator {
                cancel_during_evaluation: mode == "cancel_during",
                calls: calls.clone(),
            }),
            20,
        );
        let (experiment, _) = experiment(&runtime, &grant);
        if mode == "cancel_before" {
            runtime
                .cancellation("lab-run")
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let result = runtime.run_experiment("lab-run", &experiment.id).unwrap();
        assert_eq!(result.complete, Some(false));
        assert_eq!(result.decision, AdmissionDecision::Inconclusive);
        assert_eq!(result.evaluation_refs.len(), 8);
        let expected = if mode == "failed" {
            Disposition::Failed
        } else {
            Disposition::Cancelled
        };
        assert_eq!(
            runtime
                .store
                .allocation(&grant, result.allocation_id.as_ref().unwrap())
                .unwrap()
                .disposition,
            Some(expected.clone()),
            "{mode}"
        );
        let mut evaluator_grant = grant.clone();
        evaluator_grant.visible_splits.push(Split::Holdout);
        for (index, id) in result.evaluation_refs.iter().enumerate() {
            let record = runtime.store.record(&evaluator_grant, id).unwrap();
            let evaluation: Evaluation =
                serde_json::from_value(Value::Object(record.body)).unwrap();
            let status = if mode == "failed" {
                EvaluationExecutionStatus::Failed
            } else if index == 0 {
                EvaluationExecutionStatus::Cancelled
            } else {
                EvaluationExecutionStatus::NotStarted
            };
            assert_eq!(
                evaluation.execution_status,
                Some(status),
                "{mode} case {index}"
            );
            let allocation = runtime
                .store
                .allocation(&grant, evaluation.allocation_id.as_ref().unwrap())
                .unwrap();
            assert_eq!(
                allocation.disposition,
                Some(expected.clone()),
                "{mode} case {index}"
            );
            assert_eq!(
                runtime
                    .store
                    .allocation(&grant, allocation.parent_id.as_ref().unwrap())
                    .unwrap()
                    .disposition,
                Some(expected.clone()),
                "{mode} arm"
            );
        }
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            match mode {
                "failed" => 8,
                "cancel_before" => 0,
                _ => 1,
            }
        );
    }
}

#[test]
fn source_deletion_removes_development_evaluation_copies_but_retains_protected_evidence() {
    let (directory, mut runtime, grant) = fixture(false);
    let (protected_experiment, candidate) = experiment(&runtime, &grant);
    let protected = runtime
        .run_experiment("lab-run", &protected_experiment.id)
        .unwrap();
    runtime
        .laboratory
        .register_case(EvaluationCase {
            id: "development-case".into(),
            family: "development-family".into(),
            split: Split::Development,
            input: json!({"private_context":"DEVELOPMENT-INPUT-COPY"})
                .as_object()
                .unwrap()
                .clone(),
        })
        .unwrap();
    runtime
        .laboratory
        .register_policy(AdmissionPolicy {
            id: "development-policy".into(),
            context: grant.context.clone(),
            evaluator: "units".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["development-case".into()],
            required_checks: vec!["independent-check".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.5,
            repetitions: 1,
            allowed_cells: vec!["units".into()],
            retain_learning_memory: false,
            max_evaluations: 2,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
    let mut body = serde_json::to_value(&protected_experiment.body).unwrap();
    body["policy_id"] = json!("development-policy");
    body["case_ids"] = json!(["development-case"]);
    body["scenario_families"] = json!(["development-family"]);
    body["repetitions"] = json!(1);
    let development_experiment = submit(&runtime, &grant, "experiment", body);
    let development = runtime
        .run_experiment("lab-run", &development_experiment.id)
        .unwrap();
    for reference in &development.evaluation_refs {
        assert_eq!(
            runtime.store.record(&grant, reference).unwrap().body["output"],
            "protected evaluator observation"
        );
    }
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: candidate.id,
                expected_version: candidate.version,
                delete: true,
            },
        )
        .unwrap();
    let db = rusqlite::Connection::open(directory.path().join("store.db")).unwrap();
    let policy: String = db
        .query_row(
            "SELECT policy FROM experiments WHERE id=?1",
            [&development_experiment.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !policy.contains("DEVELOPMENT-INPUT-COPY"),
        "study configuration retained deleted development inputs"
    );
    for reference in &development.evaluation_refs {
        assert!(runtime.store.record(&grant, reference).is_err());
        let body: String = db
            .query_row("SELECT body FROM records WHERE id=?1", [reference], |row| {
                row.get(0)
            })
            .unwrap();
        let record: RecordEnvelope = serde_json::from_str(&body).unwrap();
        assert!(
            record.body.is_empty(),
            "development evaluator output survived source deletion"
        );
        assert!(record.retired);
    }
    for reference in &protected.evaluation_refs {
        assert!(runtime.store.record(&grant, reference).is_err());
        let body: String = db
            .query_row("SELECT body FROM records WHERE id=?1", [reference], |row| {
                row.get(0)
            })
            .unwrap();
        let record: RecordEnvelope = serde_json::from_str(&body).unwrap();
        assert_eq!(record.provenance.split, Split::Holdout);
        assert_eq!(record.body["output"], "protected evaluator observation");
    }
}

#[test]
fn retained_experiment_results_cannot_outlive_their_source_experiment() {
    let (_directory, runtime, grant) = fixture(false);
    let (experiment, candidate) = experiment(&runtime, &grant);
    let call =
        json!({"call_id":"study","method":"experiment.run","arguments":{"id":experiment.id}});
    let result = runtime
        .tool_call("lab-run", "tool.call", call.clone())
        .unwrap();
    let aggregate: ExperimentResult =
        serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
    assert_eq!(aggregate.decision, AdmissionDecision::Accepted);
    assert!(
        !result["content"]
            .as_str()
            .unwrap()
            .contains("protected evaluator observation")
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: candidate.id,
                expected_version: candidate.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(
        runtime.tool_call("lab-run", "tool.call", call).is_err(),
        "cached experiment result remained available after its source was deleted"
    );
    assert!(
        runtime
            .tool_call(
                "lab-run",
                "artifact.read",
                json!({"path":result["artifact"]["path"],"offset":0,"length":1000})
            )
            .is_err()
    );
}

#[test]
fn unregistered_legacy_evaluator_files_require_owner_cleanup_confirmation() {
    let (directory, runtime, grant) = fixture(false);
    let (experiment, _) = experiment(&runtime, &grant);
    runtime.run_experiment("lab-run", &experiment.id).unwrap();
    let db = rusqlite::Connection::open(directory.path().join("store.db")).unwrap();
    db.execute(
        "UPDATE experiments SET result=NULL,policy=json_remove(policy,'$.workspace') WHERE id=?1",
        [&experiment.id],
    )
    .unwrap();
    let legacy = directory.path().join("legacy-evaluator");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::write(legacy.join("input.txt"), "LEGACY-INPUT-COPY").unwrap();
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: experiment.id.clone(),
                expected_version: experiment.version,
                delete: true,
            },
        )
        .unwrap();
    let status = runtime.store.source_cleanup_status(&grant).unwrap();
    assert_eq!(
        status["jobs"][0]["status"], "failed",
        "an unknown legacy workspace was reported cleaned"
    );
    assert!(legacy.exists());
    assert!(
        status["jobs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("location was not recorded")
    );
    let mut wrong_owner = grant.clone();
    wrong_owner.context = "different policy".into();
    assert!(
        runtime
            .confirm_legacy_experiment_cleanup(&wrong_owner, &experiment.id)
            .is_err()
    );
    // The host owner handles files that the old database cannot locate.
    std::fs::remove_dir_all(&legacy).unwrap();
    runtime
        .confirm_legacy_experiment_cleanup(&grant, &experiment.id)
        .unwrap();
    runtime
        .confirm_legacy_experiment_cleanup(&grant, &experiment.id)
        .unwrap();
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "complete"
    );
    assert!(
        db.query_row(
            "SELECT result IS NULL FROM experiments WHERE id=?1",
            [&experiment.id],
            |row| row.get::<_, bool>(0)
        )
        .unwrap(),
        "owner cleanup must not invent an evaluation result"
    );
}

struct ExitingEvaluator;
impl Evaluator for ExitingEvaluator {
    fn evaluate(
        &self,
        _task: &EvaluationTask,
        workspace: &Path,
        _account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        std::fs::write(workspace.join("copied-input.txt"), "CRASHED-EVALUATOR-COPY").unwrap();
        std::process::exit(73);
    }
}

#[test]
fn evaluator_process_exit_requires_owner_confirmation_before_workspace_cleanup() {
    if let Some(root) = std::env::var_os("RIBOSOME_EVALUATOR_CRASH_ROOT") {
        let (directory, runtime, grant) = fixture_with_evaluator(Box::new(ExitingEvaluator), 20);
        std::fs::write(
            Path::new(&root).join("database-directory"),
            directory.path().to_str().unwrap(),
        )
        .unwrap();
        let (experiment, _) = experiment(&runtime, &grant);
        runtime.run_experiment("lab-run", &experiment.id).unwrap();
        panic!("evaluator did not exit the child process");
    }
    let root = tempfile::tempdir().unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "evaluator_process_exit_requires_owner_confirmation_before_workspace_cleanup",
            "--nocapture",
        ])
        .env("RIBOSOME_EVALUATOR_CRASH_ROOT", root.path())
        .output()
        .unwrap();
    assert_eq!(
        child.status.code(),
        Some(73),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let directory = std::path::PathBuf::from(
        std::fs::read_to_string(root.path().join("database-directory")).unwrap(),
    );
    let database = directory.join("store.db");
    let db = rusqlite::Connection::open(&database).unwrap();
    let workspace: String = db
        .query_row(
            "SELECT json_extract(policy,'$.workspace.path') FROM experiments",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let workspace = std::path::PathBuf::from(workspace);
    assert!(
        workspace.is_dir(),
        "child exited before producing a retained workspace"
    );
    let copied_files = std::fs::read_dir(&workspace)
        .unwrap()
        .flat_map(|arm| std::fs::read_dir(arm.unwrap().path()).unwrap())
        .map(|case| case.unwrap().path().join("copied-input.txt"))
        .collect::<Vec<_>>();
    assert!(copied_files.iter().any(|path| {
        std::fs::read_to_string(path).is_ok_and(|text| text == "CRASHED-EVALUATOR-COPY")
    }));
    let store = Store::open(&database).unwrap();
    let grant = store.grant("lab-grant").unwrap();
    let candidate: String = db
        .query_row(
            "SELECT json_extract(body,'$.body.candidate.id') FROM records WHERE kind='experiment'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let candidate = store.record(&grant, &candidate).unwrap();
    store
        .retire(
            &grant,
            &RetireRequest {
                id: candidate.id,
                expected_version: candidate.version,
                delete: true,
            },
        )
        .unwrap();
    let status = store.source_cleanup_status(&grant).unwrap();
    assert_eq!(status["jobs"][0]["status"], "failed");
    assert!(
        status["jobs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("still owns its workspace")
    );
    assert!(
        workspace.exists(),
        "cleanup removed files before execution ownership was recovered"
    );
    drop(store);
    let runtime = Runtime::new(
        Store::open(&database).unwrap(),
        Box::new(LocalHost::new(&directory, BTreeMap::new()).unwrap()),
        workspace.parent().unwrap(),
    )
    .unwrap();
    assert!(
        workspace.exists(),
        "an interrupted run is not proof that its external executors stopped"
    );
    let experiment: String = db
        .query_row("SELECT id FROM experiments", [], |row| row.get(0))
        .unwrap();
    assert!(
        runtime
            .settle_experiment_workspace(&grant, &experiment, false)
            .is_err()
    );
    assert!(workspace.exists());
    runtime
        .settle_experiment_workspace(&grant, &experiment, true)
        .unwrap();
    assert!(!workspace.exists());
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "complete"
    );
    assert_eq!(
        runtime.store.inspect_run("lab-run").unwrap()["status"],
        "interrupted"
    );
    assert!(
        db.query_row("SELECT result IS NULL FROM experiments", [], |row| row
            .get::<_, bool>(0))
            .unwrap(),
        "recovery fabricated an evaluation result"
    );
}

#[test]
fn protected_family_exposure_survives_a_fresh_grant_and_policy() {
    let (_d, mut runtime, grant) = fixture(false);
    let (first, _) = experiment(&runtime, &grant);
    runtime.run_experiment("lab-run", &first.id).unwrap();
    let mut fresh = grant.clone();
    fresh.id = "fresh-grant".into();
    runtime.store.register_grant(&fresh).unwrap();
    runtime.store.begin_run("fresh-run", &fresh.id, &decode("AgentRunRequest",json!({"run_id":"fresh-run","profile":"experimenter","operator":"experiment@1","prompt":"Evaluate","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    runtime
        .laboratory
        .register_policy(AdmissionPolicy {
            id: "renamed-policy".into(),
            context: grant.context.clone(),
            evaluator: "units".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["case-1".into(), "case-2".into()],
            required_checks: vec!["independent-check".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.5,
            repetitions: 2,
            allowed_cells: vec![],
            retain_learning_memory: false,
            max_evaluations: 8,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
    let mut body = serde_json::to_value(first.body).unwrap();
    body["policy_id"] = json!("renamed-policy");
    let second = submit(&runtime, &fresh, "experiment", body);
    let error = runtime
        .run_experiment("fresh-run", &second.id)
        .expect_err("fresh grant must not reset protected family exposure");
    assert!(error.message.contains("protected"), "{error:?}");
}

struct AggregateEvaluator;
impl Evaluator for AggregateEvaluator {
    fn provider_usage_is_metered(&self) -> bool {
        true
    }
    fn evaluate(
        &self,
        task: &EvaluationTask,
        _workspace: &Path,
        _account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        let quality = task.implementation.parameters[&task.case_id][task.repetition as usize]
            .as_f64()
            .unwrap();
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(quality),
                unit: "score".into(),
            }],
            checks: vec!["independent-check".into()],
            output: "Host measurement fixture, not semantic evidence".into(),
            descriptor: Some(task.case_id.clone()),
        })
    }
}

#[test]
fn archive_uses_all_repetitions_and_retirement_invalidates_the_exact_support() {
    let (_d, mut runtime, grant) = fixture(false);
    runtime
        .laboratory
        .register_evaluator("aggregate".into(), Box::new(AggregateEvaluator))
        .unwrap();
    for case in ["a", "b"] {
        runtime
            .laboratory
            .register_case(EvaluationCase {
                id: case.into(),
                family: format!("dev-{case}"),
                split: Split::Development,
                input: serde_json::Map::new(),
            })
            .unwrap();
    }
    runtime
        .laboratory
        .register_policy(AdmissionPolicy {
            id: "aggregate".into(),
            context: grant.context.clone(),
            evaluator: "aggregate".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["a".into(), "b".into()],
            required_checks: vec!["independent-check".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.0,
            repetitions: 2,
            allowed_cells: vec!["a".into(), "b".into()],
            retain_learning_memory: false,
            max_evaluations: 8,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: Some(ExperimentStudyObjective::Function),
            learning_cost: None,
        })
        .unwrap();
    let prepare = |name: &str, scores: Value| {
        let record = material(&runtime, &grant, name);
        let mut body = record.body;
        body.insert("parameters".into(), scores);
        submit(&runtime, &grant, "implementation", Value::Object(body))
    };
    let baseline = prepare("baseline", json!({"a":[1,1],"b":[1,1]}));
    let a = prepare("context-a", json!({"a":[10,10],"b":[1,1]}));
    let b = prepare("context-b", json!({"a":[1,11],"b":[10,10]}));
    let mut first_support = String::new();
    for candidate in [&a, &b] {
        let study = submit(
            &runtime,
            &grant,
            "experiment",
            json!({"name":"aggregate-fixture","template":"stress","study_objective":"function","candidate":{"id":candidate.id,"version":"1"},"baseline":{"id":baseline.id,"version":"1"},"hypothesis":"Aggregate contextual measurement","case_ids":["a","b"],"scenario_families":["dev-a","dev-b"],"feedback":"aggregate","model_version":"fixture","tool_versions":[],"memory_start_refs":[],"repetitions":2,"budget":grant.budget,"metrics":["quality"],"policy_id":"aggregate","selection_frozen":true,"variants":[]}),
        );
        let result = runtime.run_experiment("lab-run", &study.id).unwrap();
        assert_eq!(result.decision, AdmissionDecision::Accepted);
        if first_support.is_empty() {
            first_support = study.id.clone();
        }
        let recommendation = submit(
            &runtime,
            &grant,
            "recommendation",
            json!({"implementation":{"id":candidate.id,"version":"1"},"context":grant.context,"decision":"accepted","evaluation_refs":result.evaluation_refs,"rationale":"Repeated host fixture","restrictions":["Fixture only"]}),
        );
        runtime
            .request_admission("lab-run", &recommendation.id)
            .unwrap();
    }
    let archive = runtime.store.archive(&grant).unwrap();
    assert_eq!(archive.cells.len(), 2);
    assert_eq!(
        archive.cells[0].implementation.id, a.id,
        "single lucky 11 must not displace repeated 10"
    );
    assert_eq!(archive.cells[0].quality, 10.0);
    assert_eq!(archive.cells[1].implementation.id, b.id);
    assert_eq!(archive.cells[0].evaluation_refs.as_ref().unwrap().len(), 8);
    let evidence = runtime.store.record(&grant, &first_support).unwrap();
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: first_support,
                expected_version: evidence.version,
                delete: false,
            },
        )
        .unwrap();
    let archive = runtime.store.archive(&grant).unwrap();
    assert_eq!(archive.cells.len(), 1);
    assert_eq!(archive.cells[0].implementation.id, b.id);
}

#[test]
fn protected_family_cannot_be_a_renamed_donor_family() {
    let (_d, runtime, grant) = fixture(false);
    let (study, candidate) = experiment(&runtime, &grant);
    let mut submission: RecordSubmission = decode(
        "RecordSubmission",
        json!({"kind":"implementation","provenance":candidate.provenance,"body":candidate.body}),
    )
    .unwrap();
    submission.provenance.scenario_family = "family-case-1".into();
    let donor = runtime.store.submit(&grant, &submission, false).unwrap();
    let mut body = study.body;
    body.insert("candidate".into(), json!({"id":donor.id,"version":"1"}));
    let study = submit(&runtime, &grant, "experiment", Value::Object(body));
    let error = runtime.run_experiment("lab-run", &study.id).unwrap_err();
    assert!(error.message.contains("source lineage"), "{error:?}");
}

struct OneExhaustedCase(std::sync::atomic::AtomicU32);
impl Evaluator for OneExhaustedCase {
    fn provider_usage_is_metered(&self) -> bool {
        true
    }
    fn evaluate(
        &self,
        _task: &EvaluationTask,
        _workspace: &Path,
        _account: &ribosome_core::experiments::EvaluationAccount<'_>,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<EvaluationObservation> {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            return Err(ribosome_core::error::Error::exhausted(
                "local case allowance reached",
            ));
        }
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(1.0),
                unit: "fraction".into(),
            }],
            checks: vec!["independent-check".into()],
            output: "Independent later case executed".into(),
            descriptor: None,
        })
    }
}
#[test]
fn a_local_case_limit_does_not_cancel_other_funded_cases() {
    let (_d, runtime, grant) = fixture_with_evaluator(
        Box::new(OneExhaustedCase(std::sync::atomic::AtomicU32::new(0))),
        20,
    );
    let (study, _) = experiment(&runtime, &grant);
    let result = runtime.run_experiment("lab-run", &study.id).unwrap();
    assert_eq!(result.complete, Some(false));
    assert_eq!(result.decision, AdmissionDecision::Inconclusive);
    let mut authority = grant;
    authority.visible_splits.push(Split::Holdout);
    let statuses: Vec<_> = result
        .evaluation_refs
        .iter()
        .map(|id| runtime.store.record(&authority, id).unwrap().body["execution_status"].clone())
        .collect();
    assert_eq!(statuses[0], "exhausted");
    assert_eq!(
        result.report.as_ref().unwrap()["arms"][0]["verified_success_rate"],
        0.75
    );
    assert_eq!(statuses.iter().filter(|s| **s == "completed").count(), 7);
}
