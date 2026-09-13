use ribosome_core::{
    agent_evaluator::{AgentEvaluator, AgentEvaluatorConfig, AgentStage},
    contracts::*,
    effects::Runtime,
    experiments::{AdmissionPolicy, CommandEvaluator, EvaluationCase},
    host::{LocalHost, RegisteredTool},
    store::Store,
    supervisor::WorkerConfig,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn pi_subjects_have_isolated_memory_protected_oracles_and_one_shared_wallet() {
    run_agent_study(false);
}
#[test]
fn whole_workflow_controls_execute_fresh_stages_without_manufacturing_uplift() {
    run_agent_study(true);
}
fn run_agent_study(system: bool) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let selected = std::process::Command::new("node").args(["--input-type=module","-e","import {fixtureModel} from './tests/integration/model-fixture.mjs'; process.stdout.write(JSON.stringify({node:process.execPath,model:fixtureModel('openai').id}));"]).current_dir(&root).output().unwrap();
    assert!(selected.status.success());
    let selected: Value = serde_json::from_slice(&selected.stdout).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let grant: Grant = decode("Grant",json!({"id":"lab","scope":{"client":"test","project":"agent-study"},"mode":"sandbox","paths":[],"tools":["recipient-check"],"profiles":["experimenter"],"budget":{"max_calls":128,"max_tokens":"10000000","max_cost_microusd":"10000000","max_actions":128,"max_work_items":2,"max_depth":3,"deadline_ms":(now_ms()+120000).to_string()},"context":"recipient","visible_splits":["development"],"allow_export":false})).unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    let mut runtime = Runtime::new(
        store,
        Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
        directory.path().join("state"),
    )
    .unwrap();
    let tools = BTreeMap::from([(
        "recipient-check".into(),
        RegisteredTool {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "test \"$(cat input.txt)\" = ready".into()],
            timeout_ms: 1000,
            reads: vec!["input.txt".into()],
            validates: vec!["input.txt".into()],
            writes: vec![],
            code_files: vec![],
            validated_properties: vec![],
        },
    )]);
    for (id, state, expected) in [
        ("supported", "ready", "check"),
        ("missing", "unknown", "abstain"),
    ] {
        runtime.laboratory.register_case(EvaluationCase {id:id.into(),family:format!("fixture-{id}"),split:Split::Holdout,input:json!({"subject":{"prompt":"Follow the prepared policy; use input.txt only.","files":{"input.txt":state},"bindings":{"input":"input.txt"}},"oracle":{"expected":expected,"hidden_marker":"SEALED-ANSWER"}}).as_object().unwrap().clone()}).unwrap();
    }
    let provenance = json!({"origin":"synthetic","source_refs":[],"scenario_family":"fixture-donor","split":"development","limitations":["Provider test double; not live qualification"]});
    let material = json!({"name":"ordinary-prepared-policy","version":"1","motifs":[],"format":"instructions","material":"Inspect input. If it says ready, check the recipient. If it says unknown, abstain. Never invent missing inputs.","parameters":{},"required_capabilities":["recipient-check"],"state_assumptions":[],"possible_effects":[],"failure_behavior":"Abstain with missing evidence","evaluation_refs":[],"instruction_contract":{"inputs":[{"name":"input","kind":"artifact_path","required":true}],"outputs":["recipient observation"],"entry_obligations":["Read input"],"exit_obligations":["Report observed status"],"limitations":["Fixture policy"],"discovery_refs":[]}});
    let submit = |runtime: &Runtime, kind: &str, body: Value| {
        runtime
            .store
            .submit(
                &grant,
                &decode(
                    "RecordSubmission",
                    json!({"kind":kind,"provenance":provenance,"body":body}),
                )
                .unwrap(),
                false,
            )
            .unwrap()
    };
    let baseline = submit(&runtime, "implementation", material.clone());
    let candidate = submit(&runtime, "implementation", material);
    let mut stages = BTreeMap::new();
    if system {
        let reference = VersionRef {
            id: baseline.id.clone(),
            version: "1".into(),
        };
        for arm in ["baseline", "retry", "critique", "care", "candidate"] {
            let mut workflow = vec![AgentStage {
                implementation: Some(reference.clone()),
                prompt: "Complete the recipient task.".into(),
            }];
            if arm != "baseline" {
                workflow.push(AgentStage {
                    implementation: if arm == "candidate" {
                        None
                    } else {
                        Some(reference.clone())
                    },
                    prompt: "Inspect the previous attempt and complete this control stage.".into(),
                });
            }
            stages.insert(arm.into(), workflow);
        }
    }
    runtime
        .laboratory
        .register_evaluator(
            "agent".into(),
            Box::new(AgentEvaluator {
                config: AgentEvaluatorConfig {
                    provider: "openai".into(),
                    model: selected["model"].as_str().unwrap().into(),
                    model_version: "fixture-provider@1".into(),
                    tool_versions: vec!["recipient-check@1".into()],
                    tools,
                    paths: vec!["input.txt".into()],
                    writable_paths: vec![],
                    stages,
                    secondary_judges: vec![],
                    judge: CommandEvaluator {
                        program: selected["node"].as_str().unwrap().into(),
                        args: vec![
                            root.join("tests/integration/agent-evaluator-judge.mjs")
                                .display()
                                .to_string(),
                        ],
                        timeout_ms: 1000,
                    },
                },
                worker: WorkerConfig {
                    node: selected["node"].as_str().unwrap().into(),
                    worker: root.join("tests/integration/agent-evaluator-worker.mjs"),
                    environment: BTreeMap::new(),
                },
            }),
        )
        .unwrap();
    let mut case_budget = grant.budget.clone();
    case_budget.max_calls = if system { 6 } else { 8 };
    case_budget.max_actions = 8;
    runtime
        .laboratory
        .register_policy(AdmissionPolicy {
            id: "functional".into(),
            context: grant.context.clone(),
            evaluator: "agent".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["supported".into(), "missing".into()],
            required_checks: vec!["independent-check".into()],
            metric: if system {
                "verified_success"
            } else {
                "quality"
            }
            .into(),
            min_quality: 1.0,
            min_improvement: if system { 0.1 } else { 0.0 },
            repetitions: 2,
            allowed_cells: vec!["check".into(), "abstain".into()],
            retain_learning_memory: false,
            max_evaluations: 20,
            allow_generated_development_cases: false,
            case_budget: Some(case_budget),
            study_objective: Some(if system {
                ExperimentStudyObjective::SystemBenefit
            } else {
                ExperimentStudyObjective::Function
            }),
            learning_cost: None,
        })
        .unwrap();
    let experiment = submit(
        &runtime,
        "experiment",
        json!({"name":"actual-pi-study","template":"transfer","study_objective":if system {"system_benefit"} else {"function"},"candidate":{"id":candidate.id,"version":"1"},"baseline":{"id":baseline.id,"version":"1"},"hypothesis":"Declared checks and abstention fulfill the local contract","case_ids":["supported","missing"],"scenario_families":["fixture-supported","fixture-missing"],"feedback":"aggregate","model_version":"fixture-provider@1","tool_versions":["recipient-check@1"],"memory_start_refs":[],"repetitions":2,"budget":grant.budget,"metrics":["quality"],"policy_id":"functional","selection_frozen":true,"variants":if system {vec![json!({"arm":"retry","implementation":{"id":baseline.id,"version":"1"}}),json!({"arm":"critique","implementation":{"id":baseline.id,"version":"1"}}),json!({"arm":"care","implementation":{"id":baseline.id,"version":"1"}})]} else {vec![]}}),
    );
    runtime.store.begin_run("study-run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"study-run","profile":"experimenter","operator":"experiment@1","prompt":"Execute study","provider":"openai","model":"fixture"})).unwrap()).unwrap();
    let result = runtime.run_experiment("study-run", &experiment.id).unwrap();
    let expected_decision = if system {
        AdmissionDecision::Rejected
    } else {
        AdmissionDecision::Accepted
    };
    if result.decision != expected_decision {
        let mut evaluator = grant.clone();
        evaluator.visible_splits.push(Split::Holdout);
        for reference in &result.evaluation_refs {
            eprintln!(
                "{:?}",
                runtime.store.record(&evaluator, reference).unwrap().body
            );
        }
    }
    assert_eq!(result.complete, Some(true));
    assert_eq!(result.usage_complete, Some(true));
    assert_eq!(result.decision, expected_decision);
    let wallet = runtime.store.root_budget_status(&grant).unwrap();
    assert_eq!(wallet.usage.model_calls, if system { 68 } else { 24 }); // Retry/care can continue after the first stage uses its three calls.
    assert_eq!(wallet.usage.actions, if system { 20 } else { 8 });
    assert_eq!(
        result.report.as_ref().unwrap()["online_usage"]["model_calls"],
        if system { 68 } else { 24 }
    );
    if system {
        let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
        let mismatched: i64 = db.query_row("SELECT count(*) FROM budget_allocations stage JOIN budget_allocations child ON child.parent_id=stage.id JOIN run_allocations binding ON binding.allocation_id=child.id JOIN runs run ON run.id=binding.run_id WHERE json_extract(stage.body,'$.purpose') LIKE 'agent-stage-%' AND json_extract(stage.body,'$.disposition')<>run.status", [], |row| row.get(0)).unwrap();
        assert_eq!(
            mismatched, 0,
            "stage allocations must retain their actual execution disposition"
        );
        assert!(
            result.report.as_ref().unwrap()["arms"]
                .as_array()
                .unwrap()
                .iter()
                .all(|arm| arm["observed_mean_quality"] == 1.0),
            "a no-effect follow-up must retain the preceding worker's branch"
        );
        assert_eq!(
            result.report.as_ref().unwrap()["comparisons"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        return;
    }
    let recommendation = submit(
        &runtime,
        "recommendation",
        json!({"implementation":{"id":candidate.id,"version":"1"},"context":grant.context,"decision":"accepted","evaluation_refs":result.evaluation_refs,"rationale":"Observed repeated checks and abstentions","restrictions":["Fixture provider"]}),
    );
    runtime
        .request_admission("study-run", &recommendation.id)
        .unwrap();
    let archive = runtime.store.archive(&grant).unwrap();
    assert_eq!(archive.cells.len(), 2);
    assert!(
        archive
            .cells
            .iter()
            .all(|cell| cell.evaluation_refs.as_ref().unwrap().len() == 8)
    );
    assert_eq!(
        runtime.store.inspect_run("study-run").unwrap()["status"],
        "running",
        "nested runtime must not interrupt its parent"
    );
}
