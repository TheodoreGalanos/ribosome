use crate::{
    contracts::*,
    effects::Runtime,
    host::{LocalHost, RegisteredTool},
    store::Store,
    supervisor::{Supervisor, WorkerConfig},
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio::sync::watch;

fn fixture() -> (
    tempfile::TempDir,
    Runtime,
    Grant,
    WorkerConfig,
    AgentRunRequest,
) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("child-result.txt"), "CHILD-CHECK").unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"parking-grant","scope":{"client":"test","project":"parking"},"mode":"apply","paths":["child-result.txt"],"tools":["child-check"],"profiles":["caretaker","curator"],"budget":{"max_calls":10,"max_tokens":"3000000","max_cost_microusd":"1000000","max_actions":2,"max_work_items":3,"max_depth":3,"deadline_ms":(now_ms()+30000).to_string()},"context":"parking","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(
        directory.path(),
        BTreeMap::from([(
            "child-check".into(),
            RegisteredTool {
                program: "/bin/cat".into(),
                args: vec!["child-result.txt".into()],
                timeout_ms: 5000,
                reads: vec!["child-result.txt".into()],
                validates: vec!["child-result.txt".into()],
                writes: vec![],
                code_files: vec![],
                validated_properties: vec![],
            },
        )]),
    )
    .unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let node=std::process::Command::new("node").args(["--input-type=module","-e","import {fixtureModel} from './tests/integration/model-fixture.mjs'; process.stdout.write(JSON.stringify({node:process.execPath,model:fixtureModel('openai').id}));"]).current_dir(&root).output().unwrap();
    assert!(node.status.success());
    let selected: Value = serde_json::from_slice(&node.stdout).unwrap();
    let config = WorkerConfig {
        node: selected["node"].as_str().unwrap().into(),
        worker: root.join("tests/integration/parking-worker-fixture.mjs"),
        environment: BTreeMap::from([(
            "RIBOSOME_PARK_FIXTURE".into(),
            directory.path().to_string_lossy().into_owned(),
        )]),
    };
    let request:AgentRunRequest=decode("AgentRunRequest",json!({"run_id":"parent","profile":"caretaker","operator":"proofreading@1","prompt":"Request a curator check, wait for it, then inspect its result.","provider":"openai","model":selected["model"]})).unwrap();
    let runtime = Runtime::new(store, Box::new(host), directory.path().join("state")).unwrap();
    (directory, runtime, grant, config, request)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_one_parks_real_pi_runs_child_and_resumes_the_same_parent() {
    let (directory, runtime, grant, config, request) = fixture();
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (_send, receive) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        supervisor.run(&config, &grant.id, request, receive),
    )
    .await
    .expect("parent and child deadlocked at capacity one")
    .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("child-result.txt")).unwrap(),
        "CHILD-CHECK"
    );
    let events: Vec<Value> = std::fs::read_to_string(directory.path().join("workers.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "start")
        .collect();
    assert_eq!(starts.len(), 3);
    assert_eq!(starts[0]["run_id"], "parent");
    assert_ne!(starts[1]["run_id"], "parent");
    assert_eq!(starts[2]["run_id"], "parent");
    assert_eq!(starts[2]["checkpoint"], true);
    assert!(starts.iter().all(|event| event["previous_alive"] == false));
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime.store.inspect_run("parent").unwrap()["status"],
        "completed"
    );
    assert!(runtime.store.waiting_work("parent").unwrap().is_none());
    let timings = runtime.store.inspect_run("parent").unwrap()["timings"].clone();
    assert_eq!(timings["spans"]["worker_startup"]["samples"], 2);
    assert_eq!(timings["spans"]["worker_queue"]["samples"], 2);
    assert_eq!(timings["spans"]["child_wait"]["samples"], 1);
    assert_eq!(timings["provider_round_trip"]["observed_calls"], 4);
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT count(*) FROM work", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT status FROM work", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "completed"
    );
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        6
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT count(*) FROM effects", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timing_write_failure_preserves_real_pi_completion_and_one_host_effect() {
    let (_directory, runtime, grant, config, request) = fixture();
    runtime.store.db.execute_batch("CREATE TRIGGER reject_timing BEFORE UPDATE OF timings ON runs BEGIN SELECT RAISE(ABORT,'timing fault'); END").unwrap();
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (_send, receive) = watch::channel(false);
    let result = supervisor
        .run(&config, &grant.id, request, receive)
        .await
        .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime.store.inspect_run("parent").unwrap()["timings"]["spans"],
        json!({})
    );
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        6
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM effects WHERE phase='finalized'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM work WHERE status='completed'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        1
    );
}

fn prepared_wait(
    runtime: &Runtime,
    grant: &Grant,
    request: &AgentRunRequest,
    memory: Option<&RecordEnvelope>,
) -> (WorkItem, SessionParkRequest) {
    runtime
        .store
        .begin_run(&request.run_id, &grant.id, request)
        .unwrap();
    let context = runtime.store.authorize_context(&request.run_id).unwrap();
    let sources: Vec<_> = memory
        .into_iter()
        .map(|record| ContextSource {
            kind: ContextSourceKind::Record,
            id: record.id.clone(),
            version: record.version.clone(),
        })
        .collect();
    let mut context = runtime.store.append_context(&request.run_id, &decode("ContextAppend",json!({"segment_id":context.segment_id,"after":"0","entries":[{"message":{"role":"user","content":"Seeded coherent parent continuation for a persistence fault test.","timestamp":1},"sources":sources}]})).unwrap()).unwrap();
    let args = json!({"subject":"child-check","profile":"curator","operator":"discovery@1","reason":"Execute child-check and report its actual receipt.","evidence_refs":[]});
    let observed: ToolObservation = decode(
        "ToolObservation",
        runtime
            .tool_call(
                &request.run_id,
                "tool.call",
                json!({"call_id":"request-child","method":"work.request","arguments":args}),
            )
            .unwrap(),
    )
    .unwrap();
    let work: WorkItem = serde_json::from_str(&observed.content).unwrap();
    let wait_args = json!({"work_ids":[work.id]});
    let wait: ToolObservation = decode(
        "ToolObservation",
        runtime
            .tool_call(
                &request.run_id,
                "tool.call",
                json!({"call_id":"wait-child","method":"work.wait","arguments":wait_args}),
            )
            .unwrap(),
    )
    .unwrap();
    let entries = json!([
        {"message":{"role":"assistant","content":[{"type":"toolCall","id":"request-child","name":"work_request","arguments":args}],"timestamp":2},"sources":[]},
        {"message":{"role":"toolResult","toolCallId":"request-child","toolName":"work_request","content":[{"type":"text","text":observed.content}],"details":observed,"isError":false,"timestamp":3},"sources":observed.sources},
        {"message":{"role":"assistant","content":[{"type":"toolCall","id":"wait-child","name":"work_wait","arguments":wait_args}],"timestamp":4},"sources":[]},
        {"message":{"role":"toolResult","toolCallId":"wait-child","toolName":"work_wait","content":[{"type":"text","text":wait.content}],"details":wait,"isError":false,"timestamp":5},"sources":wait.sources}
    ]);
    context = runtime
        .store
        .append_context(
            &request.run_id,
            &decode(
                "ContextAppend",
                json!({"segment_id":context.segment_id,"after":context.count,"entries":entries}),
            )
            .unwrap(),
        )
        .unwrap();
    let checkpoint = Checkpoint {
        format: "pi-0.85.1/2".into(),
        profile: request.profile.clone(),
        operator: request.operator.clone(),
        provider: request.provider.clone(),
        model: request.model.clone(),
        messages: vec![],
        pending_operations: vec![],
        event_cursor: "0".into(),
        context: Some(context),
    };
    (
        work.clone(),
        SessionParkRequest {
            work_ids: vec![work.id],
            checkpoint,
        },
    )
}

#[test]
fn stopped_ancestors_deny_new_child_effects_and_work_but_allow_usage_settlement() {
    for disposition in [
        Disposition::Cancelled,
        Disposition::Exhausted,
        Disposition::Failed,
    ] {
        let (_directory, runtime, grant, _config, request) = fixture();
        let (work, _) = prepared_wait(&runtime, &grant, &request, None);
        let mut child = request.clone();
        child.run_id = work.id.clone();
        child.profile = work.profile;
        child.operator = work.operator;
        runtime
            .store
            .begin_run(&child.run_id, &grant.id, &child)
            .unwrap();
        let permit = runtime.store.permit(&child.run_id, &decode("PermitRequest", json!({"call_id":"issued", "input_tokens_bound":"10", "max_output_tokens":10, "cost_microusd_bound":"10"})).unwrap()).unwrap();
        runtime
            .store
            .dispatch_permit(&child.run_id, &permit.id)
            .unwrap();
        runtime
            .store
            .finish_run(
                "parent",
                &AgentResult {
                    disposition,
                    summary: "Stopped by the owner.".into(),
                },
            )
            .unwrap();
        let action = decode("Action", json!({"operation_id":format!("{}/late-check", child.run_id), "kind":"check", "tool":"child-check"})).unwrap();
        let receipt = runtime.execute(&child.run_id, action).unwrap();
        assert_eq!(
            receipt.status,
            EffectStatus::Denied,
            "a stopped ancestor must prevent new effects"
        );
        assert!(runtime.store.request_work(&child.run_id, &decode("WorkRequest", json!({"subject":"late-followup", "profile":"caretaker", "operator":"proofreading@1", "reason":"must not dispatch", "evidence_refs":[]})).unwrap()).is_err());
        runtime
            .store
            .usage(
                &child.run_id,
                &Usage {
                    permit_id: permit.id,
                    input_tokens: "1".into(),
                    output_tokens: "1".into(),
                    cost_microusd: "1".into(),
                    complete: true,
                },
            )
            .unwrap();
        assert_eq!(
            runtime
                .store
                .root_budget_status(&grant)
                .unwrap()
                .usage
                .unknown_calls,
            0
        );
    }
}

#[test]
fn parking_is_atomic_and_rejects_foreign_work_incomplete_exchanges_and_pending_effects() {
    let (_directory, runtime, grant, request_config, request) = fixture();
    let _ = request_config;
    let (work, park) = prepared_wait(&runtime, &grant, &request, None);
    let mut foreign = request.clone();
    foreign.run_id = "foreign".into();
    runtime
        .store
        .begin_run(&foreign.run_id, &grant.id, &foreign)
        .unwrap();
    assert!(
        runtime
            .store
            .wait_for_work(
                &foreign.run_id,
                &WorkWaitRequest {
                    work_ids: vec![work.id.clone()]
                }
            )
            .is_err()
    );
    assert!(
        runtime
            .store
            .wait_for_work(
                "parent",
                &WorkWaitRequest {
                    work_ids: vec!["parent".into()]
                }
            )
            .is_err()
    );
    let mut pending = park.clone();
    pending
        .checkpoint
        .pending_operations
        .push("parent/unsettled".into());
    assert!(runtime.store.park_run("parent", &pending).is_err());
    let action = decode(
        "Action",
        json!({"operation_id":"parent/check", "kind":"check", "tool":"child-check"}),
    )
    .unwrap();
    let receipt = runtime.execute("parent", action).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    runtime
        .store
        .db
        .execute(
            "UPDATE effects SET phase='observed' WHERE run_id='parent'",
            [],
        )
        .unwrap();
    assert!(
        runtime
            .store
            .park_run("parent", &park)
            .unwrap_err()
            .message
            .contains("issued effects must settle")
    );
    assert!(runtime.store.load_checkpoint("parent").unwrap().is_none());
    runtime.store.db.execute("UPDATE effects SET phase='finalized',body=json_set(body,'$.status','unknown') WHERE run_id='parent'", []).unwrap();
    assert!(
        runtime
            .store
            .park_run("parent", &park)
            .unwrap_err()
            .message
            .contains("issued effects must settle")
    );
    runtime
        .store
        .db
        .execute(
            "UPDATE effects SET body=?1 WHERE run_id='parent'",
            [serde_json::to_string(&receipt).unwrap()],
        )
        .unwrap();
    runtime.store.db.execute_batch("CREATE TRIGGER deny_wait BEFORE INSERT ON run_waits BEGIN SELECT RAISE(FAIL,'injected wait persistence failure'); END;").unwrap();
    assert!(runtime.store.park_run("parent", &park).is_err());
    assert!(runtime.store.load_checkpoint("parent").unwrap().is_none());
    assert_eq!(
        runtime.store.inspect_run("parent").unwrap()["status"],
        "running"
    );
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER deny_wait;")
        .unwrap();
    let context = park.checkpoint.context.as_ref().unwrap();
    runtime
        .store
        .db
        .execute(
            "DELETE FROM context_items WHERE segment_id=?1 AND sequence=5",
            [&context.segment_id],
        )
        .unwrap();
    let mut incomplete = park.clone();
    incomplete.checkpoint.context.as_mut().unwrap().count = "4".into();
    assert!(
        runtime
            .store
            .park_run("parent", &incomplete)
            .unwrap_err()
            .message
            .contains("unfinished tool exchange")
    );
    assert!(runtime.store.load_checkpoint("parent").unwrap().is_none());
}

#[test]
fn stopping_queued_children_preserves_cancellation_and_exhaustion_without_dispatch() {
    for disposition in [Disposition::Cancelled, Disposition::Exhausted] {
        let (_directory, runtime, grant, _config, request) = fixture();
        let (work, park) = prepared_wait(&runtime, &grant, &request, None);
        runtime.store.park_run("parent", &park).unwrap();
        assert!(
            runtime
                .store
                .stop_waiting_children("parent", &disposition)
                .unwrap()
                .is_empty()
        );
        let status = runtime.store.work_status("parent", &work.id).unwrap();
        let expected = if disposition == Disposition::Cancelled {
            WorkItemStatus::Cancelled
        } else {
            WorkItemStatus::Exhausted
        };
        assert_eq!(status.status, expected);
        assert!(!status.result_available);
        assert!(
            runtime
                .store
                .claim_work(&grant.id, "other-scheduler", 300000)
                .unwrap()
                .is_none()
        );
        let allocation_id: String = runtime
            .store
            .db
            .query_row(
                "SELECT allocation_id FROM work WHERE id=?1",
                [&work.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            runtime
                .store
                .allocation(&grant, &allocation_id)
                .unwrap()
                .disposition,
            Some(disposition)
        );
        let mut child = request.clone();
        child.run_id = work.id;
        child.profile = work.profile;
        child.operator = work.operator;
        assert!(
            runtime
                .store
                .begin_run(&child.run_id, &grant.id, &child)
                .is_err()
        );
        assert_eq!(
            runtime
                .store
                .root_budget_status(&grant)
                .unwrap()
                .usage
                .model_calls,
            0
        );
    }
}

#[tokio::test]
async fn cancelling_before_restoring_a_committed_wait_stops_queued_children() {
    let (directory, runtime, grant, config, request) = fixture();
    let (work, park) = prepared_wait(&runtime, &grant, &request, None);
    runtime.store.park_run("parent", &park).unwrap();
    runtime
        .store
        .finish_run(
            "parent",
            &AgentResult {
                disposition: Disposition::Interrupted,
                summary: "Saved wait.".into(),
            },
        )
        .unwrap();
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (_send, receive) = watch::channel(true);
    let result = supervisor
        .run(&config, &grant.id, request, receive)
        .await
        .unwrap();
    assert_eq!(result.disposition, Disposition::Cancelled);
    assert!(!directory.path().join("workers.jsonl").exists());
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime
            .store
            .work_status("parent", &work.id)
            .unwrap()
            .status,
        WorkItemStatus::Cancelled
    );
    assert!(runtime.store.waiting_work("parent").unwrap().is_none());
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        0
    );
}

#[test]
fn schema_seventeen_rolls_back_and_preserves_legacy_work_without_inventing_task_lineage() {
    let (directory, runtime, grant, _config, request) = fixture();
    let (work, _) = prepared_wait(&runtime, &grant, &request, None);
    let saved: String = runtime
        .store
        .db
        .query_row("SELECT body FROM work WHERE id=?1", [&work.id], |r| {
            r.get(0)
        })
        .unwrap();
    drop(runtime);
    let path = directory.path().join("state.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    // Leave a colliding column to fail after CREATE TABLE and prove rollback.
    db.execute_batch("DROP TABLE evaluation_sources; DROP TABLE protected_exposures; ALTER TABLE archive DROP COLUMN evidence; DROP TRIGGER record_index_insert; DROP TRIGGER record_index_update; DROP TRIGGER record_index_delete; DROP TABLE record_index_generation; DROP TABLE discovery_corpora; ALTER TABLE runs DROP COLUMN timings; ALTER TABLE permits DROP COLUMN dispatched_ms; ALTER TABLE permits DROP COLUMN observed_ms; DROP TABLE run_waits; PRAGMA user_version=16")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        16
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='run_waits'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("ALTER TABLE work DROP COLUMN source_ref")
        .unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row("SELECT body FROM work WHERE id=?1", [&work.id], |r| r
            .get::<_, String>(0))
            .unwrap(),
        saved
    );
    assert_eq!(
        db.query_row("SELECT source_ref FROM work WHERE id=?1", [&work.id], |r| r
            .get::<_, Option<String>>(0))
            .unwrap(),
        None
    );
    assert!(
        store
            .work_source(&grant, &work.id)
            .unwrap_err()
            .message
            .contains("legacy work has no verified task lineage")
    );
    assert!(Store::open(&path).is_ok());
    assert!(std::fs::read_dir(directory.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("before-schema-16")
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reopening_a_committed_parent_wait_runs_the_child_before_any_parent_provider_call() {
    let (directory, runtime, grant, config, request) = fixture();
    let (work, park) = prepared_wait(&runtime, &grant, &request, None);
    runtime.store.park_run("parent", &park).unwrap();
    assert!(
        runtime
            .store
            .claim_work(&grant.id, "unrelated-drain", 300000)
            .unwrap()
            .is_none()
    );
    assert!(runtime.store.require_active("parent").is_err());
    drop(runtime);
    let reopened = reopen_runtime(directory.path());
    assert_eq!(
        reopened.store.inspect_run("parent").unwrap()["status"],
        "interrupted"
    );
    let supervisor = Supervisor::new(reopened, 1).unwrap();
    let (_send, receive) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        supervisor.run(&config, &grant.id, request, receive),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    let events: Vec<Value> = std::fs::read_to_string(directory.path().join("workers.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "start")
        .collect();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0]["run_id"], work.id);
    assert_eq!(starts[1]["run_id"], "parent");
    assert_eq!(starts[1]["checkpoint"], true);
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        4
    );
    assert!(runtime.store.waiting_work("parent").unwrap().is_none());
}

#[test]
fn child_task_and_completion_authorization_follow_their_source_events() {
    let (_directory, runtime, grant, _config, request) = fixture();
    let memory=runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"parking-fixture","split":"development","limitations":[]},"body":{"kind":"episodic","content":"WITHDRAWN-TASK-MARKER","applicability":"test","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let (work, _park) = prepared_wait(&runtime, &grant, &request, Some(&memory));
    let mut child = request.clone();
    child.run_id = work.id.clone();
    child.profile = Profile::Curator;
    child.operator = "discovery@1".into();
    runtime
        .store
        .begin_run(&child.run_id, &grant.id, &child)
        .unwrap();
    let context = runtime.store.authorize_context(&child.run_id).unwrap();
    assert!(
        runtime
            .store
            .context_available(&grant, &context.segment_id)
            .unwrap()
    );
    runtime
        .store
        .finish_run(
            &child.run_id,
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "WITHDRAWN-TASK-MARKER".into(),
            },
        )
        .unwrap();
    assert!(
        runtime
            .store
            .work_status("parent", &work.id)
            .unwrap()
            .result_available
    );
    runtime
        .store
        .retire(
            &grant,
            &decode(
                "RetireRequest",
                json!({"id":memory.id,"expected_version":memory.version,"delete":true}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(runtime.store.work_source(&grant, &work.id).is_err());
    assert!(
        !runtime
            .store
            .context_available(&grant, &context.segment_id)
            .unwrap()
    );
    let status = runtime.store.work_status("parent", &work.id).unwrap();
    assert!(!status.result_available);
    assert!(status.result.is_none());
    assert!(status.source.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_waits_run_three_generations_with_one_worker_and_one_root_budget() {
    let (directory, runtime, grant, mut config, request) = fixture();
    config
        .environment
        .insert("RIBOSOME_PARK_MODE".into(), "nested".into());
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (_send, receive) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        supervisor.run(&config, &grant.id, request, receive),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    let events: Vec<Value> = std::fs::read_to_string(directory.path().join("workers.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "start")
        .collect();
    assert_eq!(starts.len(), 5);
    assert!(starts.iter().all(|event| event["previous_alive"] == false));
    assert_eq!(starts[0]["run_id"], starts[4]["run_id"]);
    assert_eq!(starts[1]["run_id"], starts[3]["run_id"]);
    assert_eq!(starts[3]["checkpoint"], true);
    assert_eq!(starts[4]["checkpoint"], true);
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    let usage = runtime.store.root_budget_status(&grant).unwrap().usage;
    assert_eq!(usage.model_calls, 10);
    assert_eq!(usage.work_items, 2);
    assert_eq!(usage.actions, 1);
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT count(*) FROM run_waits", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_parked_parent_settles_its_running_child_before_finishing() {
    let (directory, runtime, grant, config, request) = fixture();
    let runtime = slow_child_runtime(directory.path(), runtime);
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (send, receive) = watch::channel(false);
    let running = supervisor.run(&config, &grant.id, request, receive);
    tokio::pin!(running);
    let result=tokio::time::timeout(Duration::from_secs(20),async {
        let mut cancelled=false;
        loop {
            tokio::select! {
                result=&mut running=>break result.unwrap(),
                _=tokio::time::sleep(Duration::from_millis(10)),if !cancelled=>{
                    if directory.path().join("child-started").exists() {
                        let runtime=supervisor.runtime();let runtime=runtime.lock().await;
                        assert_eq!(runtime.store.inspect_run("parent").unwrap()["status"],"waiting");
                        drop(runtime); send.send(true).unwrap();cancelled=true;
                    }
                }
            }
        }
    }).await.unwrap();
    assert_eq!(result.disposition, Disposition::Cancelled);
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime.store.inspect_run("parent").unwrap()["status"],
        "cancelled"
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT status FROM work", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "cancelled"
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM effects WHERE phase!='finalized'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM runs WHERE status IN ('running','waiting')",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        3
    );
    assert!(runtime.store.waiting_work("parent").unwrap().is_none());
}

fn slow_child_runtime(directory: &std::path::Path, runtime: Runtime) -> Runtime {
    // Replace the fixture adapter before supervision. The marker coordinates
    // the test; the declared task input stays unchanged during the check.
    drop(runtime);
    let store = Store::open(directory.join("state.db")).unwrap();
    let host = LocalHost::new(
        directory,
        BTreeMap::from([(
            "child-check".into(),
            RegisteredTool {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "printf started > child-started; /bin/sleep 5; /bin/cat child-result.txt"
                        .into(),
                ],
                timeout_ms: 10000,
                reads: vec!["child-result.txt".into()],
                validates: vec!["child-result.txt".into()],
                writes: vec![],
                code_files: vec![],
                validated_properties: vec![],
            },
        )]),
    )
    .unwrap();
    Runtime::new(store, Box::new(host), directory.join("state")).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_wait_signals_a_child_already_claimed_by_another_scheduler() {
    let (directory, runtime, grant, config, request) = fixture();
    let runtime = slow_child_runtime(directory.path(), runtime);
    let (work, park) = prepared_wait(&runtime, &grant, &request, None);
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    let (stop_drain, drain_cancel) = watch::channel(false);
    let mut drain = {
        let supervisor = supervisor.clone();
        let config = config.clone();
        let grant_id = grant.id.clone();
        let provider = request.provider.clone();
        let model = request.model.clone();
        tokio::spawn(async move {
            supervisor
                .drain_work(&config, &grant_id, &provider, &model, drain_cancel)
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        while !directory.path().join("child-started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the independent scheduler did not start the child");
    {
        let runtime = supervisor.runtime();
        let runtime = runtime.lock().await;
        runtime.store.park_run("parent", &park).unwrap();
        // Re-enter through the supported interrupted-run continuation path.
        runtime
            .store
            .finish_run(
                "parent",
                &AgentResult {
                    disposition: Disposition::Interrupted,
                    summary: "Restore a committed wait.".into(),
                },
            )
            .unwrap();
    }
    let (stop_parent, parent_cancel) = watch::channel(false);
    let parent = supervisor.run(&config, &grant.id, request, parent_cancel);
    tokio::pin!(parent);
    let parent_result = tokio::select! {
        result = &mut parent => panic!("parent did not wait: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(50)) => {
            stop_parent.send(true).unwrap(); parent.await.unwrap()
        }
    };
    assert_eq!(parent_result.disposition, Disposition::Cancelled);
    let promptly = tokio::time::timeout(Duration::from_secs(2), &mut drain).await;
    if promptly.is_err() {
        // Settle the fixture before reporting a failed cancellation assertion.
        stop_drain.send(true).unwrap();
        drain.await.unwrap().unwrap();
        panic!("parent cancellation did not reach the independently owned child");
    }
    let results = promptly.unwrap().unwrap().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].disposition, Disposition::Cancelled);
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime
            .store
            .work_status("parent", &work.id)
            .unwrap()
            .status,
        WorkItemStatus::Cancelled
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM effects WHERE phase!='finalized'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
}

fn reopen_runtime(directory: &std::path::Path) -> Runtime {
    let host = LocalHost::new(
        directory,
        BTreeMap::from([(
            "child-check".into(),
            RegisteredTool {
                program: "/bin/cat".into(),
                args: vec!["child-result.txt".into()],
                timeout_ms: 5000,
                reads: vec!["child-result.txt".into()],
                validates: vec!["child-result.txt".into()],
                writes: vec![],
                code_files: vec![],
                validated_properties: vec![],
            },
        )]),
    )
    .unwrap();
    Runtime::new(
        Store::open(directory.join("state.db")).unwrap(),
        Box::new(host),
        directory.join("state"),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_child_run_survives_a_crash_before_work_completion_without_reexecution() {
    let (directory, runtime, grant, config, request) = fixture();
    let (work, park) = prepared_wait(&runtime, &grant, &request, None);
    let claimed = runtime
        .store
        .claim_work(&grant.id, "lost-host", 300000)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, work.id);
    let mut child = request.clone();
    child.run_id = work.id.clone();
    child.profile = work.profile;
    child.operator = work.operator;
    runtime
        .store
        .begin_run(&child.run_id, &grant.id, &child)
        .unwrap();
    runtime.store.authorize_context(&child.run_id).unwrap();
    let receipt = runtime.execute(&child.run_id, decode("Action", json!({"operation_id":format!("{}/check",work.id), "kind":"check", "tool":"child-check"})).unwrap()).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    runtime
        .store
        .finish_run(
            &child.run_id,
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "CHILD-RESULT".into(),
            },
        )
        .unwrap();
    runtime.store.park_run("parent", &park).unwrap();
    // Simulate loss after the durable run result, before finish_work. The
    // command really ran; the completed result must not authorize a repeat.
    drop(runtime);
    let reopened = reopen_runtime(directory.path());
    assert_eq!(
        reopened
            .store
            .work_status("parent", &work.id)
            .unwrap()
            .status,
        WorkItemStatus::Interrupted
    );
    let supervisor = Supervisor::new(reopened, 1).unwrap();
    let (_send, receive) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        supervisor.run(&config, &grant.id, request, receive),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    let events: Vec<Value> = std::fs::read_to_string(directory.path().join("workers.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "start")
        .collect();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["run_id"], "parent");
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime
            .store
            .work_status("parent", &work.id)
            .unwrap()
            .status,
        WorkItemStatus::Completed
    );
    assert_eq!(
        runtime
            .store
            .root_budget_status(&grant)
            .unwrap()
            .usage
            .model_calls,
        2
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT count(*) FROM effects", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
}
