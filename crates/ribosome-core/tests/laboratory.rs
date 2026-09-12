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
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(d.path().join("store.db")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"lab-grant","scope":{"client":"test","project":"lab"},"mode":"sandbox","paths":[],"tools":[],"profiles":["experimenter"],"budget":{"max_calls":20,"max_tokens":"100000","max_cost_microusd":"1000000","max_actions":10,"max_work_items":4,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"measurements","visible_splits":["development"],"allow_export":true})).unwrap();
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
        .register_evaluator("units".into(), Box::new(TestEvaluator { unknown }))
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
