use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};

fn setup(path: &std::path::Path, client: &str, run: &str) -> (Store, Grant) {
    let store = Store::open(path).unwrap();
    let grant: Grant = decode("Grant",json!({"id":client,"scope":{"client":client,"project":"context"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":10,"max_work_items":10,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"context","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let request = decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Investigate the owner task"})).unwrap();
    store.begin_run(run, &grant.id, &request).unwrap();
    (store, grant)
}
fn memory(store: &Store, grant: &Grant) -> RecordEnvelope {
    store.submit(grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"context","split":"development","limitations":[]},"body":{"kind":"episodic","content":"withdrawn marker","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap()
}

#[test]
fn rebuilt_context_preserves_independent_outstanding_obligation_references() {
    let directory = tempfile::tempdir().unwrap();
    let (store, grant) = setup(&directory.path().join("state.db"), "owner", "run");
    let obligation = store.submit(&grant, &decode("RecordSubmission",json!({"kind":"obligation","provenance":{"origin":"observed","source_refs":[],"scenario_family":"context","split":"development","limitations":[]},"body":{"description":"Verify the independent output","owner":"owner","subject":"report","state":"open","affected_outputs":[],"created_ms":"1","consequence_boundary":"Before delivery","evidence_refs":[]}})).unwrap(),false).unwrap();
    let source = memory(&store, &grant);
    let state = store.authorize_context("run").unwrap();
    let state = append(
        &store,
        "run",
        &state,
        "toolResult",
        "Independent obligation",
        json!([{"kind":"record","id":obligation.id,"version":obligation.version}]),
    );
    append(
        &store,
        "run",
        &state,
        "toolResult",
        "withdrawn marker",
        json!([{"kind":"record","id":source.id,"version":source.version}]),
    );
    store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    let clean = store.authorize_context("run").unwrap();
    let content = serde_json::to_string(&read(&store, "run", &clean).unwrap()).unwrap();
    assert!(
        content.contains(&obligation.id),
        "clean continuation lost an independent outstanding obligation"
    );
    assert!(!content.contains("withdrawn marker"));
    let page = store
        .continuation(
            "run",
            &ContinuationRead {
                kind: ContinuationKind::Obligation,
                after: "0".into(),
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(page.references[0].id, obligation.id);
    assert!(!page.complete);
    store
        .retire(
            &grant,
            &RetireRequest {
                id: obligation.id,
                expected_version: obligation.version,
                delete: false,
            },
        )
        .unwrap();
    let hidden = store
        .continuation(
            "run",
            &ContinuationRead {
                kind: ContinuationKind::Obligation,
                after: "0".into(),
                limit: 1,
            },
        )
        .unwrap();
    assert!(hidden.references.is_empty());
    assert_eq!(
        hidden.next, page.next,
        "an unavailable reference must still advance pagination"
    );
    let last = store
        .continuation(
            "run",
            &ContinuationRead {
                kind: ContinuationKind::Obligation,
                after: hidden.next,
                limit: 1,
            },
        )
        .unwrap();
    assert!(last.complete);
}
fn append(
    store: &Store,
    run: &str,
    state: &ContextState,
    role: &str,
    text: &str,
    sources: Value,
) -> ContextState {
    store.append_context(run,&decode("ContextAppend",json!({"segment_id":state.segment_id,"after":state.count,"entries":[{"message":{"role":role,"content":text,"timestamp":1},"sources":sources}]})).unwrap()).unwrap()
}
fn read(
    store: &Store,
    run: &str,
    state: &ContextState,
) -> ribosome_core::error::Result<ContextPage> {
    store.read_context(
        run,
        &ContextRead {
            segment_id: state.segment_id.clone(),
            after: "0".into(),
            limit: 20,
        },
    )
}

#[test]
fn r1_05_09_10_context_inherits_ancestry_and_never_accepts_author_supplied_independence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let (store, grant) = setup(&path, "owner", "run");
    let source = memory(&store, &grant);
    let initial = store.authorize_context("run").unwrap();
    let state = append(
        &store,
        "run",
        &initial,
        "toolResult",
        "withdrawn marker",
        json!([{"kind":"record","id":source.id,"version":"1"}]),
    );
    let state = append(&store, "run", &state, "assistant", "summary one", json!([]));
    let state = append(
        &store,
        "run",
        &state,
        "assistant",
        "summary of summary",
        json!([]),
    );
    assert_eq!(read(&store, "run", &state).unwrap().messages.len(), 3);
    let (other, other_grant) = setup(&path, "other", "other-run");
    assert!(read(&other, "other-run", &state).is_err());
    let other_state = other.authorize_context("other-run").unwrap();
    let other_state = append(
        &other,
        "other-run",
        &other_state,
        "assistant",
        "cross-client copy",
        json!([{"kind":"record","id":source.id,"version":"1"}]),
    );
    assert!(read(&other, "other-run", &other_state).is_err());
    assert!(other.authorize_context("other-run").unwrap().rebuilt);
    assert!(
        other.source_cleanup_status(&other_grant).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: "1".into(),
                delete: false,
            },
        )
        .unwrap();
    assert!(read(&store, "run", &state).is_err());
    drop(store);
    let store = Store::open(&path).unwrap();
    let clean = store.authorize_context("run").unwrap();
    assert!(clean.rebuilt);
    assert_ne!(clean.segment_id, state.segment_id);
    let content = serde_json::to_string(&read(&store, "run", &clean).unwrap()).unwrap();
    assert!(content.contains("Investigate the owner task"));
    assert!(!content.contains("withdrawn marker"));
    assert!(!content.contains("summary one"));
    assert!(
        store
            .append_context(
                "run",
                &decode(
                    "ContextAppend",
                    json!({"segment_id":state.segment_id,"after":state.count,"entries":[]})
                )
                .unwrap()
            )
            .is_err()
    );
}

#[test]
fn r1_04_deleted_context_is_inaccessible_during_failed_cleanup_and_redacted_on_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let (store, grant) = setup(&path, "owner", "run");
    let source = memory(&store, &grant);
    let state = store.authorize_context("run").unwrap();
    let state = append(
        &store,
        "run",
        &state,
        "toolResult",
        "withdrawn marker",
        json!([{"kind":"record","id":source.id,"version":"1"}]),
    );
    let state = append(
        &store,
        "run",
        &state,
        "assistant",
        "dependent conclusion",
        json!([]),
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_context_delete BEFORE UPDATE OF body ON context_items BEGIN SELECT RAISE(FAIL,'injected context cleanup failure'); END;").unwrap();
    store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: "1".into(),
                delete: true,
            },
        )
        .unwrap();
    assert!(read(&store, "run", &state).is_err());
    assert_eq!(
        store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    db.execute_batch("DROP TRIGGER fail_context_delete")
        .unwrap();
    store.cleanup_sources(&grant, 100).unwrap();
    let copies: i64 = db
        .query_row(
            "SELECT count(*) FROM context_items WHERE body IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(copies, 0);
    assert_eq!(
        store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "complete"
    );
}

#[test]
fn r1_08_legacy_checkpoint_rebuilds_without_inventing_lineage() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _) = setup(&directory.path().join("state.db"), "owner", "run");
    store.checkpoint("run",&decode("Checkpoint",json!({"format":"pi-0.85.1/1","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","messages":[{"role":"assistant","content":"unattributed legacy text"}],"pending_operations":[],"event_cursor":"0"})).unwrap()).unwrap();
    let state = store.authorize_context("run").unwrap();
    assert!(state.rebuilt);
    let text = serde_json::to_string(&read(&store, "run", &state).unwrap()).unwrap();
    assert!(text.contains("legacy continuation lacks verified lineage"));
    assert!(!text.contains("unattributed legacy text"));
}

#[test]
fn context_append_is_atomic_idempotent_and_preserves_exact_source_versions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let (store, grant) = setup(&path, "owner", "run");
    let source = memory(&store, &grant);
    let initial = store.authorize_context("run").unwrap();
    let first = append(
        &store,
        "run",
        &initial,
        "toolResult",
        "observation",
        json!([{"kind":"record","id":source.id,"version":"1"}]),
    );
    let retry = append(
        &store,
        "run",
        &initial,
        "toolResult",
        "observation",
        json!([]),
    );
    assert_eq!(first.count, retry.count);
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT version FROM context_sources", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "1"
    );
    let broken: ContextAppend=decode("ContextAppend",json!({"segment_id":first.segment_id,"after":"1","entries":[{"message":{"role":"assistant","content":"not committed","timestamp":1},"sources":[]},{"message":{"role":"invalid","content":"invalid","timestamp":1},"sources":[]}]})).unwrap();
    assert!(store.append_context("run", &broken).is_err());
    assert_eq!(read(&store, "run", &first).unwrap().messages.len(), 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn r1_10_a_model_authored_memory_cannot_drop_sources_from_its_producing_context() {
    let directory = tempfile::tempdir().unwrap();
    let (store, grant) = setup(&directory.path().join("state.db"), "owner", "run");
    let source = memory(&store, &grant);
    let state = store.authorize_context("run").unwrap();
    append(
        &store,
        "run",
        &state,
        "toolResult",
        "withdrawn marker",
        json!([{"kind":"record","id":source.id,"version":"1"}]),
    );
    let host =
        ribosome_core::host::LocalHost::new(directory.path(), std::collections::BTreeMap::new())
            .unwrap();
    let runtime = ribosome_core::effects::Runtime::new(
        store,
        Box::new(host),
        directory.path().join("runtime"),
    )
    .unwrap();
    runtime.store.begin_run("run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"run","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Investigate the owner task"})).unwrap()).unwrap();
    let saved = runtime.tool_call("run","record.submit",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"context","split":"development","limitations":[]},"body":{"kind":"episodic","content":"summary copied from withdrawn marker","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap();
    let db = rusqlite::Connection::open(directory.path().join("state.db")).unwrap();
    let anchor: String = db
        .query_row(
            "SELECT id FROM context_sources WHERE segment_id=?1 AND kind='event'",
            [&state.segment_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut expected = vec![source.id.clone(), anchor];
    expected.sort();
    assert_eq!(saved["provenance"]["source_refs"], json!(expected));
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: "1".into(),
                delete: false,
            },
        )
        .unwrap();
    assert!(
        runtime
            .store
            .record(&grant, saved["id"].as_str().unwrap())
            .is_err()
    );
}

#[test]
fn r1_08_context_migration_rolls_back_and_newer_schemas_are_not_modified() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema-three.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    db.execute_batch(include_str!("../src/migration-2.sql"))
        .unwrap();
    db.execute_batch(include_str!("../src/migration-3.sql"))
        .unwrap();
    db.execute_batch("CREATE TABLE context_items(collision TEXT)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='context_segments'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TABLE context_items").unwrap();
    assert!(Store::open(&path).is_ok());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert!(Store::open(&path).is_ok());
    db.execute_batch("PRAGMA user_version=22").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        22
    );
}
