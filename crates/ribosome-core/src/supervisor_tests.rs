use super::*;
use crate::{
    host::{LocalHost, RegisteredTool},
    store::Store,
};

fn fixture() -> (tempfile::TempDir, Runtime, Grant, WorkerConfig) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant: Grant = decode("Grant", json!({"id":"concurrency-grant","scope":{"client":"test","project":"concurrency"},"mode":"apply","paths":[],"tools":["slow"],"profiles":["caretaker"],"budget":{"max_calls":5,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":5,"max_work_items":5,"max_depth":1,"deadline_ms":(now_ms()+30000).to_string()},"context":"concurrency","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(
        directory.path(),
        BTreeMap::from([(
            "slow".into(),
            RegisteredTool {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "printf started > command-started; /bin/sleep 5".into(),
                ],
                timeout_ms: 10000,
                reads: vec![],
                validates: vec![],
                writes: vec![],
                code_files: vec![],
                validated_properties: vec![],
            },
        )]),
    )
    .unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let config = WorkerConfig {
        node: String::from_utf8(node.stdout).unwrap().trim().into(),
        worker: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/integration/concurrency-worker-fixture.mjs")
            .canonicalize()
            .unwrap(),
        environment: BTreeMap::new(),
    };
    let runtime = Runtime::new(store, Box::new(host), directory.path().join("state")).unwrap();
    (directory, runtime, grant, config)
}

fn request(directory: &std::path::Path, role: &str) -> AgentRunRequest {
    AgentRunRequest {
        run_id: role.into(),
        profile: Profile::Caretaker,
        operator: "proofreading@1".into(),
        prompt: json!({"directory": directory, "role": role}).to_string(),
        provider: "openai".into(),
        model: "test-model".into(),
        checkpoint: None,
        parent_allocation_id: None,
        invocation: None,
        discovery_corpus: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn five_second_command_does_not_block_another_workers_state_requests() {
    let (directory, runtime, grant, config) = fixture();
    let supervisor = Supervisor::new(runtime, 2).unwrap();
    let (_send, cancel) = watch::channel(false);
    let (command, probe) = tokio::join!(
        supervisor.run(
            &config,
            &grant.id,
            request(directory.path(), "command"),
            cancel.clone()
        ),
        supervisor.run(
            &config,
            &grant.id,
            request(directory.path(), "probe"),
            cancel
        ),
    );
    let command = command.unwrap();
    let probe = probe.unwrap();
    assert_eq!(
        command.disposition,
        Disposition::Completed,
        "{}",
        command.summary
    );
    assert_eq!(
        probe.disposition,
        Disposition::Completed,
        "{}",
        probe.summary
    );
    let timings: BTreeMap<String, f64> = serde_json::from_str(&probe.summary).unwrap();
    eprintln!("Independent worker latency in ms: {timings:?}");
    for (method, elapsed) in timings {
        assert!(
            elapsed < 1000.0,
            "{method} took {elapsed:.1} ms during a five-second command"
        );
    }
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    let command = runtime.store.inspect_run("command").unwrap();
    let probe = runtime.store.inspect_run("probe").unwrap();
    let micros = |run: &serde_json::Value, phase: &str| {
        run["timings"]["spans"][phase]["total_us"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert!(micros(&command, "executor_service") >= 5_000_000);
    assert_eq!(command["timings"]["spans"]["worker_startup"]["samples"], 1);
    assert_eq!(probe["timings"]["spans"]["state_service"]["samples"], 3);
    assert!(micros(&probe, "state_service") < 1_000_000);
    assert!(micros(&probe, "state_queue") < 1_000_000);
    eprintln!("Persisted command timing: {}", command["timings"]);
    eprintln!("Persisted probe timing: {}", probe["timings"]);
}

use crate::experiments::{AdmissionPolicy, EvaluationCase, Evaluator};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

// An evaluator may not cooperate with cancellation immediately. The supervisor
// must retain its actual completion handle instead of assuming it has stopped.
struct WaitingEvaluator {
    marker: PathBuf,
    release: Arc<AtomicBool>,
}
impl Evaluator for WaitingEvaluator {
    fn evaluate(
        &self,
        _: &EvaluationTask,
        _: &Path,
        _: &crate::experiments::EvaluationAccount<'_>,
        _: &AtomicBool,
    ) -> Result<EvaluationObservation> {
        std::fs::write(&self.marker, "started")?;
        let start = Instant::now();
        while !self.release.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(EvaluationObservation {
            passed: Some(true),
            measurements: vec![Measurement {
                name: "quality".into(),
                value: Some(1.0),
                unit: "fraction".into(),
            }],
            checks: vec!["fixture-check".into()],
            output: "Concurrency fixture; no semantic measurement.".into(),
            descriptor: None,
        })
    }
}

fn study(
    runtime: &mut Runtime,
    grant: &Grant,
    directory: &Path,
    release: Arc<AtomicBool>,
) -> String {
    runtime
        .laboratory
        .register_evaluator(
            "waiting".into(),
            Box::new(WaitingEvaluator {
                marker: directory.join("command-started"),
                release,
            }),
        )
        .unwrap();
    runtime
        .laboratory
        .register_case(EvaluationCase {
            id: "case".into(),
            family: "fixture".into(),
            split: Split::Development,
            input: serde_json::Map::new(),
        })
        .unwrap();
    runtime
        .laboratory
        .register_policy(AdmissionPolicy {
            id: "waiting".into(),
            context: grant.context.clone(),
            evaluator: "waiting".into(),
            evaluator_version: "1".into(),
            case_ids: vec!["case".into()],
            required_checks: vec!["fixture-check".into()],
            metric: "quality".into(),
            min_quality: 1.0,
            min_improvement: 0.5,
            repetitions: 1,
            allowed_cells: vec![],
            retain_learning_memory: false,
            max_evaluations: 2,
            allow_generated_development_cases: false,
            case_budget: None,
            study_objective: None,
            learning_cost: None,
        })
        .unwrap();
    let submit = |kind, body| {
        runtime.store.submit(grant, &decode("RecordSubmission", json!({"kind":kind,"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"fixture","split":"development","limitations":["concurrency fixture"]},"body":body})).unwrap(),false).unwrap()
    };
    let implementation = submit(
        "implementation",
        json!({"name":"fixture","version":"1","motifs":[],"format":"instructions","material":"Concurrency fixture","parameters":{},"required_capabilities":[],"state_assumptions":[],"possible_effects":[],"failure_behavior":"abstain","evaluation_refs":[]}),
    );
    submit("experiment", json!({"name":"contention","template":"transfer","candidate":{"id":implementation.id,"version":"1"},"baseline":{"id":implementation.id,"version":"1"},"hypothesis":"Fixture measures contention only","case_ids":["case"],"scenario_families":["fixture"],"feedback":"aggregate","model_version":"fixture","tool_versions":["waiting@1"],"memory_start_refs":[],"repetitions":1,"budget":grant.budget,"metrics":["quality"],"policy_id":"waiting","selection_frozen":true,"variants":[]})).id
}

async fn marker(directory: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !directory.join("command-started").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("executor did not start");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evaluator_does_not_block_state_and_dropped_waiter_retains_execution_ownership() {
    let (directory, mut runtime, grant, config) = fixture();
    let attachment_store = runtime.store.service_connection().unwrap();
    let policy = crate::attachments::AttachmentPolicy::default();
    attachment_store.open_attachment(&grant, &decode("AttachmentOpen", json!({"id":"attached","execution_id":"source","connector":"custom","connector_version":"1","start":"now","capabilities":["observe"]})).unwrap(), &policy, &request(directory.path(), "attachment-template")).unwrap();
    let release = Arc::new(AtomicBool::new(false));
    let experiment = study(&mut runtime, &grant, directory.path(), release.clone());
    let mut run = request(directory.path(), "experiment");
    run.prompt = json!({"directory":directory.path(),"role":"experiment","experiment":experiment})
        .to_string();
    let supervisor = Supervisor::new(runtime, 2).unwrap();
    let (_send, cancel) = watch::channel(false);
    let caller = {
        let supervisor = supervisor.clone();
        let config = config.clone();
        let grant = grant.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { supervisor.run(&config, &grant.id, run, cancel).await })
    };
    marker(directory.path()).await;
    let ingestion_started = Instant::now();
    attachment_store.append_attachment_events(&decode("AttachmentEvents", json!({"attachment_id":"attached","events":[{"id":"event-1","producer":"source","sequence":"1","kind":"tool.completed","timestamp_ms":"1","parents":[],"correlation":"task","artifacts":[],"payload":{}}]})).unwrap(), &policy).unwrap();
    attachment_store.attachment_status("attached").unwrap();
    let ingestion_latency = ingestion_started.elapsed();
    let probe = supervisor
        .run(
            &config,
            &grant.id,
            request(directory.path(), "probe"),
            cancel,
        )
        .await
        .unwrap();
    // Keep the evaluator active for the same five-second contention interval.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let before_abort = supervisor
        .runtime
        .lock()
        .await
        .store
        .inspect_run("experiment")
        .unwrap();
    caller.abort();
    let _ = caller.await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while !supervisor
            .runtime
            .lock()
            .await
            .cancellation("experiment")
            .load(Ordering::SeqCst)
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let during_settlement = supervisor
        .runtime
        .lock()
        .await
        .store
        .inspect_run("experiment")
        .unwrap();
    let ownership_held = supervisor.executor.try_lock().is_err();
    release.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let status = supervisor
                .runtime
                .lock()
                .await
                .store
                .inspect_run("experiment")
                .unwrap()["status"]
                .clone();
            if status != "running" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(before_abort["status"], "running");
    assert!(
        ingestion_latency < Duration::from_secs(1),
        "attachment ingestion/status took {ingestion_latency:?}"
    );
    assert_eq!(during_settlement["status"], "running");
    assert!(
        ownership_held,
        "dropping the waiter released an unsettled executor"
    );
    assert_eq!(
        probe.disposition,
        Disposition::Completed,
        "{}",
        probe.summary
    );
    let timings: BTreeMap<String, f64> = serde_json::from_str(&probe.summary).unwrap();
    eprintln!("Worker latency during evaluator in ms: {timings:?}");
    assert!(timings.values().all(|latency| *latency < 1000.0));
    assert!(supervisor.executor.try_lock().is_ok());
    assert!(supervisor.slots.available_permits() == 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_disconnect_settles_issued_command_and_service_open_does_not_recover_active_runs() {
    let (directory, runtime, grant, config) = fixture();
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let mut run = request(directory.path(), "effect");
    run.prompt = json!({"directory":directory.path(),"role":"effect","disconnect":true,"action":{"kind":"check","tool":"slow","operation_id":"disconnected-command"}}).to_string();
    let (_send, cancel) = watch::channel(false);
    let running = supervisor.run(&config, &grant.id, run, cancel);
    let inspect = async {
        marker(directory.path()).await;
        let runtime = supervisor.runtime.lock().await;
        let connection = runtime.service_connection().unwrap();
        connection.store.inspect_run("effect").unwrap()["status"].clone()
    };
    let (result, status) = tokio::join!(running, inspect);
    assert_eq!(status, "running");
    assert_eq!(result.unwrap().disposition, Disposition::Interrupted);
    let runtime = supervisor.executor.lock().await;
    let receipt = runtime.lookup("effect", "disconnected-command").unwrap();
    assert_ne!(receipt.status, EffectStatus::Started);
    assert!(
        !receipt.reconciled,
        "normal executor capture must settle before run completion"
    );
    let phase: String = runtime
        .store
        .db
        .query_row(
            "SELECT phase FROM effects WHERE id='disconnected-command'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(phase, "finalized");
    assert_eq!(supervisor.slots.available_permits(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflicting_repairs_serialize_and_recheck_versions_while_reads_continue() {
    let (directory, mut runtime, mut grant, config) = fixture();
    std::fs::write(directory.path().join("artifact.txt"), "before").unwrap();
    grant.paths = vec!["artifact.txt".into()];
    grant.id = "repair-grant".into();
    runtime.store.register_grant(&grant).unwrap();
    drop(runtime.host);
    runtime.host = Arc::new(
        LocalHost::new(
            directory.path(),
            BTreeMap::from([(
                "slow".into(),
                RegisteredTool {
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        format!(
                            "printf started > '{}'; /bin/sleep 5; test -s artifact.txt",
                            directory.path().join("command-started").display()
                        ),
                    ],
                    timeout_ms: 10000,
                    reads: vec!["artifact.txt".into()],
                    validates: vec!["artifact.txt".into()],
                    writes: vec![],
                    code_files: vec![],
                    validated_properties: vec![],
                },
            )]),
        )
        .unwrap(),
    );
    let before = runtime.host.version(&grant, "artifact.txt", None).unwrap();
    let setup = request(directory.path(), "setup");
    runtime.store.begin_run("setup", &grant.id, &setup).unwrap();
    let branch = runtime
        .execute(
            "setup",
            decode("Action", json!({"kind":"branch","operation_id":"branch"})).unwrap(),
        )
        .unwrap()
        .output;
    let edited = runtime.execute("setup", decode("Action", json!({"kind":"edit","operation_id":"branch-edit","branch_id":branch,"path":"artifact.txt","expected_version":before.version,"content":"after"})).unwrap()).unwrap();
    assert_eq!(edited.status, EffectStatus::Succeeded);
    runtime
        .store
        .finish_run(
            "setup",
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "Prepared repair branch.".into(),
            },
        )
        .unwrap();
    let provenance = json!({"origin":"synthetic","source_refs":[],"scenario_family":"fixture","split":"development","limitations":["concurrency fixture"]});
    let finding = runtime.store.submit(&grant, &decode("RecordSubmission", json!({"kind":"finding","provenance":provenance,"body":{"subject":"artifact.txt","observation":"artifact requires correction","interpretation":"repair the artifact","evidence_refs":[],"uncertainty":[],"operator":"excision-repair@1"}})).unwrap(), false).unwrap();
    let intervention = runtime.store.submit(&grant, &decode("RecordSubmission", json!({"kind":"intervention","provenance":provenance,"body":{"kind":"repair","subject":"artifact.txt","finding_ref":finding.id,"read_versions":[],"preserve":[],"replace":["artifact.txt"],"invalidate":[],"recompute":[],"required_checks":["slow"],"bindings":{},"requested_effects":["edit artifact"],"assumptions":[],"fallback":"abstain","operator":"excision-repair@1"}})).unwrap(),false).unwrap();
    let supervisor = Supervisor::new(runtime, 3).unwrap();
    let mut writer = request(directory.path(), "writer");
    writer.prompt = json!({"directory": directory.path(),"role":"effect","action":{"kind":"apply","branch_id":branch,"path":"artifact.txt","expected_version":before.version,"intervention_ref":intervention.id,"content":"after","operation_id":"writer"}}).to_string();
    let mut editor = request(directory.path(), "editor");
    editor.prompt = json!({"directory": directory.path(),"role":"effect","action":{"kind":"edit","path":"artifact.txt","expected_version":before.version,"content":"conflicting edit","operation_id":"editor"}}).to_string();
    let (_send, cancel) = watch::channel(false);
    let writing = async {
        let result = supervisor
            .run(&config, &grant.id, writer, cancel.clone())
            .await;
        let receipt = supervisor
            .executor
            .lock()
            .await
            .lookup("writer", "writer")
            .unwrap();
        assert_eq!(
            receipt.status,
            EffectStatus::Succeeded,
            "{}",
            receipt.output
        );
        result
    };
    let others = async {
        marker(directory.path()).await;
        tokio::join!(
            supervisor.run(&config, &grant.id, editor, cancel.clone()),
            supervisor.run(
                &config,
                &grant.id,
                request(directory.path(), "probe"),
                cancel.clone()
            ),
        )
    };
    let (writer, (editor, probe)) = tokio::join!(writing, others);
    assert_eq!(writer.unwrap().disposition, Disposition::Completed);
    assert_eq!(editor.unwrap().disposition, Disposition::Completed);
    let probe = probe.unwrap();
    assert_eq!(
        probe.disposition,
        Disposition::Completed,
        "{}",
        probe.summary
    );
    let timings: BTreeMap<String, f64> = serde_json::from_str(&probe.summary).unwrap();
    assert!(timings.values().all(|latency| *latency < 1000.0));
    let runtime = supervisor.executor.lock().await;
    assert_eq!(
        runtime.lookup("writer", "writer").unwrap().status,
        EffectStatus::Succeeded
    );
    assert_eq!(
        runtime.lookup("editor", "editor").unwrap().status,
        EffectStatus::Stale
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("artifact.txt")).unwrap(),
        "after"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_queued_effect_does_not_wait_for_an_unrelated_command_or_dispatch_later() {
    let (directory, runtime, grant, config) = fixture();
    std::fs::write(directory.path().join("probe-ready"), "ready").unwrap();
    let supervisor = Supervisor::new(runtime, 2).unwrap();
    let mut queued = request(directory.path(), "queued");
    queued.prompt = json!({"directory":directory.path(),"role":"effect","reportQueued":true,"action":{"kind":"check","tool":"slow","operation_id":"must-not-dispatch"}}).to_string();
    let (_send, cancel) = watch::channel(false);
    let (stop, stopped) = watch::channel(false);
    let command = supervisor.run(
        &config,
        &grant.id,
        request(directory.path(), "command"),
        cancel,
    );
    let queue = async {
        marker(directory.path()).await;
        let cancel_queued = async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while !directory.path().join("request-queued").exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let started = Instant::now();
            stop.send(true).unwrap();
            started
        };
        let (result, started) = tokio::join!(
            supervisor.run(&config, &grant.id, queued, stopped),
            cancel_queued
        );
        (result.unwrap(), started.elapsed())
    };
    let (command, (queued, latency)) = tokio::join!(command, queue);
    assert_eq!(command.unwrap().disposition, Disposition::Completed);
    assert_eq!(
        queued.disposition,
        Disposition::Cancelled,
        "{}",
        queued.summary
    );
    assert!(
        latency < Duration::from_secs(1),
        "queued cancellation took {latency:?}"
    );
    let runtime = supervisor.executor.lock().await;
    assert!(runtime.lookup("queued", "must-not-dispatch").is_err());
    assert_eq!(
        runtime.store.inspect_run("queued").unwrap()["effects"],
        json!([])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_worker_can_persist_its_final_checkpoint() {
    let (directory, runtime, grant, config) = fixture();
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (send, cancel) = watch::channel(false);
    let running = supervisor.run(
        &config,
        &grant.id,
        request(directory.path(), "cancellation"),
        cancel,
    );
    let trigger = async {
        marker(directory.path()).await;
        send.send(true).unwrap();
    };
    let (result, ()) = tokio::join!(running, trigger);
    assert_eq!(result.unwrap().disposition, Disposition::Cancelled);
    let checkpoint = supervisor
        .runtime
        .lock()
        .await
        .store
        .load_checkpoint("cancellation")
        .unwrap();
    assert!(
        checkpoint.is_some(),
        "cancellation must allow checkpoint settlement"
    );
}
