use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
fn fixture() -> (tempfile::TempDir, Store, Grant) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.db")).unwrap();
    let grant=decode("Grant",json!({"id":"search-owner","scope":{"client":"test","project":"retrieval"},"mode":"observe","paths":[],"tools":["inspect"],"profiles":["curator"],"budget":{"max_calls":1,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":0,"max_work_items":0,"max_depth":0,"deadline_ms":(now_ms()+60000).to_string()},"context":"retrieval","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    (dir, store, grant)
}
fn save(
    store: &Store,
    grant: &Grant,
    id: &str,
    text: &str,
    function: &str,
    capabilities: Value,
) -> RecordEnvelope {
    let value = json!({"kind":"implementation","body":{"name":id,"version":"1","motifs":[],"format":"instructions","material":text,"function":function,"parameters":{},"required_capabilities":capabilities,"state_assumptions":[],"possible_effects":[],"failure_behavior":"abstain","evaluation_refs":[]},"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"fixed-retrieval-corpus","split":"development","limitations":["Authored relevance labels, not semantic applicability"]}});
    store
        .submit(grant, &decode("RecordSubmission", value).unwrap(), false)
        .unwrap()
}
fn request(query: &str) -> SearchRequest {
    decode(
        "SearchRequest",
        json!({"query":query,"inventory":"evidence","offset":0,"limit":2}),
    )
    .unwrap()
}
#[test]
fn lexical_modes_ranking_and_exact_metadata_have_measured_retrieval_tradeoffs() {
    let (_dir, store, grant) = fixture();
    save(
        &store,
        &grant,
        "a-noisy",
        "check unrelated archived notes repeated overview documentation repair migration append background housekeeping fallback build packaging",
        "unrelated",
        json!([]),
    );
    save(
        &store,
        &grant,
        "b-repair",
        "repair repair repair",
        "repair",
        json!(["inspect"]),
    );
    save(
        &store,
        &grant,
        "c-check",
        "check check check",
        "check",
        json!(["inspect"]),
    );
    save(
        &store,
        &grant,
        "d-relevant",
        "check repair",
        "repair",
        json!(["inspect"]),
    );
    save(
        &store,
        &grant,
        "e-ineligible",
        "check repair check repair",
        "repair",
        json!(["unavailable"]),
    );
    let mut q = request("check repair");
    q.eligible = Some(true);
    let default = store.search(&grant, &q).unwrap();
    assert_eq!(
        default
            .records
            .iter()
            .map(|r| r.body["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["a-noisy", "d-relevant"]
    );
    q.order = Some(SearchRequestOrder::Relevance);
    let ranked = store.search(&grant, &q).unwrap();
    assert_eq!(ranked.records[0].body["name"], "d-relevant");
    q.query_mode = Some(SearchRequestQueryMode::AnyTerms);
    q.limit = 100;
    let broader = store.search(&grant, &q).unwrap();
    assert_eq!(broader.records.len(), 4);
    q.function = Some("repair".into());
    let exact = store.search(&grant, &q).unwrap();
    assert_eq!(exact.records.len(), 2);
    assert!(
        exact
            .records
            .iter()
            .all(|r| r.body["name"] == "b-repair" || r.body["name"] == "d-relevant")
    );
    println!(
        "Fixed labels: default AND top-1 usefulness 0/1; relevance AND 1/1; any-term eligible recall 3/3 relevant records with 1 benign distractor; exact repair function 2/2; eligibility errors 0."
    );
}
#[test]
fn cursor_pages_are_complete_and_reject_changed_queries_or_indexes() {
    let (_dir, store, grant) = fixture();
    for i in 0..5 {
        save(
            &store,
            &grant,
            &format!("record-{i}"),
            "check repair",
            "repair",
            json!([]),
        );
    }
    for order in [SearchRequestOrder::Id, SearchRequestOrder::Relevance] {
        let mut q = request("check repair");
        q.order = Some(order);
        let mut ids = Vec::new();
        let mut calls = 0;
        loop {
            let page = store.search(&grant, &q).unwrap();
            calls += 1;
            ids.extend(
                page.records
                    .iter()
                    .map(|r| r.body["name"].as_str().unwrap().to_owned()),
            );
            if page.complete == Some(true) {
                assert!(page.next.is_none());
                break;
            }
            q.after = page.next;
            assert!(q.after.is_some());
        }
        assert_eq!(
            ids,
            (0..5).map(|i| format!("record-{i}")).collect::<Vec<_>>()
        );
        assert_eq!(calls, 3);
    }
    let mut q = request("check");
    q.after = store.search(&grant, &q).unwrap().next;
    q.query = "repair".into();
    assert!(store.search(&grant, &q).is_err());
    q.query = "check".into();
    save(
        &store,
        &grant,
        "record-new",
        "check repair",
        "repair",
        json!([]),
    );
    assert!(
        store
            .search(&grant, &q)
            .unwrap_err()
            .message
            .contains("restart pagination")
    );
}
#[test]
fn hidden_and_retired_matches_do_not_appear_in_pages_or_terminal_counts() {
    let (_dir, store, grant) = fixture();
    let visible = save(&store, &grant, "visible", "rarecheck", "repair", json!([]));
    let mut other = grant.clone();
    other.id = "other".into();
    other.scope.client = "other".into();
    store.register_grant(&other).unwrap();
    save(&store, &other, "hidden", "rarecheck", "repair", json!([]));
    let page = store.search(&grant, &request("rarecheck")).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.next_offset, 1);
    assert_eq!(page.complete, Some(true));
    assert!(page.next.is_none());
    store
        .retire(
            &grant,
            &decode(
                "RetireRequest",
                json!({"id":visible.id,"expected_version":visible.version,"delete":false}),
            )
            .unwrap(),
        )
        .unwrap();
    let page = store.search(&grant, &request("rarecheck")).unwrap();
    assert!(page.records.is_empty());
    assert_eq!(page.next_offset, 0);
    assert_eq!(page.complete, Some(true));
}

#[test]
fn schema_nineteen_upgrades_with_existing_records_and_a_backup() {
    let (directory, store, grant) = fixture();
    let record = save(
        &store,
        &grant,
        "existing",
        "check repair",
        "repair",
        json!([]),
    );
    drop(store);
    let database = directory.path().join("state.db");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("DROP TRIGGER record_index_insert; DROP TRIGGER record_index_update; DROP TRIGGER record_index_delete; DROP TABLE record_index_generation; PRAGMA user_version=19;").unwrap();
    drop(connection);
    let store = Store::open(&database).unwrap();
    assert_eq!(
        store.record(&grant, &record.id).unwrap().body["name"],
        "existing"
    );
    assert_eq!(
        store
            .search(&grant, &request("check"))
            .unwrap()
            .records
            .len(),
        1
    );
    assert!(std::fs::read_dir(directory.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("before-schema-19")
    }));
}
