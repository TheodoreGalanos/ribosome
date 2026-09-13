use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::{LocalHost, RegisteredTool},
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn provenance() -> Value {
    json!({"origin":"synthetic","source_refs":[],"scenario_family":"invocation-mechanism","split":"development","limitations":["Authored policy and test provider; not semantic qualification"]})
}
fn implementation() -> Value {
    json!({"name":"conditional-recipient-policy","version":"1","motifs":[],"format":"instructions","material":"Inspect input. If it says ready, check the recipient. If it says unknown, abstain. Never invent missing inputs.","parameters":{},"required_capabilities":[],"state_assumptions":[],"possible_effects":[],"failure_behavior":"Abstain with missing evidence","evaluation_refs":[],"instruction_contract":{"inputs":[{"name":"input","kind":"artifact_path","required":true}],"outputs":["recipient observation"],"entry_obligations":["Read input"],"exit_obligations":["Report observed status"],"limitations":["Fixture policy"],"discovery_refs":[]}})
}
fn submit(runtime: &Runtime, grant: &Grant, kind: &str, body: Value) -> RecordEnvelope {
    runtime
        .store
        .submit(
            grant,
            &decode(
                "RecordSubmission",
                json!({"kind":kind,"body":body,"provenance":provenance()}),
            )
            .unwrap(),
            false,
        )
        .unwrap()
}
fn fixture() -> (tempfile::TempDir, Runtime, Grant, RecordEnvelope) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("input.txt"), "ready").unwrap();
    let grant = decode("Grant", json!({"id":"owner","scope":{"client":"test","project":"invocation"},"mode":"sandbox","paths":["input.txt"],"tools":["recipient-check"],"profiles":["caretaker"],"budget":{"max_calls":20,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":3,"max_depth":3,"deadline_ms":(now_ms()+60000).to_string()},"context":"recipient","visible_splits":["development"],"allow_export":false})).unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    let runtime = Runtime::new(
        store,
        Box::new(
            LocalHost::new(
                directory.path(),
                BTreeMap::from([(
                    "recipient-check".into(),
                    RegisteredTool {
                        program: "/bin/sh".into(),
                        args: vec!["-c".into(), "test \"$(cat input.txt)\" = ready".into()],
                        timeout_ms: 1000,
                        reads: vec!["input.txt".into()],
                        validates: vec!["input.txt".into()],
                        writes: vec![],
                        code_files: vec![],
                        validated_properties: vec![],
                    },
                )]),
            )
            .unwrap(),
        ),
        directory.path().join("runtime"),
    )
    .unwrap();
    let record = submit(&runtime, &grant, "implementation", implementation());
    (directory, runtime, grant, record)
}
fn request(run: &str, record: &RecordEnvelope) -> AgentRunRequest {
    decode("AgentRunRequest", json!({"run_id":run,"profile":"caretaker","operator":"execute-motif@1","prompt":"Execute the prepared policy for this recipient.","provider":"openai","model":"test-model","invocation":{"implementation":{"id":record.id,"version":"1"},"bindings":{"input":"input.txt"},"recipient_refs":[],"purpose":"experimental"}})).unwrap()
}
fn observe(
    runtime: &Runtime,
    run: &str,
    id: &str,
    method: &str,
    arguments: Value,
) -> ribosome_core::error::Result<Value> {
    let result = runtime.tool_call(
        run,
        "tool.call",
        json!({"call_id":id,"method":method,"arguments":arguments}),
    )?;
    Ok(serde_json::from_str(result["content"].as_str().unwrap()).unwrap())
}

#[test]
fn host_invocation_validates_authority_version_bindings_and_capabilities() {
    let (_dir, runtime, grant, record) = fixture();
    let valid = request("run", &record);
    for (field, value) in [
        ("purpose", json!("production")),
        ("implementation", json!({"id":record.id,"version":"2"})),
        ("bindings", json!({})),
        ("bindings", json!({"input":"outside.txt"})),
        ("bindings", json!({"input":"input.txt","undeclared":true})),
    ] {
        let mut invalid = serde_json::to_value(&valid).unwrap();
        invalid["invocation"][field] = value;
        let invalid: AgentRunRequest = serde_json::from_value(invalid).unwrap();
        assert!(
            runtime.store.begin_run("run", &grant.id, &invalid).is_err(),
            "{field}"
        );
    }
    let mut missing_capability = implementation();
    missing_capability["required_capabilities"] = json!(["ungranted"]);
    let blocked = submit(&runtime, &grant, "implementation", missing_capability);
    assert!(
        runtime
            .store
            .begin_run("run", &grant.id, &request("run", &blocked))
            .is_err()
    );
    let mut apply = grant.clone();
    apply.id = "apply".into();
    apply.mode = Mode::Apply;
    runtime.store.register_grant(&apply).unwrap();
    assert!(runtime.store.begin_run("run", &apply.id, &valid).is_err());
    runtime.store.begin_run("run", &grant.id, &valid).unwrap();
    let material = runtime
        .tool_call("run", "invocation.read", json!({}))
        .unwrap();
    assert_eq!(
        material["implementation"]["material"],
        implementation()["material"]
    );
    assert_eq!(
        material["invocation"],
        serde_json::to_value(valid.invocation).unwrap()
    );
    assert_eq!(
        runtime.store.inspect_run("run").unwrap()["request"]["invocation"],
        material["invocation"]
    );
    assert!(runtime.tool_call("run", "work.request", json!({"subject":"child","profile":"caretaker","operator":"execute-motif@1","reason":"nested","evidence_refs":[]})).unwrap_err().message.contains("not supported"));
}

#[test]
fn prepared_reuse_denies_donor_history_but_can_revisit_fresh_recipient_observations() {
    let (_dir, runtime, grant, record) = fixture();
    let donor: AgentRunRequest = decode("AgentRunRequest",json!({"run_id":"donor","profile":"caretaker","operator":"proofreading@1","prompt":"Read donor","provider":"openai","model":"test-model"})).unwrap();
    runtime.store.begin_run("donor", &grant.id, &donor).unwrap();
    let old = observe(
        &runtime,
        "donor",
        "old",
        "artifact.read",
        json!({"path":"input.txt","offset":0,"length":100}),
    )
    .unwrap();
    let retained = runtime
        .tool_call("donor", "tool.result", json!({"id":"old"}))
        .unwrap();
    let finding = submit(
        &runtime,
        &grant,
        "finding",
        json!({"subject":"donor","observation":"DONOR-SECRET","interpretation":"fixture","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}),
    );
    let event = decode("Event",json!({"id":"donor-event","scope":grant.scope,"run_id":"donor","producer":"source","sequence":"1","kind":"observation","timestamp_ms":now_ms().to_string(),"parents":[],"correlation":"donor","artifacts":[],"payload":{"text":"DONOR-SECRET"},"provenance":provenance()})).unwrap();
    runtime.store.ingest(&event).unwrap();
    for run in ["invoke", "reuse"] {
        let mut req = request(run, &record);
        if run == "reuse" {
            req.invocation = None;
            req.operator = "recombination@1".into();
        }
        runtime.store.begin_run(run, &grant.id, &req).unwrap();
        for (method, args) in [
            ("record.read", json!({"id":finding.id})),
            (
                "artifact.read",
                json!({"path":"input.txt","snapshot_id":old["snapshot_id"],"offset":0,"length":100}),
            ),
            (
                "artifact.read",
                json!({"path":retained["artifact"]["path"],"offset":0,"length":100}),
            ),
            (
                "evidence.read",
                json!({"cursor":"0","limit":10,"event_refs":["donor-event"]}),
            ),
            ("message.inbox", json!({})),
        ] {
            assert!(
                observe(&runtime, run, method, method, args).is_err(),
                "{method}"
            );
        }
        let events = observe(
            &runtime,
            run,
            "events",
            "evidence.read",
            json!({"cursor":"0","limit":100}),
        )
        .unwrap();
        assert!(!events.to_string().contains("DONOR-SECRET"));
        let search = observe(
            &runtime,
            run,
            "search",
            "search.query",
            json!({"query":"","inventory":"evidence","offset":0,"limit":100}),
        )
        .unwrap();
        assert!(!search.to_string().contains("DONOR-SECRET"));
        let fresh = observe(
            &runtime,
            run,
            "fresh",
            "artifact.read",
            json!({"path":"input.txt","offset":0,"length":100}),
        )
        .unwrap();
        let again = observe(
            &runtime,
            run,
            "again",
            "artifact.read",
            json!({"path":"input.txt","snapshot_id":fresh["snapshot_id"],"offset":0,"length":100}),
        )
        .unwrap();
        assert_eq!(again["content"], "ready");
        assert!(
            runtime
                .tool_call(run, "tool.result", json!({"id":"fresh"}))
                .is_ok()
        );
    }
}

#[test]
fn withdrawn_implementation_stops_further_use() {
    let (_dir, runtime, grant, record) = fixture();
    let req = request("run", &record);
    runtime.store.begin_run("run", &grant.id, &req).unwrap();
    runtime
        .store
        .retire(
            &grant,
            &decode(
                "RetireRequest",
                json!({"id":record.id,"expected_version":record.version,"delete":false}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        runtime
            .tool_call("run", "invocation.read", json!({}))
            .is_err()
    );
    assert!(runtime.store.require_active("run").is_err());
    let changed = request("run", &record);
    assert!(runtime.store.begin_run("run", &grant.id, &changed).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_pi_loads_pinned_material_and_generates_recipient_receipts_or_abstains() {
    use ribosome_core::supervisor::{Supervisor, WorkerConfig};
    let (directory, runtime, grant, record) = fixture();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let node=std::process::Command::new("node").args(["--input-type=module","-e","import {fixtureModel} from './tests/integration/model-fixture.mjs'; process.stdout.write(JSON.stringify({node:process.execPath,model:fixtureModel('openai').id}));"]).current_dir(&root).output().unwrap();
    assert!(node.status.success());
    let selected: Value = serde_json::from_slice(&node.stdout).unwrap();
    let config = WorkerConfig {
        node: selected["node"].as_str().unwrap().into(),
        worker: root.join("tests/integration/invocation-worker-fixture.mjs"),
        environment: BTreeMap::new(),
    };
    let supervisor = Supervisor::new(runtime, 1).unwrap();
    for (run, state, expected) in [
        ("supported", "ready", Disposition::Completed),
        ("unsupported", "unknown", Disposition::Abstained),
    ] {
        std::fs::write(directory.path().join("input.txt"), state).unwrap();
        let mut request = request(run, &record);
        request.model = selected["model"].as_str().unwrap().into();
        let (_send, cancel) = tokio::sync::watch::channel(false);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            supervisor.run(&config, &grant.id, request, cancel),
        )
        .await
        .expect("invocation did not finish")
        .unwrap();
        assert_eq!(result.disposition, expected, "{}", result.summary);
    }
    drop(supervisor);
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let successful = store.inspect_run("supported").unwrap();
    assert_eq!(
        successful["request"]["invocation"]["implementation"]["id"],
        record.id
    );
    assert_eq!(successful["effects"].as_array().unwrap().len(), 2);
    assert_eq!(successful["effects"][1]["status"], "succeeded");
    assert_eq!(successful["effects"][1]["action"]["kind"], "check");
    assert!(
        store.inspect_run("unsupported").unwrap()["effects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn production_admission_is_contextual_and_does_not_transfer_to_changed_material() {
    let (_directory, runtime, grant, record) = fixture();
    let admission=decode("RecordSubmission",json!({"kind":"admission","body":{"implementation":{"id":record.id,"version":"1"},"context":grant.context,"decision":"accepted","evaluation_refs":[],"restrictions":[],"authority":"fixture-owner","policy_id":"fixture-policy","supersedes":[]},"provenance":provenance()})).unwrap();
    runtime.store.submit(&grant, &admission, true).unwrap();
    let mut production = request("production", &record);
    production.invocation.as_mut().unwrap().purpose = ImplementationInvocationPurpose::Production;
    runtime
        .store
        .begin_run("production", &grant.id, &production)
        .unwrap();
    let inventory = runtime
        .tool_call(
            "production",
            "search.query",
            json!({"query":"", "inventory":"usable", "offset":0, "limit":10}),
        )
        .unwrap();
    assert_eq!(inventory["records"].as_array().unwrap().len(), 1);
    let mut changed = implementation();
    changed["material"] = json!("Different policy requires fresh evaluation");
    let changed = submit(&runtime, &grant, "implementation", changed);
    let mut denied = request("changed", &changed);
    denied.invocation.as_mut().unwrap().purpose = ImplementationInvocationPurpose::Production;
    assert!(
        runtime
            .store
            .begin_run("changed", &grant.id, &denied)
            .is_err()
    );
    let mut other = grant.clone();
    other.id = "other-context".into();
    other.context = "different-recipient".into();
    runtime.store.register_grant(&other).unwrap();
    production.run_id = "other".into();
    assert!(
        runtime
            .store
            .begin_run("other", &other.id, &production)
            .is_err()
    );
}

#[test]
fn reopening_retains_the_invocation_and_rejects_changed_bindings() {
    let (directory, runtime, grant, _record) = fixture();
    let mut body = implementation();
    body["instruction_contract"]["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"option","kind":"boolean","required":false}));
    let record = submit(&runtime, &grant, "implementation", body);
    let original = request("resume", &record);
    runtime
        .store
        .begin_run("resume", &grant.id, &original)
        .unwrap();
    drop(runtime);
    let runtime = Runtime::new(
        Store::open(directory.path().join("state.db")).unwrap(),
        Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
        directory.path().join("runtime"),
    )
    .unwrap();
    let store = &runtime.store;
    let mut changed = original.clone();
    changed
        .invocation
        .as_mut()
        .unwrap()
        .bindings
        .insert("option".into(), json!(true));
    assert!(
        store
            .begin_run("resume", &grant.id, &changed)
            .unwrap_err()
            .message
            .contains("pinned configuration")
    );
    store.begin_run("resume", &grant.id, &original).unwrap();
    assert_eq!(
        store.inspect_run("resume").unwrap()["request"]["invocation"],
        serde_json::to_value(original.invocation).unwrap()
    );
}

#[test]
fn executed_transplant_links_the_actual_run_and_binding_snapshot() {
    let (_directory, runtime, grant, record) = fixture();
    let req = request("run", &record);
    runtime.store.begin_run("run", &grant.id, &req).unwrap();
    let read = observe(
        &runtime,
        "run",
        "entry",
        "artifact.read",
        json!({"path":"input.txt","offset":0,"length":100}),
    )
    .unwrap();
    let body = json!({"donor":{"id":record.id,"version":"1"},"recipient":"recipient","bindings":{"input":"input.txt"},"adaptations":[],"incompatibilities":[],"checks":[],"fallback":"abstain","invocation_run":"run","recipient_entry_refs":[read["snapshot_id"]],"result_refs":[]});
    let mut invalid = body.clone();
    invalid["bindings"] = json!({"input":"different.txt"});
    let submission = |body| {
        decode(
            "RecordSubmission",
            json!({"kind":"transplant","body":body,"provenance":provenance()}),
        )
        .unwrap()
    };
    assert!(
        runtime
            .store
            .submit(&grant, &submission(invalid), false)
            .unwrap_err()
            .message
            .contains("must match")
    );
    let saved = runtime
        .store
        .submit(&grant, &submission(body), false)
        .unwrap();
    assert_eq!(saved.body["invocation_run"], "run");
}

#[test]
fn prepared_material_can_rebind_paths_without_opening_its_donor_artifact() {
    let (directory, runtime, grant, _record) = fixture();
    let donor:AgentRunRequest=decode("AgentRunRequest",json!({"run_id":"donor","profile":"caretaker","operator":"proofreading@1","prompt":"Read donor","provider":"openai","model":"test-model"})).unwrap();
    runtime.store.begin_run("donor", &grant.id, &donor).unwrap();
    let input = observe(
        &runtime,
        "donor",
        "input",
        "artifact.read",
        json!({"path":"input.txt","offset":0,"length":100}),
    )
    .unwrap();
    let mut lineage = provenance();
    lineage["source_refs"] = json!([input["snapshot_id"]]);
    let material = runtime
        .store
        .submit(
            &grant,
            &decode(
                "RecordSubmission",
                json!({"kind":"implementation","body":implementation(),"provenance":lineage}),
            )
            .unwrap(),
            false,
        )
        .unwrap();
    std::fs::write(directory.path().join("recipient.txt"), "unknown").unwrap();
    let mut recipient = grant.clone();
    recipient.id = "recipient-owner".into();
    recipient.paths = vec!["recipient.txt".into()];
    runtime.store.register_grant(&recipient).unwrap();
    let mut req = request("recipient", &material);
    req.invocation
        .as_mut()
        .unwrap()
        .bindings
        .insert("input".into(), json!("recipient.txt"));
    runtime
        .store
        .begin_run("recipient", &recipient.id, &req)
        .unwrap();
    assert!(
        runtime
            .tool_call("recipient", "invocation.read", json!({}))
            .is_ok()
    );
    assert!(
        observe(
            &runtime,
            "recipient",
            "donor",
            "artifact.read",
            json!({"path":"input.txt","snapshot_id":input["snapshot_id"],"offset":0,"length":100})
        )
        .is_err()
    );
    let fresh = observe(
        &runtime,
        "recipient",
        "fresh",
        "artifact.read",
        json!({"path":"recipient.txt","offset":0,"length":100}),
    )
    .unwrap();
    assert_eq!(fresh["content"], "unknown");
}

#[test]
fn registered_tool_execution_keeps_its_existing_admission_and_effect_path() {
    let (_directory, runtime, grant, _record) = fixture();
    let mut body = implementation();
    body["format"] = json!("registered_tool");
    body["material"] = json!("recipient-check");
    body.as_object_mut().unwrap().remove("instruction_contract");
    let material = submit(&runtime, &grant, "implementation", body);
    runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"admission","body":{"implementation":{"id":material.id,"version":"1"},"context":grant.context,"decision":"accepted","evaluation_refs":[],"restrictions":[],"authority":"fixture-owner","policy_id":"fixture-policy","supersedes":[]},"provenance":provenance()})).unwrap(),true).unwrap();
    let request=decode("AgentRunRequest",json!({"run_id":"registered","profile":"caretaker","operator":"proofreading@1","prompt":"Execute the admitted registered tool","provider":"openai","model":"test-model"})).unwrap();
    runtime
        .store
        .begin_run("registered", &grant.id, &request)
        .unwrap();
    let branch = runtime
        .execute(
            "registered",
            decode("Action", json!({"operation_id":"branch","kind":"branch"})).unwrap(),
        )
        .unwrap();
    let receipt=runtime.execute("registered",decode("Action",json!({"operation_id":"execute","kind":"execute","tool":"recipient-check","implementation":{"id":material.id,"version":"1"},"branch_id":branch.output})).unwrap()).unwrap();
    assert_eq!(
        receipt.status,
        EffectStatus::Succeeded,
        "{}",
        receipt.output
    );
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::ExecutionEstablished)
    );
}
