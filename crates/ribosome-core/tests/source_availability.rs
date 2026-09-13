use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::json;

fn grant() -> Grant {
    decode("Grant", json!({"id":"source-owner","scope":{"client":"one","project":"project"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":4,"max_tokens":"100000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":5,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"sources","visible_splits":["development"],"allow_export":true})).unwrap()
}
fn finding(store: &Store, grant: &Grant, references: Vec<String>) -> RecordEnvelope {
    store.submit(grant, &decode("RecordSubmission", json!({"kind":"finding","provenance":{"origin":"observed","source_refs":references,"scenario_family":"source-availability","split":"development","limitations":[]},"body":{"subject":"artifact","observation":"withdrawn derived content","interpretation":"source-dependent interpretation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap(),false).unwrap()
}

#[test]
fn r1_04_interrupted_cleanup_cannot_expose_transitive_derivatives() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    let source = finding(&store, &owner, vec![]);
    let derivative = finding(&store, &owner, vec![source.id.clone()]);
    let second = finding(&store, &owner, vec![derivative.id.clone()]);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER fail_derivative BEFORE UPDATE ON records WHEN OLD.id='{}' BEGIN SELECT RAISE(FAIL,'injected derivative cleanup failure'); END;", derivative.id)).unwrap();
    let retirement = store.retire(
        &owner,
        &RetireRequest {
            id: source.id.clone(),
            expected_version: source.version,
            delete: true,
        },
    );
    // The tombstone must deny access even when subsequent physical cleanup fails.
    assert!(store.record(&owner, &source.id).is_err());
    assert!(
        store.record(&owner, &derivative.id).is_err(),
        "direct lookup leaked a derivative while cleanup was interrupted"
    );
    assert!(
        store.record(&owner, &second.id).is_err(),
        "transitive derivative leaked"
    );
    let query = decode(
        "SearchRequest",
        json!({"query":"withdrawn","inventory":"evidence","limit":100,"offset":0}),
    )
    .unwrap();
    assert!(store.search(&owner, &query).unwrap().records.is_empty());
    assert!(
        retirement.is_ok(),
        "logical retirement should be acknowledged; physical cleanup has a separate status"
    );
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert!(reopened.record(&owner, &second.id).is_err());
    assert!(reopened.search(&owner, &query).unwrap().records.is_empty());
    let status = reopened.source_cleanup_status(&owner).unwrap();
    assert_eq!(status["jobs"][0]["status"], "failed");
    assert!(
        status["jobs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("injected derivative cleanup failure")
    );
    let mut other = owner.clone();
    other.scope.client = "other".into();
    assert!(
        reopened.source_cleanup_status(&other).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.execute_batch("DROP TRIGGER fail_derivative").unwrap();
    for _ in 0..3 {
        reopened.cleanup_sources(&owner, 100).unwrap();
    }
    assert!(
        reopened.source_cleanup_status(&owner).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job["status"] == "complete")
    );
    let bodies: Vec<String> = db
        .prepare("SELECT body FROM records")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert!(
        bodies
            .iter()
            .all(|body| !body.contains("withdrawn derived content"))
    );
}

#[test]
fn r1_08_schema_two_upgrade_backs_up_and_reconstructs_only_declared_edges() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("previous.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    db.execute_batch(include_str!("../src/migration-2.sql"))
        .unwrap();
    let owner = grant();
    for (id, references) in [
        ("legacy-source", vec![]),
        ("legacy-derived", vec!["legacy-source"]),
    ] {
        let body = json!({"schema_version":"1","id":id,"scope":owner.scope,"kind":"finding","version":"1","created_ms":"1","updated_ms":"1","retired":false,"provenance":{"origin":"observed","source_refs":references,"scenario_family":"legacy","split":"development","limitations":[]},"body":{"subject":"legacy artifact","observation":"retained observation","interpretation":"legacy interpretation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}});
        db.execute("INSERT INTO records(id,client,project,kind,version,split,body) VALUES(?1,'one','project','finding','1','development',?2)",rusqlite::params![id,body.to_string()]).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.record(&owner, "legacy-derived").unwrap().body["observation"],
        "retained observation"
    );
    assert_eq!(db.query_row("SELECT count(*) FROM source_edges WHERE subject_id='legacy-derived' AND source_id='legacy-source' AND source_kind='record' AND source_version IS NULL",[],|r|r.get::<_,u32>(0)).unwrap(),1);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    let backups = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("before-schema-2-")
                .then_some(path)
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert!(
        store.source_cleanup_status(&grant()).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(Store::open(&path).is_ok());
}

#[test]
fn r1_08_failed_source_migration_preserves_schema_two() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("previous.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/store.sql")).unwrap();
    db.execute_batch(include_str!("../src/migration-2.sql"))
        .unwrap();
    db.execute_batch("CREATE TABLE source_edges(existing TEXT)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='source_tombstones'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn r1_04_large_cleanup_has_bounded_batches_and_resumes_after_a_midway_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    let source = finding(&store, &owner, vec![]);
    for index in 0..1050 {
        let event:Event=decode("Event",json!({"id":format!("large-event-{index:04}"),"scope":owner.scope,"run_id":"source-run","producer":"source-agent","sequence":index.to_string(),"kind":"observation","timestamp_ms":"1","parents":[],"correlation":"large","artifacts":[],"payload":{"text":"withdrawn event content"},"provenance":{"origin":"observed","source_refs":[source.id],"scenario_family":"large-cleanup","split":"development","limitations":[]}})).unwrap();
        store.ingest(&event).unwrap();
    }
    let derivative = finding(&store, &owner, vec!["large-event-1049".into()]);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER interrupt_large_cleanup BEFORE UPDATE OF body ON events WHEN OLD.id='large-event-0150' BEGIN SELECT RAISE(FAIL,'injected later-batch failure'); END").unwrap();
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    let redacted: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE json_extract(body,'$.payload.redacted')=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        redacted <= 100,
        "one cleanup batch rewrote {redacted} event payloads"
    );
    assert!(store.record(&owner, &derivative.id).is_err());
    assert!(
        store
            .evidence(
                &owner,
                &decode("EvidenceRequest", json!({"cursor":"0","limit":100})).unwrap()
            )
            .unwrap()
            .events
            .is_empty()
    );
    for _ in 0..30 {
        if store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"] == "failed" {
            break;
        }
        store.cleanup_sources(&owner, 1).unwrap();
    }
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "failed"
    );
    let before: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE json_extract(body,'$.payload.redacted')=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(before > 0 && before < 1050);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.record(&owner, &derivative.id).is_err());
    db.execute_batch("DROP TRIGGER interrupt_large_cleanup")
        .unwrap();
    for _ in 0..40 {
        store.cleanup_sources(&owner, 1).unwrap();
    }
    let status = store.source_cleanup_status(&owner).unwrap();
    assert!(
        status["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job["status"] == "complete"),
        "large cleanup did not finish: {status}"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE json_extract(body,'$.payload.redacted')=1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1050
    );
    assert!(
        !db.query_row(
            "SELECT body FROM records WHERE id=?1",
            [&derivative.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
        .contains("withdrawn derived content")
    );
}

#[test]
fn r1_04_late_derivatives_reopen_cleanup_without_losing_deleted_ancestry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("late.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    store.register_grant(&owner).unwrap();
    store.begin_run("late-run",&owner.id,&decode("AgentRunRequest",json!({"run_id":"late-run","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect the owner task"})).unwrap()).unwrap();
    let context = store.authorize_context("late-run").unwrap();
    let source = finding(&store, &owner, vec![]);
    let derived = finding(&store, &owner, vec![source.id.clone()]);
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "complete"
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM source_edges WHERE subject_id=?1 AND source_id=?2",
            [&derived.id, &source.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let event:Event=decode("Event",json!({"id":"late-observation","scope":owner.scope,"run_id":"source-run","producer":"source-agent","sequence":"1","kind":"observation","timestamp_ms":"1","parents":[],"correlation":"late","artifacts":[],"payload":{"text":"late withdrawn copy"},"provenance":{"origin":"observed","source_refs":[derived.id],"scenario_family":"late-cleanup","split":"development","limitations":[]}})).unwrap();
    store.ingest(&event).unwrap();
    store.append_context("late-run",&decode("ContextAppend",json!({"segment_id":context.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","content":"late withdrawn context","timestamp":1},"sources":[{"kind":"record","id":derived.id,"version":"1"}]}]})).unwrap()).unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "pending"
    );
    assert!(store.require_reference(&owner, &event.id).is_err());
    drop(store);
    let store = Store::open(&path).unwrap();
    store.cleanup_sources(&owner, 1).unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "complete"
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
    assert!(
        !db.query_row(
            "SELECT body FROM events WHERE id='late-observation'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
        .contains("late withdrawn copy")
    );
}

#[test]
fn r1_04_cleanup_status_paginates_all_jobs_without_cross_scope_rows() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("pages.db")).unwrap();
    let owner = grant();
    for _ in 0..105 {
        let record = finding(&store, &owner, vec![]);
        store
            .retire(
                &owner,
                &RetireRequest {
                    id: record.id,
                    expected_version: record.version,
                    delete: true,
                },
            )
            .unwrap();
    }
    let first = store.source_cleanup_status(&owner).unwrap();
    assert_eq!(first["jobs"].as_array().unwrap().len(), 100);
    assert_eq!(first["complete"], false);
    let mut after = "0".to_owned();
    let mut ids = std::collections::HashSet::new();
    loop {
        let page = store
            .source_cleanup_status_page(&owner, &after, 17)
            .unwrap();
        for job in page["jobs"].as_array().unwrap() {
            assert!(ids.insert(job["source_id"].as_str().unwrap().to_owned()));
            assert_eq!(job["status"], "complete");
            assert_eq!(job["queued"], "0");
        }
        if page["complete"] == true {
            break;
        }
        assert_ne!(page["next"], after);
        after = page["next"].as_str().unwrap().to_owned();
    }
    assert_eq!(ids.len(), 105);
    let mut other = owner.clone();
    other.scope.client = "other".into();
    assert!(
        store.source_cleanup_status_page(&other, "0", 100).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(store.source_cleanup_status_page(&owner, "-1", 100).is_err());
    assert!(store.source_cleanup_status_page(&owner, "0", 0).is_err());
}

#[test]
fn r1_04_cleanup_preserves_protected_and_other_client_evidence_and_terminates_cycles() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    let source = finding(&store, &owner, vec![]);
    for (id, client, split, reference) in [
        ("protected", "one", "holdout", source.id.as_str()),
        ("other", "other", "development", source.id.as_str()),
        ("descendant", "one", "development", "protected"),
    ] {
        store.ingest(&decode("Event",json!({"id":id,"scope":{"client":client,"project":"project"},"run_id":"run","producer":id,"sequence":"1","kind":"observation","timestamp_ms":"1","parents":[],"correlation":"scope","artifacts":[],"payload":{"text":"retained under policy"},"provenance":{"origin":"observed","source_refs":[reference],"scenario_family":"cleanup-scope","split":split,"limitations":[]}})).unwrap()).unwrap();
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO source_edges(subject_kind,subject_id,source_kind,source_id) VALUES('event','protected','event','descendant')",[]).unwrap();
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "complete"
    );
    for id in ["protected", "other"] {
        assert!(
            db.query_row("SELECT body FROM events WHERE id=?1", [id], |r| r
                .get::<_, String>(0))
                .unwrap()
                .contains("retained under policy")
        );
    }
    assert!(
        !db.query_row("SELECT body FROM events WHERE id='descendant'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .contains("retained under policy")
    );
    let mut evaluator = owner.clone();
    evaluator.visible_splits.push(Split::Holdout);
    assert!(store.require_reference(&evaluator, "protected").is_err());
}

#[test]
fn r1_04_deleting_an_already_retired_source_escalates_its_entire_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("escalation.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    let source = finding(&store, &owner, vec![]);
    let derived = finding(&store, &owner, vec![source.id.clone()]);
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: source.version,
                delete: false,
            },
        )
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let version: String = db
        .query_row(
            "SELECT version FROM records WHERE id=?1",
            [&source.id],
            |r| r.get(0),
        )
        .unwrap();
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: version,
                delete: true,
            },
        )
        .unwrap();
    for id in [&source.id, &derived.id] {
        assert!(
            !db.query_row("SELECT body FROM records WHERE id=?1", [id], |r| r
                .get::<_, String>(0))
                .unwrap()
                .contains("withdrawn derived content")
        );
    }
    let status = store.source_cleanup_status(&owner).unwrap();
    assert_eq!(status["jobs"][0]["delete_content"], true);
    assert_eq!(status["jobs"][0]["status"], "complete");
}

#[test]
fn r1_04_payload_and_frontier_roll_back_together_when_progress_cannot_be_saved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("atomic.db");
    let store = Store::open(&path).unwrap();
    let owner = grant();
    let source = finding(&store, &owner, vec![]);
    let event:Event=decode("Event",json!({"id":"cursor-event","scope":owner.scope,"run_id":"run","producer":"source","sequence":"1","kind":"observation","timestamp_ms":"1","parents":[],"correlation":"atomic","artifacts":[],"payload":{"text":"withdrawn cursor content"},"provenance":{"origin":"observed","source_refs":[source.id],"scenario_family":"atomic-cleanup","split":"development","limitations":[]}})).unwrap();
    store.ingest(&event).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_progress BEFORE UPDATE ON source_cleanup_items WHEN OLD.id='cursor-event' BEGIN SELECT RAISE(FAIL,'injected progress write failure'); END").unwrap();
    store
        .retire(
            &owner,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "failed"
    );
    assert!(store.require_reference(&owner, &event.id).is_err());
    assert!(
        db.query_row("SELECT body FROM events WHERE id='cursor-event'", [], |r| r
            .get::<_, String>(0))
            .unwrap()
            .contains("withdrawn cursor content")
    );
    assert_eq!(
        db.query_row(
            "SELECT payload_done FROM source_cleanup_items WHERE id='cursor-event'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_progress").unwrap();
    store.cleanup_sources(&owner, 1).unwrap();
    assert_eq!(
        store.source_cleanup_status(&owner).unwrap()["jobs"][0]["status"],
        "complete"
    );
    assert!(
        !db.query_row("SELECT body FROM events WHERE id='cursor-event'", [], |r| r
            .get::<_, String>(0))
            .unwrap()
            .contains("withdrawn cursor content")
    );
}
