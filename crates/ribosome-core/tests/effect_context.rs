use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::{LocalHost, hash},
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, Runtime, Grant, RecordEnvelope) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("report.txt"), "original").unwrap();
    let grant: Grant = decode("Grant", json!({"id":"owner","scope":{"client":"test","project":"effect-context"},"mode":"apply","paths":["report.txt"],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"100000","max_cost_microusd":"1000000","max_actions":10,"max_work_items":2,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"effects","visible_splits":["development"],"allow_export":false})).unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    let runtime = Runtime::new(
        store,
        Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
        directory.path().join("runtime"),
    )
    .unwrap();
    for run in ["actor", "reader"] {
        runtime.store.begin_run(run, &grant.id, &decode("AgentRunRequest", json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect the current task"})).unwrap()).unwrap();
    }
    let source = runtime.store.submit(&grant, &decode("RecordSubmission", json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"effect-context","split":"development","limitations":[]},"body":{"kind":"episodic","content":"WITHDRAWN-EFFECT-CONTENT","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let context = runtime.store.authorize_context("actor").unwrap();
    runtime.store.append_context("actor", &decode("ContextAppend", json!({"segment_id":context.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","toolName":"record_read","content":"WITHDRAWN-EFFECT-CONTENT","timestamp":1},"sources":[{"kind":"record","id":source.id,"version":source.version}]}]})).unwrap()).unwrap();
    (directory, runtime, grant, source)
}

#[test]
fn withdrawn_effect_content_is_withheld_without_losing_the_recorded_outcome() {
    let (directory, runtime, grant, source) = fixture();
    let action: Action = decode("Action", json!({"operation_id":"edit","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"WITHDRAWN-EFFECT-CONTENT"})).unwrap();
    let original = runtime.execute("actor", action).unwrap();
    assert_eq!(original.status, EffectStatus::Succeeded);
    let call = json!({"call_id":"receipt","method":"action.lookup","arguments":{"id":"edit"}});
    let copied = runtime
        .tool_call("reader", "tool.call", call.clone())
        .unwrap();
    assert!(
        copied["content"]
            .as_str()
            .unwrap()
            .contains("WITHDRAWN-EFFECT-CONTENT")
    );
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER pause_effect_cleanup BEFORE UPDATE OF body ON effects BEGIN SELECT RAISE(FAIL,'injected effect copy cleanup failure'); END").unwrap();
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    let receipt = runtime.lookup("reader", "edit").unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::ExecutionEstablished)
    );
    assert!(
        !serde_json::to_string(&receipt)
            .unwrap()
            .contains("WITHDRAWN-EFFECT-CONTENT"),
        "receipt lookup disclosed withdrawn action text"
    );
    assert!(
        !runtime
            .store
            .inspect_run("actor")
            .unwrap()
            .to_string()
            .contains("WITHDRAWN-EFFECT-CONTENT")
    );
    assert!(
        runtime.tool_call("reader", "tool.call", call).is_err(),
        "a retained receipt copy bypassed withdrawal"
    );
    let fresh = runtime
        .tool_call(
            "reader",
            "tool.call",
            json!({"call_id":"current-receipt","method":"action.lookup","arguments":{"id":"edit"}}),
        )
        .unwrap();
    let fresh: Value = serde_json::from_str(fresh["content"].as_str().unwrap()).unwrap();
    assert_eq!(fresh["status"], "succeeded");
    assert_eq!(fresh["content_available"], false);
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    db.execute_batch("DROP TRIGGER pause_effect_cleanup")
        .unwrap();
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    let (body, observation): (String, String) = db
        .query_row(
            "SELECT body,observation FROM effects WHERE id='edit'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(!body.contains("WITHDRAWN-EFFECT-CONTENT"));
    assert!(!observation.contains("WITHDRAWN-EFFECT-CONTENT"));
    assert_eq!(
        runtime.lookup("reader", "edit").unwrap().status,
        EffectStatus::Succeeded
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM effects", [], |row| row
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("report.txt")).unwrap(),
        "WITHDRAWN-EFFECT-CONTENT",
        "retirement must not replay or reverse a host effect"
    );
}

#[test]
fn delayed_finalization_keeps_dispatch_sources_after_context_rebuild() {
    let (directory, runtime, grant, source) = fixture();
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER pause_finalization BEFORE UPDATE OF phase ON effects WHEN NEW.phase='finalized' BEGIN SELECT RAISE(FAIL,'injected finalization failure'); END").unwrap();
    let action = decode("Action", json!({"operation_id":"delayed","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"WITHDRAWN-EFFECT-CONTENT"})).unwrap();
    assert!(
        runtime
            .execute("actor", action)
            .unwrap_err()
            .message
            .contains("injected finalization failure")
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    assert!(runtime.store.authorize_context("actor").unwrap().rebuilt);
    db.execute_batch("DROP TRIGGER pause_finalization").unwrap();
    let receipt = runtime.lookup("reader", "delayed").unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(receipt.content_available, Some(false));
    assert!(
        !serde_json::to_string(&receipt)
            .unwrap()
            .contains("WITHDRAWN-EFFECT-CONTENT")
    );
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    let observation: String = db
        .query_row(
            "SELECT observation FROM effects WHERE id='delayed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!observation.contains("WITHDRAWN-EFFECT-CONTENT"));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("report.txt")).unwrap(),
        "WITHDRAWN-EFFECT-CONTENT"
    );
}

#[test]
fn legacy_receipt_copies_are_unverified_and_deleted_with_scoped_source_cleanup() {
    let (directory, runtime, grant, source) = fixture();
    let action = decode("Action", json!({"operation_id":"legacy","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"WITHDRAWN-EFFECT-CONTENT"})).unwrap();
    let receipt = runtime.execute("actor", action).unwrap();
    let event = receipt.evidence_ref.unwrap();
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    db.execute_batch("UPDATE effects SET body=json_remove(body,'$.content_available','$.evidence_ref'),observation=NULL WHERE id='legacy'").unwrap();
    db.execute(
        "UPDATE events SET body=json_remove(body,'$.payload.content_available') WHERE id=?1",
        [&event],
    )
    .unwrap();
    db.execute(
        "DELETE FROM source_edges WHERE subject_kind='event' AND subject_id=?1",
        [&event],
    )
    .unwrap();
    assert_eq!(
        runtime
            .lookup("reader", "legacy")
            .unwrap()
            .content_available,
        Some(false)
    );
    assert!(
        runtime
            .store
            .evidence(
                &grant,
                &decode(
                    "EvidenceRequest",
                    json!({"cursor":"0","limit":10,"event_refs":[event]})
                )
                .unwrap()
            )
            .is_err()
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    let body: String = db
        .query_row("SELECT body FROM effects WHERE id='legacy'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!body.contains("WITHDRAWN-EFFECT-CONTENT"));
    let body: String = db
        .query_row("SELECT body FROM events WHERE id=?1", [event], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!body.contains("WITHDRAWN-EFFECT-CONTENT"));
    assert_eq!(
        runtime.lookup("reader", "legacy").unwrap().status,
        EffectStatus::Succeeded
    );
}

#[test]
fn continuation_pages_keep_old_operations_child_work_and_evidence_position() {
    let (directory, runtime, grant, _) = fixture();
    let receipt = runtime.execute("actor",decode("Action",json!({"operation_id":"first","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"finished"})).unwrap()).unwrap();
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    // Populate historical receipt metadata to exercise pagination, not host execution.
    for index in 0..105 {
        let mut saved = receipt.clone();
        saved.operation_id = format!("history-{index}");
        saved.action.operation_id = saved.operation_id.clone();
        db.execute("INSERT INTO effects(id,run_id,grant_id,body,phase) VALUES(?1,'actor',?2,?3,'finalized')",rusqlite::params![saved.operation_id,grant.id,serde_json::to_string(&saved).unwrap()]).unwrap();
    }
    let mut after = "0".to_string();
    let mut operations = Vec::new();
    loop {
        let page = runtime
            .store
            .continuation(
                "actor",
                &ContinuationRead {
                    kind: ContinuationKind::Effect,
                    after: after.clone(),
                    limit: 50,
                },
            )
            .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 32768);
        operations.extend(page.references.into_iter().map(|reference| reference.id));
        if page.complete {
            break;
        }
        assert_ne!(after, page.next);
        after = page.next;
    }
    assert_eq!(operations.len(), 106);
    assert_eq!(operations[0], "first");
    assert_eq!(operations.last().unwrap(), "history-104");
    assert!(
        runtime
            .store
            .continuation(
                "reader",
                &ContinuationRead {
                    kind: ContinuationKind::Effect,
                    after: "0".into(),
                    limit: 50
                }
            )
            .unwrap()
            .references
            .is_empty()
    );
    let child = runtime.store.request_work("actor",&decode("WorkRequest",json!({"subject":"inspect","profile":"caretaker","operator":"proofreading@1","reason":"Inspect the next output","evidence_refs":[]})).unwrap()).unwrap();
    let context = runtime.store.authorize_context("actor").unwrap();
    runtime.store.checkpoint("actor",&decode("Checkpoint",json!({"format":"pi-0.85.1/2","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","messages":[],"pending_operations":[],"event_cursor":"71","context":context})).unwrap()).unwrap();
    let page = runtime
        .store
        .continuation(
            "actor",
            &ContinuationRead {
                kind: ContinuationKind::Work,
                after: "0".into(),
                limit: 20,
            },
        )
        .unwrap();
    assert_eq!(page.references[0].id, child.id);
    assert_eq!(page.evidence_cursor, "71");
    assert!(
        runtime
            .store
            .continuation(
                "reader",
                &ContinuationRead {
                    kind: ContinuationKind::Work,
                    after: "0".into(),
                    limit: 20
                }
            )
            .unwrap()
            .references
            .is_empty()
    );
}
