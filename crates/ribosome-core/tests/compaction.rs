use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};

fn setup(path: &std::path::Path, run: &str, client: &str) -> (Store, Grant) {
    let store = Store::open(path).unwrap();
    let grant:Grant=decode("Grant",json!({"id":client,"scope":{"client":client,"project":"summary"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"10000000","max_cost_microusd":"1000000","max_actions":10,"max_work_items":10,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"summary","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    store.begin_run(run,&grant.id,&decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Preserve the outstanding review obligation."})).unwrap()).unwrap();
    (store, grant)
}
fn source(store: &Store, grant: &Grant) -> RecordEnvelope {
    store.submit(grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"summary","split":"development","limitations":[]},"body":{"kind":"episodic","content":"unique obligation","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap()
}
fn append(store: &Store, run: &str, message: Value, sources: Value) {
    let state = store.authorize_context(run).unwrap();
    store.append_context(run,&decode("ContextAppend",json!({"segment_id":state.segment_id,"after":state.count,"entries":[{"message":message,"sources":sources}]})).unwrap()).unwrap();
}
fn grow(store: &Store, source: &RecordEnvelope) {
    for _ in 0..4 {
        append(
            store,
            "run",
            json!({"role":"assistant","content":[{"type":"thinking","thinking":"private reasoning must not enter compaction"},{"type":"text","text":"Read the observation"}],"timestamp":1}),
            json!([]),
        );
        append(
            store,
            "run",
            json!({"role":"toolResult","toolName":"record_read","toolCallId":"fixture","content":[{"type":"text","text":"Observed fact. ".repeat(5000)}],"timestamp":1}),
            json!([{"kind":"record","id":source.id,"version":"1"}]),
        );
    }
}
fn permit(store: &Store, summary: Option<&str>) -> Permit {
    let mut request = json!({"max_output_tokens":1000,"input_tokens_bound":"10000","cost_microusd_bound":"10000"});
    if let Some(summary) = summary {
        request["compaction_id"] = json!(summary);
    }
    store
        .permit("run", &decode("PermitRequest", request).unwrap())
        .unwrap()
}
fn settle(store: &Store, permit: &Permit, complete: bool) {
    store
        .usage(
            "run",
            &Usage {
                permit_id: permit.id.clone(),
                input_tokens: "10".into(),
                output_tokens: "10".into(),
                cost_microusd: "20".into(),
                complete,
            },
        )
        .unwrap();
}
fn commit(store: &Store, plan: &CompactionPlan, text: &str) -> ContextSummary {
    let permit = permit(store, Some(&plan.id));
    settle(store, &permit, true);
    store
        .commit_compaction(
            "run",
            &CompactionCommit {
                id: plan.id.clone(),
                text: text.into(),
                permit_id: permit.id,
            },
        )
        .unwrap()
}

#[test]
fn r1_05_10_summaries_are_metered_idempotent_and_inherit_host_lineage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let (store, grant) = setup(&path, "run", "owner");
    let memory = source(&store, &grant);
    append(
        &store,
        "run",
        json!({"role":"user","content":"Owner task","timestamp":1}),
        json!([]),
    );
    grow(&store, &memory);
    let plan = store.prepare_compaction("run").unwrap().plan.unwrap();
    assert_eq!(store.prepare_compaction("run").unwrap().plan.unwrap(), plan);
    let input = store.read_compaction("run", &plan.id).unwrap();
    assert!(
        !serde_json::to_string(&input)
            .unwrap()
            .contains("private reasoning")
    );
    assert!(input.messages.len() <= 20);
    let (other, _) = setup(&path, "other-run", "other-client");
    assert!(other.read_compaction("other-run", &plan.id).is_err());
    let unrelated = permit(&store, None);
    settle(&store, &unrelated, true);
    let mut request = CompactionCommit {
        id: plan.id.clone(),
        text: "Preserve unique obligation".into(),
        permit_id: unrelated.id,
    };
    assert!(store.commit_compaction("run", &request).is_err());
    let unfinished = permit(&store, Some(&plan.id));
    request.permit_id = unfinished.id.clone();
    assert!(store.commit_compaction("run", &request).is_err());
    settle(&store, &unfinished, false);
    assert!(store.commit_compaction("run", &request).is_err());
    let valid = permit(&store, Some(&plan.id));
    request.permit_id = valid.id.clone();
    settle(&store, &valid, true);
    let fault_db = rusqlite::Connection::open(&path).unwrap();
    fault_db.execute_batch("CREATE TRIGGER fail_summary_commit BEFORE INSERT ON events WHEN NEW.producer='pi-compactor' BEGIN SELECT RAISE(FAIL,'injected summary event failure'); END").unwrap();
    assert!(store.commit_compaction("run", &request).is_err());
    assert_eq!(
        fault_db
            .query_row(
                "SELECT status FROM context_summaries WHERE id=?1",
                [&request.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "prepared"
    );
    fault_db
        .execute_batch("DROP TRIGGER fail_summary_commit")
        .unwrap();
    let first = store.commit_compaction("run", &request).unwrap();
    assert_eq!(store.commit_compaction("run", &request).unwrap(), first);
    assert!(
        first
            .sources
            .iter()
            .any(|s| s.id == memory.id && s.version == "1")
    );
    request.text = "different summary".into();
    assert!(store.commit_compaction("run", &request).is_err());
    assert!(other.read_summary("other-run", &first.id).is_err());
    grow(&store, &memory);
    let plan = store.prepare_compaction("run").unwrap().plan.unwrap();
    assert_eq!(
        store
            .read_compaction("run", &plan.id)
            .unwrap()
            .previous_summary
            .unwrap()
            .id,
        first.id
    );
    let second = commit(
        &store,
        &plan,
        "Preserve unique obligation and its prior summary",
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM source_edges WHERE subject_id=?1 AND source_id=?2",
            [&second.id, &first.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let retained: i64 = db
        .query_row(
            "SELECT count(*) FROM context_items WHERE body IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, 17);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.authorize_context("run").unwrap().summary_ref,
        Some(second.id)
    );
}

#[test]
fn r1_04_05_withdrawal_blocks_prepared_and_transitive_summaries_before_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let (store, grant) = setup(&path, "run", "owner");
    let memory = source(&store, &grant);
    grow(&store, &memory);
    let plan = store.prepare_compaction("run").unwrap().plan.unwrap();
    let first = commit(&store, &plan, "unique obligation");
    grow(&store, &memory);
    let plan = store.prepare_compaction("run").unwrap().plan.unwrap();
    let second = commit(&store, &plan, "unique obligation inherited");
    grow(&store, &memory);
    let pending = store.prepare_compaction("run").unwrap().plan.unwrap();
    let paid = permit(&store, Some(&pending.id));
    settle(&store, &paid, true);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_summary_cleanup BEFORE UPDATE OF body ON context_summaries BEGIN SELECT RAISE(FAIL,'injected summary cleanup failure'); END").unwrap();
    store
        .retire(
            &grant,
            &RetireRequest {
                id: memory.id,
                expected_version: "1".into(),
                delete: true,
            },
        )
        .unwrap();
    assert!(store.read_summary("run", &first.id).is_err());
    assert!(store.read_summary("run", &second.id).is_err());
    assert!(store.read_compaction("run", &pending.id).is_err());
    assert!(
        store
            .commit_compaction(
                "run",
                &CompactionCommit {
                    id: pending.id,
                    text: "in flight stale summary".into(),
                    permit_id: paid.id
                }
            )
            .is_err()
    );
    assert_eq!(
        store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    db.execute_batch("DROP TRIGGER fail_summary_cleanup")
        .unwrap();
    store.cleanup_sources(&grant, 100).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM context_summaries WHERE body IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert!(
        store
            .authorize_context("run")
            .unwrap()
            .summary_ref
            .is_none()
    );
}
