use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};

fn request(run: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect work"})).unwrap()
}
fn fixture() -> (tempfile::TempDir, Store, Grant, RecordEnvelope) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.db")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"owner","scope":{"client":"client","project":"project"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":5,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"context","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    for run in ["sender", "receiver"] {
        store.begin_run(run, &grant.id, &request(run)).unwrap();
    }
    let memory=store.submit(&grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"copy","split":"development","limitations":[]},"body":{"kind":"episodic","content":"WITHDRAWN-COPY-MARKER","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let state = store.authorize_context("sender").unwrap();
    store.append_context("sender",&decode("ContextAppend",json!({"segment_id":state.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","toolName":"record_read","content":"WITHDRAWN-COPY-MARKER","timestamp":1},"sources":[{"kind":"record","id":memory.id,"version":memory.version}]}]})).unwrap()).unwrap();
    (dir, store, grant, memory)
}
fn send(store: &Store) -> Message {
    store.send_message("sender",&decode("MessageSend",json!({"recipient":"receiver","topic":"WITHDRAWN-COPY-MARKER","body":"WITHDRAWN-COPY-MARKER: derived message","correlation":"WITHDRAWN-COPY-MARKER"})).unwrap()).unwrap()
}

#[test]
fn schema_nine_preserves_unverified_copies_and_rolls_back_partial_migration() {
    let (_source_dir, _source_store, grant, memory) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    for migration in [
        include_str!("../src/store.sql"),
        include_str!("../src/migration-2.sql"),
        include_str!("../src/migration-3.sql"),
        include_str!("../src/migration-4.sql"),
        include_str!("../src/migration-5.sql"),
        include_str!("../src/migration-6.sql"),
        include_str!("../src/migration-7.sql"),
        include_str!("../src/migration-8.sql"),
    ] {
        db.execute_batch(migration).unwrap();
    }
    db.execute(
        "INSERT INTO grants(id,body) VALUES(?1,?2)",
        rusqlite::params![grant.id, serde_json::to_string(&grant).unwrap()],
    )
    .unwrap();
    let result = json!({"disposition":"interrupted","summary":"LEGACY-COPY"}).to_string();
    for run in ["sender", "receiver"] {
        db.execute("INSERT INTO runs(id,grant_id,request,status,result,started_ms) VALUES(?1,?2,?3,'interrupted',?4,'1')",rusqlite::params![run,grant.id,serde_json::to_string(&request(run)).unwrap(),result]).unwrap();
    }
    let checkpoint=json!({"format":"pi-0.85.1/1","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","messages":[{"role":"assistant","content":"LEGACY-COPY"}],"pending_operations":["sender/pending"],"event_cursor":"4"}).to_string();
    db.execute(
        "INSERT INTO checkpoints(run_id,body) VALUES('sender',?1)",
        [&checkpoint],
    )
    .unwrap();
    db.execute("INSERT INTO messages(id,grant_id,sender,recipient,body) VALUES('legacy-message',?1,'sender','receiver',?2)",rusqlite::params![grant.id,"LEGACY-COPY"]).unwrap();
    db.execute_batch("INSERT INTO context_segments(id,run_id,reason,source_format) VALUES('legacy-segment','sender','created',2)").unwrap();
    db.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,body,result_run_id,result_call_id,result_method,result_content) VALUES('legacy-result',?1,?2,?3,'ribosome-result:legacy-result','development','v1','v1','historical','{}','sender','inbox','message.inbox','LEGACY-COPY')",rusqlite::params![grant.scope.client,grant.scope.project,grant.id]).unwrap();
    db.execute("INSERT INTO records(id,client,project,kind,version,split,body) VALUES(?1,?2,?3,'memory','1','development',?4)",rusqlite::params![memory.id,grant.scope.client,grant.scope.project,serde_json::to_string(&memory).unwrap()]).unwrap();
    db.execute_batch("CREATE INDEX run_result_source ON runs(status)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('messages') WHERE name='source_format'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('runs') WHERE name='result_source'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert!(
        db.query_row(
            "SELECT available FROM artifact_snapshots WHERE id='legacy-result'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM checkpoints WHERE run_id='sender'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        checkpoint
    );
    db.execute_batch("DROP INDEX run_result_source").unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row("SELECT result FROM runs WHERE id='sender'", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        result
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM messages WHERE id='legacy-message'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "LEGACY-COPY"
    );
    assert!(
        !db.query_row(
            "SELECT available FROM artifact_snapshots WHERE id='legacy-result'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM source_edges", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "Migration must not invent evidence for old copies"
    );
    for run in ["sender", "receiver"] {
        store.begin_run(run, &grant.id, &request(run)).unwrap();
    }
    assert!(store.inbox("receiver").unwrap().messages.is_empty());
    assert!(store.inspect_run("sender").unwrap()["result"].is_null());
    assert!(
        store
            .load_checkpoint("sender")
            .unwrap()
            .unwrap()
            .messages
            .is_empty()
    );
    assert_ne!(
        store.authorize_context("sender").unwrap().segment_id,
        "legacy-segment"
    );
    store
        .retire(
            &grant,
            &RetireRequest {
                id: memory.id,
                expected_version: memory.version,
                delete: true,
            },
        )
        .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT body FROM messages WHERE id='legacy-message'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "{}"
    );
    assert!(
        db.query_row(
            "SELECT result IS NULL FROM runs WHERE id='sender'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    assert!(
        db.query_row(
            "SELECT result_content IS NULL FROM artifact_snapshots WHERE id='legacy-result'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains("before-schema-8-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 2);
    for backup in backups {
        let saved = rusqlite::Connection::open(backup).unwrap();
        assert_eq!(
            saved
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            8
        );
        assert_eq!(
            saved
                .query_row(
                    "SELECT body FROM checkpoints WHERE run_id='sender'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            checkpoint
        );
    }
}

#[test]
fn withdrawn_messages_and_run_results_stay_unavailable_during_failed_cleanup_and_reopen() {
    let (dir, store, grant, memory) = fixture();
    let message = send(&store);
    assert_eq!(store.inbox("receiver").unwrap().messages.len(), 1);
    store
        .finish_run(
            "sender",
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "WITHDRAWN-COPY-MARKER: derived completion".into(),
            },
        )
        .unwrap();
    assert!(
        store.inspect_run("sender").unwrap()["result"]["summary"]
            .as_str()
            .unwrap()
            .contains("WITHDRAWN")
    );
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_message_cleanup BEFORE UPDATE OF body ON messages BEGIN SELECT RAISE(FAIL,'injected message cleanup failure'); END").unwrap();
    store
        .retire(
            &grant,
            &RetireRequest {
                id: memory.id.clone(),
                expected_version: memory.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(
        store.inbox("receiver").unwrap().messages.is_empty(),
        "Withdrawn sender context must deny its message before physical cleanup"
    );
    assert!(
        store.inspect_run("sender").unwrap()["result"].is_null(),
        "A saved run summary must inherit the sender context"
    );
    assert_eq!(
        store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    drop(store);
    let store = Store::open(dir.path().join("state.db")).unwrap();
    assert!(store.inbox("receiver").unwrap().messages.is_empty());
    db.execute_batch("DROP TRIGGER fail_message_cleanup")
        .unwrap();
    store.cleanup_sources(&grant, 100).unwrap();
    assert_eq!(
        db.query_row("SELECT body FROM messages WHERE id=?1", [message.id], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "{}"
    );
    assert!(
        db.query_row(
            "SELECT result IS NULL FROM runs WHERE id='sender'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
}

#[test]
fn legacy_checkpoint_payloads_are_withheld_and_deleted_without_guessing_their_sources() {
    let (dir, store, grant, memory) = fixture();
    let checkpoint:Checkpoint=decode("Checkpoint",json!({"format":"pi-0.85.1/1","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","messages":[{"role":"assistant","content":"WITHDRAWN-COPY-MARKER"}],"pending_operations":["sender/uncertain-effect"],"event_cursor":"7"})).unwrap();
    store.checkpoint("sender", &checkpoint).unwrap();
    let loaded = store.load_checkpoint("sender").unwrap().unwrap();
    assert!(
        loaded.messages.is_empty(),
        "Legacy messages lack verified lineage and must not be returned to continuation callers"
    );
    assert_eq!(loaded.pending_operations, checkpoint.pending_operations);
    store
        .retire(
            &grant,
            &RetireRequest {
                id: memory.id,
                expected_version: memory.version,
                delete: true,
            },
        )
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    let body: String = db
        .query_row(
            "SELECT body FROM checkpoints WHERE run_id='sender'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let retained: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(retained["messages"], json!([]));
    assert_eq!(
        retained["pending_operations"],
        json!(["sender/uncertain-effect"])
    );
    assert!(!body.contains("WITHDRAWN"));
}

#[test]
fn message_delivery_and_completion_commit_with_their_source_evidence() {
    let (dir, store, _grant, _memory) = fixture();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_message_event BEFORE INSERT ON events WHEN NEW.producer='communication' BEGIN SELECT RAISE(FAIL,'injected message evidence failure'); END").unwrap();
    let message:MessageSend=decode("MessageSend",json!({"recipient":"receiver","topic":"test","body":"observed message","correlation":"test"})).unwrap();
    assert!(store.send_message("sender", &message).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_message_event").unwrap();
    store.send_message("sender", &message).unwrap();
    let second = store.send_message("sender", &message).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER fail_attempt BEFORE UPDATE OF attempts ON messages WHEN OLD.id='{}' BEGIN SELECT RAISE(FAIL,'injected delivery update failure'); END",second.id)).unwrap();
    assert!(store.inbox("receiver").is_err());
    assert_eq!(
        db.query_row("SELECT sum(attempts) FROM messages", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_attempt").unwrap();
    assert!(
        store
            .inbox("receiver")
            .unwrap()
            .messages
            .iter()
            .all(|message| message.attempts == 1)
    );
    let result = AgentResult {
        disposition: Disposition::Completed,
        summary: "completion".into(),
    };
    db.execute_batch("CREATE TRIGGER fail_completion BEFORE INSERT ON events WHEN NEW.producer='run-result' BEGIN SELECT RAISE(FAIL,'injected completion evidence failure'); END").unwrap();
    assert!(store.finish_run("sender", &result).is_err());
    assert_eq!(store.inspect_run("sender").unwrap()["status"], "running");
    assert!(store.inspect_run("sender").unwrap()["result"].is_null());
    db.execute_batch("DROP TRIGGER fail_completion").unwrap();
    store.finish_run("sender", &result).unwrap();
    store
        .finish_run(
            "sender",
            &AgentResult {
                disposition: Disposition::Failed,
                summary: "later unrelated callback".into(),
            },
        )
        .unwrap();
    assert_eq!(
        store.inspect_run("sender").unwrap()["result"]["summary"],
        "completion"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE producer='run-result'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn legacy_cleanup_preserves_other_clients_and_protected_copies() {
    let (dir, store, grant, memory) = fixture();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    for (run, client, protected) in [
        ("legacy", "client", false),
        ("other", "other-client", false),
        ("protected", "client", true),
    ] {
        let mut scoped = grant.clone();
        scoped.id = run.into();
        scoped.scope.client = client.into();
        if protected {
            scoped.visible_splits = vec![Split::Holdout];
        }
        store.register_grant(&scoped).unwrap();
        store.begin_run(run, &scoped.id, &request(run)).unwrap();
        let body=json!({"id":run,"sender":run,"recipient":run,"topic":"legacy","body":"LEGACY-COPY-MARKER","correlation":"legacy","sequence":"0","attempts":0}).to_string();
        db.execute(
            "INSERT INTO messages(id,grant_id,sender,recipient,body) VALUES(?1,?1,?1,?1,?2)",
            rusqlite::params![run, body],
        )
        .unwrap();
        db.execute(
            "UPDATE runs SET result=?2 WHERE id=?1",
            rusqlite::params![
                run,
                json!({"disposition":"completed","summary":"LEGACY-COPY-MARKER"}).to_string()
            ],
        )
        .unwrap();
        assert!(store.inbox(run).unwrap().messages.is_empty());
        assert!(store.inspect_run(run).unwrap()["result"].is_null());
    }
    store
        .retire(
            &grant,
            &RetireRequest {
                id: memory.id,
                expected_version: memory.version,
                delete: true,
            },
        )
        .unwrap();
    for run in ["legacy", "other", "protected"] {
        let body: String = db
            .query_row("SELECT body FROM messages WHERE id=?1", [run], |r| r.get(0))
            .unwrap();
        let result: Option<String> = db
            .query_row("SELECT result FROM runs WHERE id=?1", [run], |r| r.get(0))
            .unwrap();
        if run == "legacy" {
            assert_eq!(body, "{}");
            assert!(result.is_none());
        } else {
            assert!(body.contains("LEGACY-COPY-MARKER"));
            assert!(result.unwrap().contains("LEGACY-COPY-MARKER"));
        }
    }
}
