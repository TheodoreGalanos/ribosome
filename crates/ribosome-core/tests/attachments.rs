use ribosome_core::{
    attachments::AttachmentPolicy,
    contracts::*,
    effects::Runtime,
    host::{LocalHost, hash},
    store::Store,
    validation::{decode, now_ms},
};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn grant() -> Grant {
    decode("Grant",json!({"id":"owner","scope":{"client":"one","project":"project"},"mode":"apply","paths":["report.txt"],"tools":["check"],"profiles":["caretaker"],"budget":{"max_calls":20,"max_tokens":"100000","max_cost_microusd":"1000000","max_actions":20,"max_work_items":20,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"attached","visible_splits":["development"],"allow_export":false,"required_checks":["check"]})).unwrap()
}
fn request(id: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":id,"profile":"caretaker","operator":"proofreading@1","prompt":"Inspect the report","provider":"openai","model":"test-model"})).unwrap()
}
fn open(id: &str) -> AttachmentOpen {
    decode("AttachmentOpen",json!({"id":id,"execution_id":format!("source-{id}"),"connector":"custom","connector_version":"1","start":"now","capabilities":["observe"]})).unwrap()
}
fn event(sequence: u32) -> HarnessEvent {
    decode("HarnessEvent",json!({"id":format!("event-{sequence}"),"producer":"source","sequence":sequence.to_string(),"kind":"tool.completed","timestamp_ms":"1","parents":[],"correlation":"task","artifacts":[{"path":"report.txt","version":hash(b"original")}],"payload":{"sequence":sequence}})).unwrap()
}
fn fixture() -> (tempfile::TempDir, Runtime, Grant, AttachmentPolicy) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("report.txt"), "original").unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant = grant();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(directory.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), directory.path().join("state")).unwrap();
    let policy = AttachmentPolicy {
        batch_size: 1,
        ..Default::default()
    };
    (directory, runtime, grant, policy)
}
fn append(
    store: &Store,
    id: &str,
    events: Vec<HarnessEvent>,
    policy: &AttachmentPolicy,
) -> ribosome_core::error::Result<IngestionReceipt> {
    store.append_attachment_events(
        &AttachmentEvents {
            attachment_id: id.into(),
            events,
        },
        policy,
    )
}

#[test]
fn schema_one_migration_retains_existing_grants_and_runs() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    let g = grant();
    let r = request("old-run");
    db.execute(
        "INSERT INTO grants(id,body) VALUES (?1,?2)",
        params![g.id, serde_json::to_string(&g).unwrap()],
    )
    .unwrap();
    db.execute("INSERT INTO runs(id,grant_id,request,status,started_ms) VALUES ('old-run',?1,?2,'running','1')",params![g.id,serde_json::to_string(&r).unwrap()]).unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.grant(&g.id).unwrap(), g);
    assert_eq!(store.inspect_run("old-run").unwrap()["status"], "running");
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    assert!(Store::open(&path).is_ok());
}

#[test]
fn failed_migration_rolls_back_new_columns_and_preserves_the_old_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    db.execute_batch("CREATE TABLE attachments(existing TEXT)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('subscriptions') WHERE name='source_run'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn ingestion_is_idempotent_atomic_and_reports_source_gaps() {
    let (_d, r, g, p) = fixture();
    r.store
        .open_attachment(&g, &open("a"), &p, &request("template"))
        .unwrap();
    assert_eq!(
        r.store.attachment("a").unwrap().source_status,
        AttachmentSourceStatus::Unknown
    );
    let first = append(&r.store, "a", vec![event(1)], &p).unwrap();
    assert_eq!(first.inserted, 1);
    assert_eq!(
        append(&r.store, "a", vec![event(1)], &p)
            .unwrap()
            .duplicates,
        1
    );
    let mut bad = event(1);
    bad.payload.insert("changed".into(), json!(true));
    assert!(append(&r.store, "a", vec![event(2), bad], &p).is_err());
    let a = r.store.attachment("a").unwrap();
    assert_eq!(a.frontier["source"], "1");
    let events = r
        .store
        .evidence(
            &g,
            &EvidenceRequest {
                event_refs: None,
                neighbors: None,
                kind: None,
                artifact: None,
                query: None,
                through_cursor: None,
                cursor: "0".into(),
                limit: 100,
                run_id: Some(a.execution_id.clone()),
            },
        )
        .unwrap();
    assert_eq!(events.events.len(), 1);
    append(&r.store, "a", vec![event(3)], &p).unwrap();
    assert_eq!(r.store.attachment("a").unwrap().coverage.len(), 2);
}

#[test]
fn routing_and_cursor_advancement_commit_together() {
    let (d, r, g, p) = fixture();
    r.store
        .open_attachment(&g, &open("a"), &p, &request("template"))
        .unwrap();
    append(&r.store, "a", vec![event(1)], &p).unwrap();
    let db = rusqlite::Connection::open(d.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_cursor BEFORE UPDATE OF cursor ON subscriptions BEGIN SELECT RAISE(ABORT,'injected persistence failure'); END;").unwrap();
    assert!(r.store.poll_subscription("a").is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM work", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(r.store.attachment("a").unwrap().cursor, "0");
    db.execute_batch("DROP TRIGGER reject_cursor").unwrap();
    assert!(r.store.poll_subscription("a").unwrap().is_some());
    assert!(r.store.poll_subscription("a").unwrap().is_none());
    assert_eq!(
        db.query_row("SELECT count(*) FROM attachment_work", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn maintenance_evidence_cannot_escape_its_attached_execution() {
    let (_d, r, g, p) = fixture();
    for id in ["a", "b"] {
        r.store
            .open_attachment(&g, &open(id), &p, &request("template"))
            .unwrap();
        append(&r.store, id, vec![event(1)], &p).unwrap();
    }
    let work = r.store.poll_subscription("a").unwrap().unwrap();
    r.store
        .begin_run(&work.id, &g.id, &request(&work.id))
        .unwrap();
    let page = r
        .tool_call(&work.id, "evidence.read", json!({"cursor":"0","limit":100}))
        .unwrap();
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["run_id"], "source-a");
    assert!(
        r.tool_call(
            &work.id,
            "evidence.read",
            json!({"cursor":"0","limit":100,"run_id":"source-b"})
        )
        .is_err()
    );
    assert!(
        r.tool_call(
            &work.id,
            "host.hello",
            json!({"protocol":"ribosome-host/1"})
        )
        .is_err()
    );
}

#[test]
fn ungranted_capabilities_and_live_writes_are_denied() {
    let (_d, r, g, p) = fixture();
    let mut forged = open("a");
    forged.capabilities.push(AttachmentCapability::Steer);
    assert!(
        r.store
            .open_attachment(&g, &forged, &p, &request("template"))
            .is_err()
    );
    r.store
        .open_attachment(&g, &open("a"), &p, &request("template"))
        .unwrap();
    append(&r.store, "a", vec![event(1)], &p).unwrap();
    let work = r.store.poll_subscription("a").unwrap().unwrap();
    r.store
        .begin_run(&work.id, &g.id, &request(&work.id))
        .unwrap();
    let action:Action=decode("Action",json!({"operation_id":"ungranted","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"bad"})).unwrap();
    assert_eq!(
        r.execute(&work.id, action).unwrap().status,
        EffectStatus::Denied
    );
    assert!(
        r.store
            .begin_attachment_repair("a", &request("template"))
            .is_err()
    );
}

#[test]
fn writer_generations_are_fenced_and_unstarted_repairs_can_be_released() {
    let (_d, r, g, mut p) = fixture();
    p.allow_coordinated_writes = true;
    let mut input = open("a");
    input
        .capabilities
        .push(AttachmentCapability::CoordinatedWrite);
    r.store
        .open_attachment(&g, &input, &p, &request("template"))
        .unwrap();
    append(&r.store, "a", vec![event(1)], &p).unwrap();
    let first = r
        .store
        .begin_attachment_repair("a", &request("template"))
        .unwrap();
    assert!(r.store.detach_attachment("a").is_err());
    r.release_attachment_repair(&AttachmentRelease {
        attachment_id: "a".into(),
        generation: first.generation.clone(),
    })
    .unwrap();
    let second = r
        .store
        .begin_attachment_repair("a", &request("template"))
        .unwrap();
    assert_ne!(first.generation, second.generation);
    assert!(
        r.release_attachment_repair(&AttachmentRelease {
            attachment_id: "a".into(),
            generation: first.generation
        })
        .is_err()
    );
    r.release_attachment_repair(&AttachmentRelease {
        attachment_id: "a".into(),
        generation: second.generation,
    })
    .unwrap();
    r.store.detach_attachment("a").unwrap();
}

#[test]
fn an_expired_handoff_cannot_apply_a_live_write() {
    let (d, r, g, mut p) = fixture();
    p.allow_coordinated_writes = true;
    let mut input = open("a");
    input
        .capabilities
        .push(AttachmentCapability::CoordinatedWrite);
    r.store
        .open_attachment(&g, &input, &p, &request("template"))
        .unwrap();
    append(&r.store, "a", vec![event(1)], &p).unwrap();
    let handoff = r
        .store
        .begin_attachment_repair("a", &request("template"))
        .unwrap();
    r.store
        .begin_run(&handoff.work_id, &g.id, &request(&handoff.work_id))
        .unwrap();
    let db = rusqlite::Connection::open(d.path().join("state.db")).unwrap();
    db.execute(
        "UPDATE attachments SET body=json_set(body,'$.handoff.deadline_ms','0') WHERE id='a'",
        [],
    )
    .unwrap();
    let action:Action=decode("Action",json!({"operation_id":"late","kind":"edit","path":"report.txt","expected_version":hash(b"original"),"content":"bad"})).unwrap();
    let receipt = r.execute(&handoff.work_id, action).unwrap();
    assert_eq!(receipt.status, EffectStatus::Denied);
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "original"
    );
}

#[test]
fn feedback_steering_intent_remains_unknown_until_explicit_acknowledgement() {
    let (d, r, g, mut p) = fixture();
    p.allow_steering = true;
    let mut input = open("a");
    input.capabilities.push(AttachmentCapability::Steer);
    r.store
        .open_attachment(&g, &input, &p, &request("template"))
        .unwrap();
    r.store
        .begin_run("maintenance", &g.id, &request("maintenance"))
        .unwrap();
    r.store
        .finish_run(
            "maintenance",
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "advice".into(),
            },
        )
        .unwrap();
    let feedback:AttachmentFeedback=decode("AttachmentFeedback",json!({"id":"feedback","attachment_id":"a","run_id":"maintenance","kind":"finding","state":"pending","summary":"advice","disposition":"completed","record_refs":[],"evidence_refs":[],"artifact_versions":[],"expires_ms":g.budget.deadline_ms,"attempts":0,"detail":""})).unwrap();
    let db = rusqlite::Connection::open(d.path().join("state.db")).unwrap();
    db.execute(
        "INSERT INTO attachment_feedback(id,attachment_id,body) VALUES ('feedback','a',?1)",
        [serde_json::to_string(&feedback).unwrap()],
    )
    .unwrap();
    assert_eq!(r.attachment_feedback("a").unwrap().items.len(), 1);
    r.begin_attachment_steering(&FeedbackRequest {
        attachment_id: "a".into(),
        feedback_id: "feedback".into(),
    })
    .unwrap();
    assert!(r.attachment_feedback("a").unwrap().items.is_empty());
    let body: String = db
        .query_row(
            "SELECT body FROM attachment_feedback WHERE id='feedback'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["state"],
        "unknown"
    );
    r.store
        .acknowledge_feedback(&FeedbackAcknowledgement {
            attachment_id: "a".into(),
            feedback_id: "feedback".into(),
            outcome: FeedbackAcknowledgementOutcome::Acknowledged,
            detail: "Host queued the steering message".into(),
        })
        .unwrap();
    assert!(
        r.begin_attachment_steering(&FeedbackRequest {
            attachment_id: "a".into(),
            feedback_id: "feedback".into()
        })
        .is_err()
    );
}

#[test]
fn backup_and_index_rebuild_preserve_scope_and_retirement() {
    let (d, r, g, p) = fixture();
    r.store
        .open_attachment(&g, &open("a"), &p, &request("template"))
        .unwrap();
    append(&r.store, "a", vec![event(1)], &p).unwrap();
    let record = |text: &str| -> RecordSubmission {
        decode("RecordSubmission",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[],"scenario_family":"backup-test","split":"development","limitations":[]},"body":{"subject":"report","observation":text,"interpretation":"Inspect report","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap()
    };
    let visible = r
        .store
        .submit(&g, &record("visible finding"), false)
        .unwrap();
    let retired = r
        .store
        .submit(&g, &record("withdrawn finding"), false)
        .unwrap();
    r.store
        .retire(
            &g,
            &RetireRequest {
                id: retired.id.clone(),
                expected_version: retired.version,
                delete: true,
            },
        )
        .unwrap();
    let mut other = g.clone();
    other.id = "other-grant".into();
    other.scope.client = "other".into();
    r.store.register_grant(&other).unwrap();
    let foreign = r
        .store
        .submit(&other, &record("foreign finding"), false)
        .unwrap();
    let db = rusqlite::Connection::open(d.path().join("state.db")).unwrap();
    let backup = d.path().join("backup.db");
    db.execute("VACUUM INTO ?1", [backup.to_str().unwrap()])
        .unwrap();
    let restored = Store::open(&backup).unwrap();
    let restored_db = rusqlite::Connection::open(&backup).unwrap();
    restored_db
        .execute("DELETE FROM record_search", [])
        .unwrap();
    restored_db
        .execute_batch(include_str!("../../../scripts/rebuild-index.sql"))
        .unwrap();
    let query: SearchRequest = decode(
        "SearchRequest",
        json!({"query":"finding","inventory":"evidence","limit":100,"offset":0}),
    )
    .unwrap();
    let results = restored.search(&g, &query).unwrap();
    assert_eq!(results.records.len(), 1);
    assert_eq!(results.records[0].id, visible.id);
    assert!(restored.record(&g, &foreign.id).is_err());
    assert!(restored.record(&g, &retired.id).is_err());
    assert_eq!(restored.attachment("a").unwrap().frontier["source"], "1");
    assert_eq!(
        restored_db
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn feedback_withholds_inherited_memory_and_deletion_removes_its_stored_copy() {
    for delete in [false, true] {
        let (directory, runtime, grant, policy) = fixture();
        runtime
            .store
            .open_attachment(&grant, &open("a"), &policy, &request("template"))
            .unwrap();
        runtime
            .store
            .begin_run("maintenance", &grant.id, &request("maintenance"))
            .unwrap();
        let memory = runtime.store.submit(&grant, &decode("RecordSubmission", json!({
            "kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"feedback","split":"development","limitations":[]},
            "body":{"kind":"episodic","content":"WITHDRAWN-FEEDBACK-MEMORY","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}
        })).unwrap(), false).unwrap();
        let context = runtime.store.authorize_context("maintenance").unwrap();
        runtime.store.append_context("maintenance", &decode("ContextAppend", json!({
            "segment_id":context.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","toolName":"record_read","content":"WITHDRAWN-FEEDBACK-MEMORY","timestamp":1},"sources":[{"kind":"record","id":memory.id,"version":memory.version}]}]
        })).unwrap()).unwrap();
        runtime
            .store
            .finish_run(
                "maintenance",
                &AgentResult {
                    disposition: Disposition::Completed,
                    summary: "WITHDRAWN-FEEDBACK-MEMORY: derived advice".into(),
                },
            )
            .unwrap();
        // Persisted delivery fixture: no explicit reference to the memory. The
        // summary must inherit its run's observed sources independently.
        let feedback: AttachmentFeedback = decode("AttachmentFeedback", json!({"id":"feedback","attachment_id":"a","run_id":"maintenance","kind":"finding","state":"pending","summary":"WITHDRAWN-FEEDBACK-MEMORY: derived advice","disposition":"completed","record_refs":[],"evidence_refs":[],"artifact_versions":[],"expires_ms":grant.budget.deadline_ms,"attempts":0,"detail":""})).unwrap();
        let path = directory.path().join("state.db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO attachment_feedback(id,attachment_id,body) VALUES ('feedback','a',?1)",
            [serde_json::to_string(&feedback).unwrap()],
        )
        .unwrap();
        assert_eq!(runtime.attachment_feedback("a").unwrap().items.len(), 1);
        runtime
            .store
            .retire(
                &grant,
                &RetireRequest {
                    id: memory.id,
                    expected_version: memory.version,
                    delete,
                },
            )
            .unwrap();
        assert!(
            runtime.attachment_feedback("a").unwrap().items.is_empty(),
            "withdrawn run summary was redelivered"
        );
        if delete {
            let body: String = db
                .query_row(
                    "SELECT body FROM attachment_feedback WHERE id='feedback'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                !body.contains("WITHDRAWN-FEEDBACK-MEMORY"),
                "cleanup left a copied summary behind"
            );
        }
        drop(runtime);
        let reopened = Runtime::new(
            Store::open(path).unwrap(),
            Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
            directory.path().join("state"),
        )
        .unwrap();
        assert!(reopened.attachment_feedback("a").unwrap().items.is_empty());
    }
}
