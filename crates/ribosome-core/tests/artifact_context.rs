use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn request(run: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect current work"})).unwrap()
}
fn fixture() -> (tempfile::TempDir, Runtime, Grant) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("report.txt"),
        "PREVIOUS-ARTIFACT-CONTENT",
    )
    .unwrap();
    let grant: Grant=decode("Grant",json!({"id":"owner","scope":{"client":"client","project":"project"},"mode":"observe","paths":["report.txt"],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":5,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"context","visible_splits":["development"],"allow_export":false})).unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(directory.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), directory.path().join("runtime")).unwrap();
    runtime
        .store
        .begin_run("run", &grant.id, &request("run"))
        .unwrap();
    (directory, runtime, grant)
}
fn read(runtime: &Runtime, extra: Value) -> ribosome_core::error::Result<Value> {
    let mut params = json!({"path":"report.txt","offset":0,"length":1000});
    params
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    runtime.tool_call("run", "artifact.read", params)
}

#[test]
fn r1_03_historical_snapshot_cannot_satisfy_current_read_after_revision() {
    let (directory, runtime, _) = fixture();
    let original = read(&runtime, json!({"length":8})).unwrap();
    assert_eq!(original["required_freshness"], "current");
    std::fs::write(
        directory.path().join("report.txt"),
        "CURRENT-ARTIFACT-CONTENT",
    )
    .unwrap();
    let stale = read(&runtime, json!({"snapshot_id":original["snapshot_id"]})).unwrap_err();
    assert_eq!(stale.code, -32002);
    assert!(stale.message.contains("historical"));
    let historical = read(
        &runtime,
        json!({"snapshot_id":original["snapshot_id"],"required_freshness":"historical"}),
    )
    .unwrap();
    assert_eq!(historical["content"], "PREVIOUS");
    assert_eq!(historical["required_freshness"], "historical");
    assert_eq!(
        historical["artifact"]["version"],
        original["artifact"]["version"]
    );
    assert!(!historical["eof"].as_bool().unwrap());
    assert!(
        read(
            &runtime,
            json!({"snapshot_id":original["snapshot_id"],"required_freshness":"historical","offset":8})
        )
        .is_err()
    );
    let current = read(&runtime, json!({})).unwrap();
    assert_eq!(current["content"], "CURRENT-ARTIFACT-CONTENT");
    assert_ne!(
        current["artifact"]["version"],
        original["artifact"]["version"]
    );
}

#[test]
fn r1_04_deleted_artifact_denies_copies_while_cleanup_fails_and_retry_redacts_them() {
    let (directory, runtime, grant) = fixture();
    let original = read(&runtime, json!({})).unwrap();
    let state = runtime.store.authorize_context("run").unwrap();
    runtime.tool_call("run","session.context.append",json!({"segment_id":state.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","content":"PREVIOUS-ARTIFACT-CONTENT","timestamp":1},"sources":[{"kind":"artifact","id":original["snapshot_id"],"version":original["artifact"]["version"]}]}]})).unwrap();
    let derived=runtime.tool_call("run","record.submit",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[],"scenario_family":"artifact","split":"development","limitations":[]},"body":{"subject":"report.txt","observation":"PREVIOUS-ARTIFACT-CONTENT","interpretation":"copied interpretation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap();
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_context_delete BEFORE UPDATE OF body ON context_items BEGIN SELECT RAISE(FAIL,'injected context cleanup failure'); END").unwrap();
    std::fs::remove_file(directory.path().join("report.txt")).unwrap();
    assert!(
        read(
            &runtime,
            json!({"snapshot_id":original["snapshot_id"],"required_freshness":"historical"})
        )
        .is_err()
    );
    assert!(
        runtime
            .tool_call("run", "record.read", json!({"id":derived["id"]}))
            .is_err()
    );
    assert!(
        runtime
            .store
            .read_context(
                "run",
                &ContextRead {
                    segment_id: state.segment_id.clone(),
                    after: "0".into(),
                    limit: 20
                }
            )
            .is_err()
    );
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    db.execute_batch("DROP TRIGGER fail_context_delete")
        .unwrap();
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT body FROM artifact_snapshots WHERE id=?1",
            [original["snapshot_id"].as_str().unwrap()],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "{}"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM context_items WHERE body IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    std::fs::write(directory.path().join("report.txt"), "replacement file").unwrap();
    assert!(
        read(
            &runtime,
            json!({"snapshot_id":original["snapshot_id"],"required_freshness":"historical"})
        )
        .is_err(),
        "Recreating the path must not revive deleted snapshots"
    );
    assert_eq!(
        read(&runtime, json!({})).unwrap()["content"],
        "replacement file"
    );
}

#[test]
fn r1_09_snapshots_cannot_escape_client_scope_or_path_permissions() {
    let (_directory, runtime, grant) = fixture();
    let original = read(&runtime, json!({})).unwrap();
    let mut other = grant.clone();
    other.id = "other".into();
    other.scope.client = "other".into();
    runtime.store.register_grant(&other).unwrap();
    runtime
        .store
        .begin_run("other-run", &other.id, &request("other-run"))
        .unwrap();
    assert!(runtime.tool_call("other-run","artifact.read",json!({"path":"report.txt","offset":0,"length":1000,"snapshot_id":original["snapshot_id"],"required_freshness":"historical"})).is_err());
    let mut narrow = grant.clone();
    narrow.id = "narrow".into();
    narrow.paths.clear();
    runtime.store.register_grant(&narrow).unwrap();
    runtime
        .store
        .begin_run("narrow-run", &narrow.id, &request("narrow-run"))
        .unwrap();
    assert!(runtime.tool_call("narrow-run","artifact.read",json!({"path":"report.txt","offset":0,"length":1000,"snapshot_id":original["snapshot_id"],"required_freshness":"historical"})).is_err());
    assert!(
        read(
            &runtime,
            json!({"snapshot_id":original["snapshot_id"],"required_freshness":"historical"})
        )
        .is_ok()
    );
}

#[test]
fn historical_windows_follow_the_live_read_utf8_boundary_rule() {
    let (directory, runtime, _) = fixture();
    std::fs::write(directory.path().join("report.txt"), "abéx").unwrap();
    let snapshot = read(&runtime, json!({})).unwrap();
    let live = read(&runtime, json!({"length":3})).unwrap();
    let historical = read(
        &runtime,
        json!({"snapshot_id":snapshot["snapshot_id"],"required_freshness":"historical","length":3}),
    )
    .unwrap();
    assert_eq!(live["content"], "ab");
    assert_eq!(historical["content"], live["content"]);
    assert!(read(&runtime,json!({"snapshot_id":snapshot["snapshot_id"],"required_freshness":"historical","offset":3})).is_err());
}

#[test]
fn r1_08_pre_artifact_context_migration_rebuilds_without_inventing_observations() {
    let (directory, _runtime, grant) = fixture();
    let path = directory.path().join("schema-four.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    db.execute_batch(include_str!("../src/migration-2.sql"))
        .unwrap();
    db.execute_batch(include_str!("../src/migration-3.sql"))
        .unwrap();
    db.execute_batch(include_str!("../src/migration-4.sql"))
        .unwrap();
    db.execute(
        "INSERT INTO grants(id,body) VALUES(?1,?2)",
        rusqlite::params![grant.id, serde_json::to_string(&grant).unwrap()],
    )
    .unwrap();
    db.execute("INSERT INTO runs(id,grant_id,request,status,started_ms) VALUES('legacy-run',?1,?2,'running','1')",rusqlite::params![grant.id,serde_json::to_string(&request("legacy-run")).unwrap()]).unwrap();
    db.execute("INSERT INTO context_segments(id,run_id,reason) VALUES('legacy-segment','legacy-run','initial context')",[]).unwrap();
    db.execute("INSERT INTO context_items(segment_id,sequence,kind,body) VALUES('legacy-segment',1,'observed_evidence',?1)",[json!({"role":"toolResult","toolName":"artifact_read","content":"UNATTRIBUTED-OLD-ARTIFACT","timestamp":1}).to_string()]).unwrap();
    let store = Store::open(&path).unwrap();
    let state = store.authorize_context("legacy-run").unwrap();
    assert!(
        state.rebuilt,
        "An old segment lacking artifact lineage must rebuild"
    );
    let retained: String = db
        .query_row(
            "SELECT body FROM context_items WHERE segment_id='legacy-segment'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(retained.contains("UNATTRIBUTED-OLD-ARTIFACT"));
    assert_eq!(
        db.query_row("SELECT count(*) FROM artifact_snapshots", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        store
            .read_context(
                "legacy-run",
                &ContextRead {
                    segment_id: "legacy-segment".into(),
                    after: "0".into(),
                    limit: 20
                }
            )
            .is_err()
    );
    let clean = store
        .read_context(
            "legacy-run",
            &ContextRead {
                segment_id: state.segment_id,
                after: "0".into(),
                limit: 20,
            },
        )
        .unwrap();
    assert!(
        !serde_json::to_string(&clean)
            .unwrap()
            .contains("UNATTRIBUTED-OLD-ARTIFACT")
    );
}
