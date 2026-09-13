use ribosome_core::{
    contracts::*,
    effects::Runtime,
    error::Result,
    host::LocalHost,
    store::Store,
    validation::{decode, id, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn body(kind: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("fixtures/motif-records.json")).unwrap()[kind]
        .clone()
}
fn submission(kind: &str, body: Value) -> RecordSubmission {
    decode("RecordSubmission", json!({"kind":kind,"body":body,"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"contract-fixture","split":"development","limitations":["Mechanical record validation; no semantic assessment"]}})).unwrap()
}
fn fixture() -> (tempfile::TempDir, Runtime, Grant) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.db")).unwrap();
    let grant: Grant = decode("Grant", json!({"id":"grant","scope":{"client":"test","project":"motifs"},"mode":"observe","paths":[],"tools":[],"profiles":["curator"],"budget":{"max_calls":10,"max_tokens":"100000","max_cost_microusd":"100000","max_actions":5,"max_work_items":5,"max_depth":3,"deadline_ms":(now_ms()+60000).to_string()},"context":"motifs","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(dir.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), dir.path().join("runtime")).unwrap();
    let request = decode("AgentRunRequest", json!({"run_id":"curator","profile":"curator","operator":"discovery@1","prompt":"Investigate permitted execution evidence","provider":"openai","model":"test-model"})).unwrap();
    runtime
        .store
        .begin_run("curator", &grant.id, &request)
        .unwrap();
    for (id, sequence, parents) in [("input", "1", vec![]), ("check", "2", vec!["input"])] {
        let event = decode("Event", json!({"id":id,"scope":grant.scope,"run_id":"donor","producer":"worker","sequence":sequence,"kind":"fixture_observation","timestamp_ms":now_ms().to_string(),"parents":parents,"correlation":"donor","artifacts":[],"payload":{"label":id},"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"contract-fixture","split":"development","limitations":["Authored fixture; not execution evidence"]}})).unwrap();
        runtime.store.ingest(&event).unwrap();
    }
    (dir, runtime, grant)
}
fn observe(runtime: &Runtime, method: &str, arguments: Value) -> Result<Value> {
    let result = runtime.tool_call(
        "curator",
        "tool.call",
        json!({"call_id":id(),"method":method,"arguments":arguments}),
    )?;
    Ok(serde_json::from_str(result["content"].as_str().unwrap()).unwrap())
}
fn definition(runtime: &Runtime, grant: &Grant) -> RecordEnvelope {
    runtime
        .store
        .submit(grant, &submission("definition", body("definition")), false)
        .unwrap()
}
fn occurrence(definition: &RecordEnvelope) -> Value {
    let mut value = body("occurrence");
    value["definition"]["id"] = json!(definition.id);
    value
}
fn discovery(definition: &RecordEnvelope) -> Value {
    let mut value = body("discovery");
    value["definition_refs"][0]["id"] = json!(definition.id);
    value
}
fn read_sources(runtime: &Runtime, definition: &RecordEnvelope) {
    observe(runtime, "record.read", json!({"id":definition.id})).unwrap();
    observe(
        runtime,
        "evidence.read",
        json!({"cursor":"0","limit":10,"run_id":"donor"}),
    )
    .unwrap();
}

#[test]
fn legacy_records_stay_readable_and_functional_relations_pin_definition_versions() {
    let (_dir, runtime, grant) = fixture();
    let mut legacy = body("definition");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("functional_contract");
    let saved = runtime
        .store
        .submit(&grant, &submission("definition", legacy), false)
        .unwrap();
    assert!(
        !runtime
            .store
            .record(&grant, &saved.id)
            .unwrap()
            .body
            .contains_key("functional_contract")
    );
    let mut related = body("definition");
    related["functional_contract"]["relations"] = json!([{"kind":"specializes","definition":{"id":saved.id,"version":"1"},"explanation":"A narrower entry condition"}]);
    assert_eq!(
        runtime
            .store
            .submit(&grant, &submission("definition", related.clone()), false)
            .unwrap_err()
            .code,
        -32002
    );
    related["functional_contract"]["relations"][0]["definition"]["version"] = json!("2");
    let child = runtime
        .store
        .submit(&grant, &submission("definition", related), false)
        .unwrap();
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                delete: false,
                id: saved.id,
                expected_version: saved.version,
            },
        )
        .unwrap();
    assert!(
        runtime.store.record(&grant, &child.id).is_err(),
        "nested relation participates in source withdrawal"
    );
    let mut duplicate = body("definition");
    duplicate["functional_contract"]["roles"][1]["name"] = json!("input");
    assert!(
        runtime
            .store
            .submit(&grant, &submission("definition", duplicate), false)
            .unwrap_err()
            .message
            .contains("unique")
    );
}

#[test]
fn grounded_occurrences_validate_roles_boundaries_and_reported_dependencies() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    let valid = occurrence(&definition);
    runtime
        .store
        .submit(&grant, &submission("occurrence", valid.clone()), false)
        .unwrap();
    let cases = [
        ("/grounding/role_bindings/0/role", json!("invented"), "role"),
        (
            "/grounding/role_bindings/0/event_refs",
            json!(["definition"]),
            "reference",
        ),
        (
            "/grounding/dependency_evidence/0/source_event_ref",
            json!("check"),
            "dependency",
        ),
        (
            "/grounding/local_outcome/evidence_refs",
            json!([]),
            "local outcome",
        ),
        (
            "/grounding/annotator/operator",
            json!("other@1"),
            "operator",
        ),
        ("/definition/version", json!("1"), "version"),
        ("/frontier/donor~1worker", json!("1"), "frontier"),
    ];
    for (pointer, replacement, message) in cases {
        let mut changed = valid.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let error = runtime
            .store
            .submit(&grant, &submission("occurrence", changed), false)
            .unwrap_err();
        assert!(
            error.message.contains(message),
            "{pointer}: {}",
            error.message
        );
    }
    let mut inferred = valid;
    inferred["grounding"]["dependency_evidence"][0]["source_event_ref"] = json!("check");
    inferred["grounding"]["dependency_evidence"][0]["target_event_ref"] = json!("input");
    assert!(
        runtime
            .store
            .submit(&grant, &submission("occurrence", inferred.clone()), false)
            .unwrap_err()
            .message
            .contains("source_reported")
    );
    inferred["grounding"]["dependency_evidence"][0]["basis"] = json!("inferred");
    let saved = runtime
        .store
        .submit(&grant, &submission("occurrence", inferred), false)
        .unwrap();
    assert_eq!(
        saved.body["grounding"]["dependency_evidence"][0]["basis"],
        "inferred"
    );
}

#[test]
fn discovery_cannot_omit_functional_contract_or_grounding_to_use_the_legacy_path() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    read_sources(&runtime, &definition);
    for (kind, field, mut value) in [
        ("definition", "functional_contract", body("definition")),
        ("occurrence", "grounding", occurrence(&definition)),
    ] {
        value.as_object_mut().unwrap().remove(field);
        let error = observe(
            &runtime,
            "record.submit",
            serde_json::to_value(submission(kind, value)).unwrap(),
        )
        .unwrap_err();
        assert!(error.message.contains(field), "{}", error.message);
    }
}

#[test]
fn host_supplies_discovery_identity_and_frontiers_without_selecting_extra_evidence() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    read_sources(&runtime, &definition);
    let mut value = occurrence(&definition);
    value.as_object_mut().unwrap().remove("frontier");
    value.as_object_mut().unwrap().remove("operator");
    value["grounding"]
        .as_object_mut()
        .unwrap()
        .remove("annotator");
    let saved = observe(
        &runtime,
        "record.submit",
        serde_json::to_value(submission("occurrence", value)).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["body"]["frontier"], json!({"donor/worker":"2"}));
    assert_eq!(
        saved["body"]["grounding"]["annotator"],
        json!({"run_id":"curator","operator":"discovery@1"})
    );
    let mut value = discovery(&definition);
    value.as_object_mut().unwrap().remove("run_refs");
    value["source_windows"][0]
        .as_object_mut()
        .unwrap()
        .remove("frontier");
    let saved = observe(
        &runtime,
        "record.submit",
        serde_json::to_value(submission("discovery", value)).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["body"]["run_refs"], json!(["curator"]));
    assert_eq!(
        saved["body"]["source_windows"][0]["event_refs"],
        json!(["input", "check"])
    );
    assert_eq!(
        saved["body"]["source_windows"][0]["frontier"],
        json!({"donor/worker":"2"})
    );
}

#[test]
fn curator_must_retrieve_cited_evidence_and_cannot_self_certify_online_visibility() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    let value = occurrence(&definition);
    let submit = |value| {
        observe(
            &runtime,
            "record.submit",
            serde_json::to_value(submission("occurrence", value)).unwrap(),
        )
    };
    assert!(
        submit(value.clone())
            .unwrap_err()
            .message
            .contains("not retrieved")
    );
    read_sources(&runtime, &definition);
    let saved = submit(value.clone()).unwrap();
    assert_eq!(
        saved["body"]["grounding"]["local_outcome"]["state"],
        "satisfied"
    );
    let mut online = value;
    online["grounding"]["recognition_visibility"] = json!("online");
    assert!(
        submit(online)
            .unwrap_err()
            .message
            .contains("source-prefix assignment")
    );
    let saved_discovery = observe(
        &runtime,
        "record.submit",
        serde_json::to_value(submission("discovery", discovery(&definition))).unwrap(),
    )
    .unwrap();
    assert_eq!(saved_discovery["body"]["run_refs"], json!(["curator"]));
}

#[test]
fn discovery_retains_empty_results_and_rejects_false_boundaries_and_attribution() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    let mut empty = discovery(&definition);
    empty["hypotheses"] = json!([]);
    empty["definition_refs"] = json!([]);
    empty["decision"] = json!("no_motif");
    let saved = runtime
        .store
        .submit(&grant, &submission("discovery", empty.clone()), false)
        .unwrap();
    assert_eq!(saved.body["decision"], "no_motif");
    let mut update = submission("discovery", empty);
    update.id = Some(saved.id);
    update.expected_version = Some(saved.version);
    assert!(
        runtime
            .store
            .submit(&grant, &update, false)
            .unwrap_err()
            .message
            .contains("immutable")
    );
    let cases = [
        (
            "/source_windows/0/event_refs",
            json!(["input"]),
            "source window",
        ),
        (
            "/source_windows/0/execution",
            json!("another-run"),
            "different execution",
        ),
        ("/hypotheses/0/support_refs", json!([]), "supporting"),
        ("/definition_refs", json!([]), "pinned definition"),
        (
            "/occurrence_refs",
            json!([definition.id]),
            "occurrence records",
        ),
        ("/run_refs", json!(["missing"]), "run"),
    ];
    for (pointer, replacement, message) in cases {
        let mut changed = discovery(&definition);
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let error = runtime
            .store
            .submit(&grant, &submission("discovery", changed), false)
            .unwrap_err();
        assert!(
            error.message.contains(message),
            "{pointer}: {}",
            error.message
        );
    }
}

#[test]
fn nested_contrasts_cannot_escape_scope_split_or_retirement() {
    let (_dir, runtime, grant) = fixture();
    let definition = definition(&runtime, &grant);
    let mut other = grant.clone();
    other.id = "foreign".into();
    other.scope.project = "foreign".into();
    runtime.store.register_grant(&other).unwrap();
    let foreign = runtime
        .store
        .submit(&other, &submission("definition", body("definition")), false)
        .unwrap();
    let mut value = discovery(&definition);
    value["hypotheses"][0]["alternatives"][0]["evidence_refs"] = json!([foreign.id]);
    assert!(
        runtime
            .store
            .submit(&grant, &submission("discovery", value), false)
            .is_err()
    );
    let mut protected = grant.clone();
    protected.id = "protected".into();
    protected.visible_splits.push(Split::Holdout);
    runtime.store.register_grant(&protected).unwrap();
    let mut source = submission("definition", body("definition"));
    source.provenance.split = Split::Holdout;
    let secret = runtime.store.submit(&protected, &source, false).unwrap();
    let mut value = discovery(&definition);
    value["hypotheses"][0]["contradiction_refs"] = json!([secret.id]);
    assert!(
        runtime
            .store
            .submit(&protected, &submission("discovery", value), false)
            .is_err()
    );
    let saved = runtime
        .store
        .submit(
            &grant,
            &submission("discovery", discovery(&definition)),
            false,
        )
        .unwrap();
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                delete: false,
                id: definition.id,
                expected_version: definition.version,
            },
        )
        .unwrap();
    assert!(runtime.store.record(&grant, &saved.id).is_err());
}
