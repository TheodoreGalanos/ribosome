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

fn request(run: &str, corpus: Option<VersionRef>) -> AgentRunRequest {
    let mut request: AgentRunRequest = decode("AgentRunRequest",json!({"run_id":run,"profile":"curator","operator":"discovery@1","prompt":"Investigate the assigned source evidence","provider":"openai","model":"test-model"})).unwrap();
    request.discovery_corpus = corpus;
    request
}
fn observe(runtime: &Runtime, run: &str, method: &str, arguments: Value) -> Result<Value> {
    let result = runtime.tool_call(
        run,
        "tool.call",
        json!({"call_id":id(),"method":method,"arguments":arguments}),
    )?;
    Ok(serde_json::from_str(result["content"].as_str().unwrap()).unwrap())
}
fn submission(kind: &str, body: Value) -> Value {
    json!({"kind":kind,"body":body,"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"corpus-boundary-fixture","split":"development","limitations":["Mechanical fixture, not semantic discovery evidence"]}})
}
fn definition() -> Value {
    serde_json::from_str::<Value>(include_str!("fixtures/motif-records.json")).unwrap()["definition"].clone()
}

struct Fixture {
    directory: tempfile::TempDir,
    runtime: Runtime,
    grant: Grant,
    corpus: DiscoveryCorpus,
    hidden_definition: RecordEnvelope,
    future_snapshot: String,
    future_result_path: String,
}
fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("input.txt"), "PREFIX-ARTIFACT").unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let grant: Grant=decode("Grant",json!({"id":"owner","scope":{"client":"test","project":"corpus"},"mode":"apply","paths":["input.txt"],"tools":["unused-check"],"profiles":["curator","caretaker"],"budget":{"max_calls":30,"max_tokens":"3000000","max_cost_microusd":"1000000","max_actions":3,"max_work_items":3,"max_depth":3,"deadline_ms":(now_ms()+60000).to_string()},"context":"corpus","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    let runtime = Runtime::new(
        store,
        Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
        directory.path().join("runtime"),
    )
    .unwrap();
    runtime
        .store
        .begin_run("source-owner", &grant.id, &request("source-owner", None))
        .unwrap();
    let original = observe(
        &runtime,
        "source-owner",
        "artifact.read",
        json!({"path":"input.txt","offset":0,"length":100,"required_freshness":"historical"}),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("input.txt"),
        "FUTURE-ARTIFACT-CONTENT",
    )
    .unwrap();
    let future=runtime.tool_call("source-owner","tool.call",json!({"call_id":"future-read","method":"artifact.read","arguments":{"path":"input.txt","offset":0,"length":100,"required_freshness":"historical"}})).unwrap();
    let future_chunk: Value = serde_json::from_str(future["content"].as_str().unwrap()).unwrap();
    let mut hidden = definition();
    hidden["intent"] = json!("FUTURE-RECORD-CONTENT");
    let hidden_definition = runtime
        .store
        .submit(
            &grant,
            &decode("RecordSubmission", submission("definition", hidden)).unwrap(),
            false,
        )
        .unwrap();
    for (event_id, sequence, text, parents) in [
        ("past", "1", "PREFIX-EVENT", vec![]),
        (
            "future",
            "2",
            "FUTURE-EVENT-CONTENT contradiction",
            vec!["past"],
        ),
    ] {
        let event=decode("Event",json!({"id":event_id,"scope":grant.scope,"run_id":"donor","producer":"worker","sequence":sequence,"kind":"observation","timestamp_ms":now_ms().to_string(),"parents":parents,"correlation":"donor","artifacts":[original["artifact"]],"payload":{"text":text},"provenance":{"origin":"synthetic","source_refs":[],"scenario_family":"corpus-boundary-fixture","split":"development","limitations":["Authored source prefix fixture"]}})).unwrap();
        runtime.store.ingest(&event).unwrap();
    }
    let corpus=decode("DiscoveryCorpus",json!({"id":"selected","version":"1","visibility":"online","source_windows":[{"execution":"donor","event_refs":["past"],"frontier":{"donor/worker":"1"}}],"definition_refs":[],"artifacts":[{"artifact":original["artifact"],"snapshot_id":original["snapshot_id"]}],"dependencies":[],"limitations":["This fixture tests delivery bounds, not model reasoning"]})).unwrap();
    runtime
        .store
        .register_discovery_corpus(&grant, &corpus)
        .unwrap();
    runtime
        .store
        .begin_run(
            "curator",
            &grant.id,
            &request(
                "curator",
                Some(VersionRef {
                    id: "selected".into(),
                    version: "1".into(),
                }),
            ),
        )
        .unwrap();
    Fixture {
        directory,
        runtime,
        grant,
        corpus,
        hidden_definition,
        future_snapshot: future_chunk["snapshot_id"].as_str().unwrap().into(),
        future_result_path: future["artifact"]["path"].as_str().unwrap().into(),
    }
}

#[test]
fn corpus_registration_is_immutable_scoped_and_survives_reopen() {
    let f = fixture();
    f.runtime
        .store
        .register_discovery_corpus(&f.grant, &f.corpus)
        .unwrap();
    let mut changed = f.corpus.clone();
    changed.source_windows[0].event_refs.push("future".into());
    assert_eq!(
        f.runtime
            .store
            .register_discovery_corpus(&f.grant, &changed)
            .unwrap_err()
            .code,
        -32002
    );
    changed.version = "2".into();
    assert!(
        f.runtime
            .store
            .register_discovery_corpus(&f.grant, &changed)
            .unwrap_err()
            .message
            .contains("frontier")
    );
    changed.source_windows[0]
        .frontier
        .insert("donor/worker".into(), json!("2"));
    f.runtime
        .store
        .register_discovery_corpus(&f.grant, &changed)
        .unwrap();
    let mut foreign = f.grant.clone();
    foreign.scope.project = "foreign".into();
    assert!(
        f.runtime
            .store
            .discovery_corpus(
                &foreign,
                &VersionRef {
                    id: "selected".into(),
                    version: "1".into()
                }
            )
            .is_err()
    );
    let path = f.directory.path().join("state.db");
    drop(f.runtime);
    let reopened = Store::open(path).unwrap();
    assert_eq!(
        reopened
            .discovery_corpus(
                &f.grant,
                &VersionRef {
                    id: "selected".into(),
                    version: "1".into()
                }
            )
            .unwrap(),
        f.corpus
    );
}

#[test]
fn assigned_reads_exclude_future_events_records_artifacts_and_other_runs_results() {
    let f = fixture();
    let runtime = &f.runtime;
    let effective = runtime.store.run_grant("curator").unwrap();
    assert_eq!(effective.mode, Mode::Observe);
    assert!(effective.tools.is_empty());
    let corpus = observe(runtime, "curator", "evidence.corpus", json!({})).unwrap();
    assert_eq!(corpus["visibility"], "online");
    let evidence = observe(
        runtime,
        "curator",
        "evidence.read",
        json!({"cursor":"0","limit":100}),
    )
    .unwrap();
    assert_eq!(evidence["events"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["events"][0]["id"], "past");
    let neighbors = observe(
        runtime,
        "curator",
        "evidence.read",
        json!({"cursor":"0","limit":100,"event_refs":["past"],"neighbors":true}),
    )
    .unwrap();
    assert_eq!(neighbors["events"].as_array().unwrap().len(), 1);
    let queried = observe(
        runtime,
        "curator",
        "evidence.read",
        json!({"cursor":"0","limit":100,"query":"contradiction"}),
    )
    .unwrap();
    assert!(queried["events"].as_array().unwrap().is_empty());
    assert!(
        observe(
            runtime,
            "curator",
            "evidence.read",
            json!({"cursor":"0","limit":100,"event_refs":["future"]})
        )
        .is_err()
    );
    assert!(
        observe(
            runtime,
            "curator",
            "record.read",
            json!({"id":f.hidden_definition.id})
        )
        .is_err()
    );
    let records = observe(
        runtime,
        "curator",
        "search.query",
        json!({"query":"","inventory":"evidence","limit":100,"offset":0}),
    )
    .unwrap();
    assert!(records["records"].as_array().unwrap().is_empty());
    assert_eq!(records["next_offset"], 0);
    let pinned = &f.corpus.artifacts[0];
    let historical=observe(runtime,"curator","artifact.read",json!({"path":"input.txt","snapshot_id":pinned.snapshot_id,"offset":0,"length":100,"required_freshness":"historical"})).unwrap();
    assert_eq!(historical["content"], "PREFIX-ARTIFACT");
    for arguments in [
        json!({"path":"input.txt","offset":0,"length":100}),
        json!({"path":"input.txt","snapshot_id":f.future_snapshot,"offset":0,"length":100,"required_freshness":"historical"}),
        json!({"path":f.future_result_path,"offset":0,"length":100}),
    ] {
        assert!(observe(runtime, "curator", "artifact.read", arguments).is_err());
    }
    for (method, args) in [
        ("message.inbox", json!({})),
        ("artifact.validity", json!({"path":"input.txt"})),
        (
            "action.execute",
            json!({"operation_id":"forbidden","kind":"check","tool":"unused-check"}),
        ),
        (
            "record.retire",
            json!({"id":f.hidden_definition.id,"expected_version":f.hidden_definition.version,"delete":true}),
        ),
    ] {
        let error = observe(runtime, "curator", method, args).unwrap_err();
        assert!(
            error.message.contains("frozen evidence"),
            "{method}: {}",
            error.message
        );
    }
    assert!(
        runtime
            .store
            .record(&f.grant, &f.hidden_definition.id)
            .is_ok()
    );
    assert_eq!(
        std::fs::read_to_string(f.directory.path().join("input.txt")).unwrap(),
        "FUTURE-ARTIFACT-CONTENT"
    );
}

#[test]
fn assignment_metadata_does_not_count_as_retrieved_evidence_and_online_labels_match_authority() {
    let f = fixture();
    let runtime = &f.runtime;
    observe(runtime, "curator", "evidence.corpus", json!({})).unwrap();
    let discovery = json!({"corpus":{"id":"selected","version":"1"},"source_windows":f.corpus.source_windows,"hypotheses":[],"definition_refs":[],"occurrence_refs":[],"decision":"no_motif","open_questions":["The observed prefix is insufficient"],"run_refs":["curator"]});
    assert!(
        observe(
            runtime,
            "curator",
            "record.submit",
            submission("discovery", discovery.clone())
        )
        .unwrap_err()
        .message
        .contains("not retrieved")
    );
    observe(
        runtime,
        "curator",
        "evidence.read",
        json!({"cursor":"0","limit":10}),
    )
    .unwrap();
    observe(
        runtime,
        "curator",
        "record.submit",
        submission("discovery", discovery.clone()),
    )
    .unwrap();
    let mut altered = discovery;
    altered["corpus"]["version"] = json!("2");
    assert!(
        observe(
            runtime,
            "curator",
            "record.submit",
            submission("discovery", altered)
        )
        .unwrap_err()
        .message
        .contains("assignment")
    );
    let created = observe(
        runtime,
        "curator",
        "record.submit",
        submission("definition", definition()),
    )
    .unwrap();
    observe(
        runtime,
        "curator",
        "record.read",
        json!({"id":created["id"]}),
    )
    .unwrap();
    let occurrence = json!({"definition":{"id":created["id"],"version":"2"},"execution":"donor","event_refs":["past"],"artifacts":[],"frontier":{"donor/worker":"1"},"recognition":"tentative","obligations":[],"assumptions":["Only a prefix is available"],"operator":"discovery@1","grounding":{"role_bindings":[{"role":"input","event_refs":["past"]}],"incoming_context_refs":[],"dependency_evidence":[],"local_outcome":{"state":"unresolved","evidence_refs":[],"limitations":["No local outcome is visible"]},"annotator":{"run_id":"curator","operator":"discovery@1"},"recognition_visibility":"online"}});
    observe(
        runtime,
        "curator",
        "record.submit",
        submission("occurrence", occurrence.clone()),
    )
    .unwrap();
    let mut retrospective = occurrence;
    retrospective["grounding"]["recognition_visibility"] = json!("retrospective");
    assert!(
        observe(
            runtime,
            "curator",
            "record.submit",
            submission("occurrence", retrospective)
        )
        .unwrap_err()
        .message
        .contains("visibility")
    );
}

#[test]
fn child_and_resumed_runs_cannot_drop_or_replace_their_corpus() {
    let f = fixture();
    let runtime = &f.runtime;
    let work: WorkItem=decode("WorkItem",observe(runtime,"curator","work.request",json!({"subject":"contrast","profile":"curator","operator":"contrast-motif@1","reason":"Check the evidence boundary","evidence_refs":["past"]})).unwrap()).unwrap();
    assert!(
        runtime
            .store
            .begin_run(&work.id, &f.grant.id, &request(&work.id, None))
            .unwrap_err()
            .message
            .contains("parent")
    );
    let corpus = Some(VersionRef {
        id: "selected".into(),
        version: "1".into(),
    });
    runtime
        .store
        .begin_run(&work.id, &f.grant.id, &request(&work.id, corpus.clone()))
        .unwrap();
    assert_eq!(
        runtime.store.run_grant(&work.id).unwrap().discovery_corpus,
        corpus
    );
    assert!(
        observe(
            runtime,
            &work.id,
            "record.read",
            json!({"id":f.hidden_definition.id})
        )
        .is_err()
    );
    runtime
        .store
        .finish_run(
            "curator",
            &AgentResult {
                disposition: Disposition::Interrupted,
                summary: "saved".into(),
            },
        )
        .unwrap();
    assert!(
        runtime
            .store
            .begin_run("curator", &f.grant.id, &request("curator", None))
            .unwrap_err()
            .message
            .contains("pinned configuration")
    );
    runtime
        .store
        .begin_run("curator", &f.grant.id, &request("curator", corpus))
        .unwrap();
    let evidence = observe(
        runtime,
        "curator",
        "evidence.read",
        json!({"cursor":"0","limit":100}),
    )
    .unwrap();
    assert_eq!(evidence["events"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_pi_parent_and_contrast_child_keep_the_same_corpus_across_parking() {
    use ribosome_core::supervisor::{Supervisor, WorkerConfig};
    let f = fixture();
    std::fs::write(f.directory.path().join("worker-settings.json"),serde_json::to_vec(&json!({"original_snapshot":f.corpus.artifacts[0].snapshot_id,"future_snapshot":f.future_snapshot,"hidden_definition":f.hidden_definition.id})).unwrap()).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let node=std::process::Command::new("node").args(["--input-type=module","-e","import {fixtureModel} from './tests/integration/model-fixture.mjs'; process.stdout.write(JSON.stringify({node:process.execPath,model:fixtureModel('openai').id}));"]).current_dir(&root).output().unwrap();
    assert!(node.status.success());
    let selected: Value = serde_json::from_slice(&node.stdout).unwrap();
    let config = WorkerConfig {
        node: selected["node"].as_str().unwrap().into(),
        worker: root.join("tests/integration/corpus-worker-fixture.mjs"),
        environment: BTreeMap::from([(
            "RIBOSOME_CORPUS_FIXTURE".into(),
            f.directory.path().to_string_lossy().into_owned(),
        )]),
    };
    let mut request = request(
        "parent",
        Some(VersionRef {
            id: "selected".into(),
            version: "1".into(),
        }),
    );
    request.model = selected["model"].as_str().unwrap().into();
    let supervisor = Supervisor::new(f.runtime, 1).unwrap();
    let (_send, cancel) = tokio::sync::watch::channel(false);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        supervisor.run(&config, &f.grant.id, request, cancel),
    )
    .await
    .expect("assigned parent/child did not finish")
    .unwrap();
    assert_eq!(
        result.disposition,
        Disposition::Completed,
        "{}",
        result.summary
    );
    let trace = std::fs::read_to_string(f.directory.path().join("provider-views.jsonl")).unwrap();
    for marker in [
        "FUTURE-EVENT-CONTENT",
        "FUTURE-ARTIFACT-CONTENT",
        "FUTURE-RECORD-CONTENT",
    ] {
        assert!(
            !trace.contains(marker),
            "{marker} entered an actual provider request"
        );
    }
    assert!(trace.contains("PREFIX-EVENT"));
    assert!(trace.contains("PREFIX-ARTIFACT"));
    let events: Vec<Value> = trace
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "start")
        .collect();
    assert_eq!(starts.len(), 3);
    assert_eq!(starts[2]["resumed"], true);
    assert!(
        starts
            .iter()
            .all(|event| event["corpus"] == json!({"id":"selected","version":"1"}))
    );
    for view in events
        .iter()
        .filter(|event| event["type"] == "provider" && event["run_id"] != "parent")
    {
        let context = serde_json::to_string(&view["context"]).unwrap();
        assert!(!context.contains("PREFIX-ARTIFACT"));
        assert!(
            !context.contains("PREFIX-EVENT"),
            "contrast context copied parent observations without retrieval"
        );
    }
    let runtime = supervisor.runtime();
    let runtime = runtime.lock().await;
    assert_eq!(
        runtime
            .store
            .root_budget_status(&f.grant)
            .unwrap()
            .usage
            .model_calls,
        14
    );
    assert_eq!(
        runtime.store.inspect_run("parent").unwrap()["timings"]["spans"]["child_wait"]["samples"],
        1
    );
}

#[test]
fn equivalent_artifact_snapshots_remain_readable_within_the_assigned_version() {
    let f = fixture();
    let copy=observe(&f.runtime,"source-owner","artifact.read",json!({"path":"input.txt","snapshot_id":f.corpus.artifacts[0].snapshot_id,"required_freshness":"historical","offset":0,"length":100})).unwrap();
    let result = observe(
        &f.runtime,
        "curator",
        "artifact.read",
        json!({"path":"input.txt","snapshot_id":copy["snapshot_id"],"required_freshness":"historical","offset":0,"length":100}),
    ).unwrap();
    assert_eq!(result["content"], "PREFIX-ARTIFACT");
}

#[test]
fn corpus_assignment_does_not_keep_a_deleted_artifact_available() {
    let f = fixture();
    let arguments = json!({"path":"input.txt","snapshot_id":f.corpus.artifacts[0].snapshot_id,"required_freshness":"historical","offset":0,"length":100});
    assert_eq!(
        observe(&f.runtime, "curator", "artifact.read", arguments.clone()).unwrap()["content"],
        "PREFIX-ARTIFACT"
    );
    std::fs::remove_file(f.directory.path().join("input.txt")).unwrap();
    assert!(observe(&f.runtime, "curator", "artifact.read", arguments).is_err());
}
