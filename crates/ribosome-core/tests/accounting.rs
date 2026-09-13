use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::json;
use std::sync::{Arc, Barrier};

fn fixture(calls: u32) -> (tempfile::TempDir, Store, Grant) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant: Grant = decode("Grant", json!({"id":"accounting","scope":{"client":"test","project":"budget"},"mode":"apply","paths":[],"tools":[],"profiles":["caretaker","curator","experimenter"],"budget":{"max_calls":calls,"max_tokens":"100","max_cost_microusd":"100","max_actions":5,"max_work_items":5,"max_depth":3,"deadline_ms":(now_ms()+60000).to_string()},"context":"budget","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    (directory, store, grant)
}
fn run(store: &Store, grant: &Grant, id: &str, parent: Option<String>) {
    let request: AgentRunRequest = decode("AgentRunRequest", json!({"run_id":id,"profile":"caretaker","operator":"proofreading@1","prompt":"Accounting boundary test","provider":"openai","model":"test-model"})).unwrap();
    store
        .begin_run(
            id,
            &grant.id,
            &AgentRunRequest {
                parent_allocation_id: parent,
                ..request
            },
        )
        .unwrap();
}
fn request(call: &str) -> PermitRequest {
    decode("PermitRequest", json!({"call_id":call,"max_output_tokens":10,"input_tokens_bound":"10","cost_microusd_bound":"20"})).unwrap()
}

#[test]
fn child_ceilings_share_root_capacity_and_cannot_change_authority_on_retry() {
    let (_directory, store, grant) = fixture(5);
    let root = store.root_budget_status(&grant).unwrap().allocation;
    let mut budget = grant.budget.clone();
    budget.max_calls = 2;
    let allocation = BudgetAllocationRequest {
        id: "study".into(),
        parent_id: root.id.clone(),
        cause_id: "host-study".into(),
        purpose: "experiment".into(),
        budget,
    };
    let study = store.allocate(&grant, &allocation).unwrap();
    assert_eq!(study, store.allocate(&grant, &allocation).unwrap());
    let mut changed = allocation.clone();
    changed.budget.max_calls = 3;
    assert!(store.allocate(&grant, &changed).is_err());
    let mut over = allocation.clone();
    over.id = "over".into();
    over.budget.max_tokens = "101".into();
    assert!(store.allocate(&grant, &over).is_err());
    run(&store, &grant, "a", Some(study.id.clone()));
    run(&store, &grant, "b", Some(study.id.clone()));
    let one = store.permit("a", &request("one")).unwrap();
    assert_eq!(one, store.permit("a", &request("one")).unwrap());
    let mut changed_call = request("one");
    changed_call.max_output_tokens = 11;
    assert!(store.permit("a", &changed_call).is_err());
    store.permit("b", &request("two")).unwrap();
    assert!(store.permit("a", &request("three")).is_err());
    assert_eq!(
        store
            .budget_status(&grant, &study.id)
            .unwrap()
            .usage
            .model_calls,
        2
    );
    let root = store.root_budget_status(&grant).unwrap();
    assert_eq!(
        root.usage.model_calls, 2,
        "ancestor rollups must not add each leaf more than once"
    );
    assert_eq!(root.usage.reserved_tokens, "40");
    assert_eq!(root.remaining.max_calls, 3);
    let mut foreign = grant.clone();
    foreign.id = "foreign".into();
    store.register_grant(&foreign).unwrap();
    assert!(store.allocate(&foreign, &allocation).is_err());
    run(&store, &grant, "outside", None);
    assert!(store.lookup_permit("outside", "one").is_err());
    store.permit("outside", &request("outside-call")).unwrap();
    let study_status = store.budget_status(&grant, &study.id).unwrap();
    assert_eq!(study_status.usage.model_calls, 2);
    assert_eq!(
        study_status.remaining.max_tokens, "40",
        "remaining capacity must include sibling spending at the root"
    );
    assert_eq!(
        store.run_budget_status("a").unwrap().remaining,
        study_status.remaining
    );
}

#[test]
fn concurrent_reservations_and_conflicting_settlements_are_atomic() {
    let (directory, store, mut grant) = fixture(10);
    grant.id = "one-reservation".into();
    grant.budget.max_tokens = "20".into();
    grant.budget.max_cost_microusd = "20".into();
    store.register_grant(&grant).unwrap();
    run(&store, &grant, "one", None);
    run(&store, &grant, "two", None);
    let barrier = Arc::new(Barrier::new(2));
    let jobs: Vec<_> = ["one", "two"]
        .into_iter()
        .map(|id| {
            let barrier = barrier.clone();
            let path = directory.path().join("state.db");
            std::thread::spawn(move || {
                let store = Store::open(path).unwrap();
                barrier.wait();
                (id, store.permit(id, &request(id)))
            })
        })
        .collect();
    let results: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let (run, permit) = results
        .into_iter()
        .find(|(_, result)| result.is_ok())
        .unwrap();
    let permit = permit.unwrap();
    store.dispatch_permit(run, &permit.id).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let jobs: Vec<_> = [3, 7]
        .into_iter()
        .map(|cost| {
            let barrier = barrier.clone();
            let path = directory.path().join("state.db");
            let permit = permit.id.clone();
            std::thread::spawn(move || {
                let store = Store::open(path).unwrap();
                barrier.wait();
                let usage = Usage {
                    permit_id: permit,
                    input_tokens: cost.to_string(),
                    output_tokens: "0".into(),
                    cost_microusd: cost.to_string(),
                    complete: true,
                };
                (cost, store.usage(run, &usage))
            })
        })
        .collect();
    let results: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let cost = results.iter().find(|(_, result)| result.is_ok()).unwrap().0;
    let status = store.root_budget_status(&grant).unwrap();
    assert_eq!(status.usage.settled_cost_microusd, cost.to_string());
    assert_eq!(status.usage.reserved_cost_microusd, "0");
}

#[test]
fn dispatched_unknown_usage_survives_reopen_and_only_undispatched_calls_can_release() {
    let (directory, store, grant) = fixture(1);
    run(&store, &grant, "run", None);
    let first = store.permit("run", &request("not-dispatched")).unwrap();
    store.release_permit("run", &first.id).unwrap();
    store.release_permit("run", &first.id).unwrap();
    assert!(store.dispatch_permit("run", &first.id).is_err());
    let dispatched = store.permit("run", &request("lost-response")).unwrap();
    store.dispatch_permit("run", &dispatched.id).unwrap();
    assert!(store.dispatch_permit("run", &dispatched.id).is_err());
    store
        .finish_run(
            "run",
            &AgentResult {
                disposition: Disposition::Interrupted,
                summary: "Injected loss after the durable dispatch boundary.".into(),
            },
        )
        .unwrap();
    drop(store);
    let store = Store::open(directory.path().join("state.db")).unwrap();
    run(&store, &grant, "run", None);
    assert_eq!(
        store.lookup_permit("run", "lost-response").unwrap(),
        dispatched
    );
    assert!(store.release_permit("run", &dispatched.id).is_err());
    assert!(store.permit("run", &request("new-call")).is_err());
    let status = store.root_budget_status(&grant).unwrap();
    assert_eq!(status.usage.unknown_calls, 1);
    assert_eq!(status.usage.reserved_tokens, "20");
    let usage = Usage {
        permit_id: dispatched.id,
        input_tokens: "4".into(),
        output_tokens: "2".into(),
        cost_microusd: "6".into(),
        complete: true,
    };
    store.usage("run", &usage).unwrap();
    store.usage("run", &usage).unwrap();
    let settled = store.root_budget_status(&grant).unwrap();
    assert_eq!(settled.usage.unknown_calls, 0);
    assert_eq!(settled.usage.settled_tokens, "6");
    assert_eq!(settled.usage.model_calls, 1);
}

#[test]
fn action_attempts_and_followup_work_obey_child_ceilings_without_resetting_the_root() {
    let (directory, store, grant) = fixture(5);
    let root = store.root_budget_status(&grant).unwrap().allocation;
    let mut budget = grant.budget.clone();
    budget.max_actions = 1;
    budget.max_work_items = 1;
    budget.max_depth = 1;
    let child = store
        .allocate(
            &grant,
            &BudgetAllocationRequest {
                id: "limited".into(),
                parent_id: root.id,
                cause_id: "owner".into(),
                purpose: "curation".into(),
                budget,
            },
        )
        .unwrap();
    run(&store, &grant, "parent", Some(child.id));
    let host =
        ribosome_core::host::LocalHost::new(directory.path(), std::collections::BTreeMap::new())
            .unwrap();
    // Acquire the normal host lifecycle before dispatching; construction
    // interrupts the seeded run, which is then explicitly resumed.
    let runtime = ribosome_core::effects::Runtime::new(
        store,
        Box::new(host),
        directory.path().join("runtime"),
    )
    .unwrap();
    run(&runtime.store, &grant, "parent", Some("limited".into()));
    let action = |id| {
        decode(
            "Action",
            json!({"operation_id":id,"kind":"check","tool":"unavailable"}),
        )
        .unwrap()
    };
    let first = runtime.execute("parent", action("denied-attempt")).unwrap();
    assert_eq!(first.status, EffectStatus::Denied);
    let second = runtime.execute("parent", action("over-ceiling")).unwrap();
    assert!(second.output.contains("child action budget"));
    let request = |subject| {
        decode("WorkRequest",json!({"subject":subject,"profile":"curator","operator":"motif-discovery@1","reason":"follow up","evidence_refs":[]})).unwrap()
    };
    let work = runtime
        .store
        .request_work("parent", &request("one"))
        .unwrap();
    assert!(
        runtime
            .store
            .request_work("parent", &request("two"))
            .is_err()
    );
    runtime
        .store
        .finish_run(
            "parent",
            &AgentResult {
                disposition: Disposition::Completed,
                summary: "Follow-up queued before completion.".into(),
            },
        )
        .unwrap();
    let child_request: AgentRunRequest = decode("AgentRunRequest",json!({"run_id":work.id,"profile":"curator","operator":"motif-discovery@1","prompt":"Follow-up","provider":"openai","model":"test-model"})).unwrap();
    runtime
        .store
        .begin_run(&work.id, &grant.id, &child_request)
        .unwrap();
    assert!(
        runtime
            .store
            .permit(&work.id, &crate::request("child-call"))
            .is_ok(),
        "completion of the parent must not revoke already queued work"
    );
    assert!(
        runtime
            .store
            .request_work(&work.id, &request("too-deep"))
            .is_err()
    );
    let root = runtime.store.root_budget_status(&grant).unwrap();
    assert_eq!(
        root.usage.actions, 2,
        "denied attempts retain the existing action-counting semantics"
    );
    assert_eq!(root.usage.work_items, 1);
    assert_eq!(root.usage.model_calls, 1);
}

#[test]
fn schema_sixteen_rolls_back_partial_migration_and_preserves_unattributed_legacy_liabilities() {
    let (directory, store, grant) = fixture(5);
    run(&store, &grant, "run", None);
    let known = store.permit("run", &request("known")).unwrap();
    store.dispatch_permit("run", &known.id).unwrap();
    let usage = Usage {
        permit_id: known.id.clone(),
        input_tokens: "4".into(),
        output_tokens: "2".into(),
        cost_microusd: "6".into(),
        complete: true,
    };
    store.usage("run", &usage).unwrap();
    let unknown = store.permit("run", &request("unknown")).unwrap();
    store.dispatch_permit("run", &unknown.id).unwrap();
    drop(store);
    let path = directory.path().join("state.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("fixtures/remove-budget-schema.sql"))
        .unwrap();
    db.execute_batch("PRAGMA user_version=15; CREATE TABLE run_allocations(collision TEXT)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        15
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='budget_allocations'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    let body: String = db
        .query_row("SELECT usage FROM permits WHERE id=?1", [&known.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(serde_json::from_str::<Usage>(&body).unwrap(), usage);
    db.execute_batch("DROP TABLE run_allocations").unwrap();
    let reopened = Store::open(&path).unwrap();
    let root = reopened.root_budget_status(&grant).unwrap();
    assert_eq!(root.usage.model_calls, 2);
    assert_eq!(root.usage.unknown_calls, 1);
    assert_eq!(root.usage.reserved_tokens, "20");
    assert_eq!(root.usage.settled_tokens, "6");
    assert_eq!(
        reopened
            .run_budget_status("run")
            .unwrap()
            .allocation
            .purpose,
        "root",
        "migration must not invent historical allocation roles"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM permits WHERE allocation_id IS NOT NULL",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert!(reopened.release_permit("run", &unknown.id).is_err());
}
