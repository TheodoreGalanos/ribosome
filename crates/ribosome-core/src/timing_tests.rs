use super::*;
use crate::{
    contracts::*,
    validation::{decode, now_ms},
};

fn fixture() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant: Grant = decode("Grant", json!({"id":"timing","scope":{"client":"test","project":"timing"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":1,"max_work_items":1,"max_depth":1,"deadline_ms":(now_ms()+60000).to_string()},"context":"timing","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let request = decode("AgentRunRequest", json!({"run_id":"run","profile":"caretaker","operator":"proofreading@1","prompt":"Measure host boundaries","provider":"openai","model":"test-model"})).unwrap();
    store.begin_run("run", &grant.id, &request).unwrap();
    (directory, store)
}

fn permit(store: &Store, call: Option<&str>) -> Permit {
    store
        .permit(
            "run",
            &PermitRequest {
                call_id: call.map(str::to_owned),
                max_output_tokens: 10,
                input_tokens_bound: "10".into(),
                cost_microusd_bound: "20".into(),
                compaction_id: None,
            },
        )
        .unwrap()
}

fn report(store: &Store, permit: &Permit, complete: bool) -> Usage {
    let usage = Usage {
        permit_id: permit.id.clone(),
        input_tokens: "2".into(),
        output_tokens: "3".into(),
        cost_microusd: "4".into(),
        complete,
    };
    store.usage("run", &usage).unwrap();
    usage
}

#[test]
fn timings_aggregate_reopen_and_reject_partial_updates() {
    let (directory, store) = fixture();
    store
        .record_timings(
            "run",
            &[
                (TimingPhase::WorkerQueue, Duration::from_micros(7)),
                (TimingPhase::WorkerQueue, Duration::from_micros(13)),
                (TimingPhase::StateService, Duration::from_micros(3)),
            ],
        )
        .unwrap();
    let saved = store.inspect_timings("run").unwrap();
    assert_eq!(
        saved["spans"]["worker_queue"],
        json!({"samples":2,"total_us":"20","max_us":"13"})
    );
    assert!(
        saved["spans"].get("executor_service").is_none(),
        "no observation is not zero execution time"
    );
    store.db.execute_batch("CREATE TRIGGER reject_timing BEFORE UPDATE OF timings ON runs BEGIN SELECT RAISE(ABORT,'timing fault'); END").unwrap();
    assert!(
        store
            .record_timings("run", &[(TimingPhase::WorkerQueue, Duration::from_secs(1))])
            .is_err()
    );
    store.observe_timings(
        "run",
        &[(TimingPhase::StateService, Duration::from_secs(1))],
    );
    assert_eq!(store.inspect_timings("run").unwrap(), saved);
    drop(store);
    assert_eq!(
        Store::open(directory.path().join("state.db"))
            .unwrap()
            .inspect_timings("run")
            .unwrap(),
        saved
    );
}

#[test]
fn provider_observation_is_distinct_from_settlement_and_unknown_dispatch() {
    let (directory, store) = fixture();
    let completed = permit(&store, Some("completed"));
    let before = now_ms();
    store.dispatch_permit("run", &completed.id).unwrap();
    assert!(store.dispatch_permit("run", &completed.id).is_err());
    let usage = report(&store, &completed, true);
    let after = now_ms();
    let stamps = || {
        store
            .db
            .query_row(
                "SELECT dispatched_ms,observed_ms FROM permits WHERE id=?1",
                [&completed.id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .unwrap()
    };
    let original = stamps();
    assert!((before..=after).contains(&counter(&original.0).unwrap()));
    assert!((before..=after).contains(&counter(&original.1).unwrap()));
    store.usage("run", &usage).unwrap();
    assert_eq!(
        stamps(),
        original,
        "repeated usage does not move the observation boundary"
    );
    let incomplete = permit(&store, Some("incomplete"));
    store.dispatch_permit("run", &incomplete.id).unwrap();
    report(&store, &incomplete, false);
    let unknown = permit(&store, Some("unknown"));
    store.dispatch_permit("run", &unknown.id).unwrap();
    let legacy = permit(&store, None);
    report(&store, &legacy, true);
    let reserved = permit(&store, Some("reserved"));
    store.release_permit("run", &reserved.id).unwrap();
    // Deterministic clock-fault injection; actual dispatch/report timestamps
    // above establish that the real API records both boundaries.
    store
        .db
        .execute(
            "UPDATE permits SET dispatched_ms='1000',observed_ms='1040' WHERE id=?1",
            [&completed.id],
        )
        .unwrap();
    store
        .db
        .execute(
            "UPDATE permits SET dispatched_ms='1000',observed_ms='990' WHERE id=?1",
            [&incomplete.id],
        )
        .unwrap();
    let expected = json!({"observed_calls":2,"total_ms":"40","unobserved_dispatched_calls":1,"missing_dispatch_time_calls":1,"clock_reversals":1});
    assert_eq!(
        store.inspect_run("run").unwrap()["timings"]["provider_round_trip"],
        expected
    );
    let budget = store.run_budget_status("run").unwrap();
    assert_eq!(budget.usage.unknown_calls, 2);
    drop(store);
    let reopened = Store::open(directory.path().join("state.db")).unwrap();
    assert_eq!(
        reopened.inspect_timings("run").unwrap()["provider_round_trip"],
        expected
    );
    assert_eq!(
        reopened.run_budget_status("run").unwrap().usage,
        budget.usage
    );
}

#[test]
fn schema_eighteen_rolls_back_and_preserves_legacy_usage_without_inventing_timestamps() {
    let (directory, store) = fixture();
    let legacy = permit(&store, None);
    report(&store, &legacy, true);
    let saved: String = store
        .db
        .query_row("SELECT usage FROM permits WHERE id=?1", [&legacy.id], |r| {
            r.get(0)
        })
        .unwrap();
    drop(store);
    let path = directory.path().join("state.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    // Leave dispatched_ms to collide after the first ALTER TABLE.
    db.execute_batch("DROP TABLE evaluation_sources; DROP TABLE protected_exposures; ALTER TABLE archive DROP COLUMN evidence; DROP TRIGGER record_index_insert; DROP TRIGGER record_index_update; DROP TRIGGER record_index_delete; DROP TABLE record_index_generation; DROP TABLE discovery_corpora; ALTER TABLE runs DROP COLUMN timings; ALTER TABLE permits DROP COLUMN observed_ms; PRAGMA user_version=17").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        17
    );
    assert!(db.prepare("SELECT timings FROM runs").is_err());
    db.execute_batch("ALTER TABLE permits DROP COLUMN dispatched_ms")
        .unwrap();
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row("SELECT usage FROM permits WHERE id=?1", [&legacy.id], |r| r
            .get::<_, String>(0))
            .unwrap(),
        saved
    );
    assert_eq!(
        reopened.inspect_timings("run").unwrap(),
        json!({"spans":{},"provider_round_trip":{"observed_calls":0,"total_ms":"0","unobserved_dispatched_calls":0,"missing_dispatch_time_calls":1,"clock_reversals":0}})
    );
    assert!(std::fs::read_dir(directory.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("before-schema-17")
    }));
}

#[test]
fn sqlite_write_begin_measures_actual_writer_contention() {
    let (_directory, store) = fixture();
    let observer = store.service_connection().unwrap();
    let held = store.write_transaction().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        send.send(()).unwrap();
        observer.write_transaction().unwrap().commit().unwrap();
        observer.write_wait_us.get()
    });
    receive.recv().unwrap();
    std::thread::sleep(Duration::from_millis(120));
    held.commit().unwrap();
    let wait = thread.join().unwrap();
    assert!(wait >= 100_000, "writer contention measured only {wait} us");
}
