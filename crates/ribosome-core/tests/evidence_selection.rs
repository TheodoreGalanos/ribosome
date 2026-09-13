use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};

fn fixture() -> (Store, Grant) {
    let store = Store::open(":memory:").unwrap();
    let grant: Grant = decode("Grant", json!({"id":"grant","scope":{"client":"test","project":"selection"},"mode":"observe","paths":["source.txt","result.txt"],"tools":[],"profiles":["curator"],"budget":{"max_calls":5,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":2,"max_work_items":2,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"selection","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    (store, grant)
}
fn event(grant: &Grant, id: &str, sequence: u32, parents: Vec<&str>) -> Event {
    decode("Event", json!({"id":id,"scope":grant.scope,"run_id":"donor","producer":"worker","sequence":sequence.to_string(),"kind":if sequence==1 {"input"} else {"check"},"timestamp_ms":now_ms().to_string(),"parents":parents,"correlation":"donor","artifacts":[{"path":"source.txt","version":if sequence==1 {"v1"} else {"v2"}}],"payload":{"text":format!("Evidence {id}: literal.%_query")},"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"selection-fixture","split":"development","limitations":["Authored selection fixture; not host execution evidence"]}})).unwrap()
}
fn read(store: &Store, grant: &Grant, extra: Value) -> EvidencePage {
    let mut value = json!({"cursor":"0","limit":10});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    store
        .evidence(grant, &decode("EvidenceRequest", value).unwrap())
        .unwrap()
}
fn ids(page: &EvidencePage) -> Vec<&str> {
    page.events.iter().map(|e| e.id.as_str()).collect()
}

#[test]
fn explicit_events_neighbors_and_intersecting_filters_stay_bounded_and_scoped() {
    let (store, grant) = fixture();
    for (id, sequence, parents) in [
        ("a", 1, vec![]),
        ("b", 2, vec!["a"]),
        ("c", 3, vec!["b"]),
        ("unrelated", 4, vec![]),
    ] {
        store.ingest(&event(&grant, id, sequence, parents)).unwrap();
    }
    let mut foreign = event(&grant, "foreign", 5, vec!["a"]);
    foreign.scope.project = "foreign".into();
    store.ingest(&foreign).unwrap();
    let mut protected = event(&grant, "protected", 6, vec!["a"]);
    protected.provenance.split = Split::Holdout;
    store.ingest(&protected).unwrap();
    assert_eq!(
        ids(&read(&store, &grant, json!({"event_refs":["a"]}))),
        vec!["a"]
    );
    let neighborhood = read(&store, &grant, json!({"event_refs":["a"],"neighbors":true}));
    assert_eq!(
        ids(&neighborhood),
        vec!["a", "b"],
        "one hop excludes the grandchild and inaccessible children"
    );
    assert_eq!(neighborhood.events[1].parents, vec!["a"]);
    let mut dependency: Dependency = decode("Dependency", json!({"source":{"path":"source.txt","version":"v1"},"dependent":{"path":"result.txt","version":"r1"},"basis":"observed","evidence_refs":["a"]})).unwrap();
    store.add_dependency(&grant.scope, &dependency).unwrap();
    let selected = read(&store, &grant, json!({"event_refs":["a"]}));
    assert_eq!(selected.dependencies, vec![dependency.clone()]);
    assert!(
        read(&store, &grant, json!({"event_refs":["b"]}))
            .dependencies
            .is_empty()
    );
    dependency.source.version = "v2".into();
    dependency.basis = DependencyBasis::Inferred;
    store.add_dependency(&grant.scope, &dependency).unwrap();
    assert_eq!(
        read(&store, &grant, json!({"event_refs":["b"]})).dependencies,
        vec![dependency]
    );
    assert_eq!(
        ids(&read(
            &store,
            &grant,
            json!({"event_refs":["b"],"neighbors":true})
        )),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        ids(&read(
            &store,
            &grant,
            json!({"event_refs":["a"],"neighbors":true,"kind":"check","artifact":{"path":"source.txt","version":"v2"},"query":"LITERAL.%_QUERY"})
        )),
        vec!["b"]
    );
    assert!(
        read(&store, &grant, json!({"query":"%"})).events.len() == 4,
        "percent is a literal character, not SQL wildcard syntax"
    );
    assert!(
        read(&store, &grant, json!({"query":"Evidence%"}))
            .events
            .is_empty()
    );
    for id in ["foreign", "protected", "missing"] {
        let request = decode(
            "EvidenceRequest",
            json!({"cursor":"0","limit":10,"event_refs":["a",id],"neighbors":true}),
        )
        .unwrap();
        assert_eq!(
            store.evidence(&grant, &request).unwrap_err().message,
            "selected source event is unavailable"
        );
    }
    let request = decode(
        "EvidenceRequest",
        json!({"cursor":"0","limit":10,"neighbors":true}),
    )
    .unwrap();
    assert!(
        store
            .evidence(&grant, &request)
            .unwrap_err()
            .message
            .contains("event_refs")
    );
}

#[test]
fn cursor_ceiling_and_pagination_preserve_noncontiguous_selection() {
    let (store, grant) = fixture();
    for sequence in 1..=6 {
        store
            .ingest(&event(
                &grant,
                &format!("event-{sequence}"),
                sequence,
                vec![],
            ))
            .unwrap();
    }
    let boundary = read(&store, &grant, json!({"limit":3})).cursor;
    let selected = read(
        &store,
        &grant,
        json!({"event_refs":["event-1","event-3","event-6"],"through_cursor":boundary,"limit":1}),
    );
    assert_eq!(ids(&selected), vec!["event-1"]);
    let next = read(
        &store,
        &grant,
        json!({"event_refs":["event-1","event-3","event-6"],"through_cursor":boundary,"limit":1,"cursor":selected.cursor}),
    );
    assert_eq!(ids(&next), vec!["event-3"]);
    let end = read(
        &store,
        &grant,
        json!({"through_cursor":boundary,"cursor":next.cursor}),
    );
    assert!(end.events.is_empty());
    assert_eq!(end.cursor, next.cursor);
}

#[test]
fn withdrawn_matches_advance_cursor_without_returning_payload_or_frontier() {
    let (store, grant) = fixture();
    let definition: Value = serde_json::from_str::<Value>(include_str!(
        "fixtures/motif-records.json"
    ))
    .unwrap()["definition"]
        .clone();
    let record=store.submit(&grant,&decode("RecordSubmission",json!({"kind":"definition","body":definition,"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"selection-fixture","split":"development","limitations":[]}})).unwrap(),false).unwrap();
    let mut withdrawn = event(&grant, "withdrawn", 1, vec![]);
    withdrawn.provenance.source_refs.push(record.id.clone());
    store.ingest(&withdrawn).unwrap();
    store.ingest(&event(&grant, "visible", 2, vec![])).unwrap();
    store
        .retire(
            &grant,
            &RetireRequest {
                id: record.id,
                expected_version: record.version,
                delete: false,
            },
        )
        .unwrap();
    let first = read(&store, &grant, json!({"query":"literal","limit":1}));
    assert!(first.events.is_empty());
    assert!(first.frontier.is_empty());
    assert_ne!(first.cursor, "0");
    let next = read(
        &store,
        &grant,
        json!({"query":"literal","limit":1,"cursor":first.cursor}),
    );
    assert_eq!(ids(&next), vec!["visible"]);
    let request = decode(
        "EvidenceRequest",
        json!({"cursor":"0","limit":10,"event_refs":["withdrawn"]}),
    )
    .unwrap();
    assert_eq!(
        store.evidence(&grant, &request).unwrap_err().message,
        "selected source event is unavailable"
    );
}
