use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use ribosome_import::{Limits, profile::ImportProfile, reader::normalize, study::project_episode};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

fn observe(runtime: &Runtime, method: &str, arguments: Value) -> ribosome_import::Result<Value> {
    let result = runtime.tool_call(
        "reader",
        "tool.call",
        json!({"call_id":ribosome_core::validation::id(),"method":method,"arguments":arguments}),
    )?;
    Ok(serde_json::from_str(result["content"].as_str().unwrap()).unwrap())
}

#[test]
fn assigned_prefix_excludes_suffix_snapshots_and_withdrawal_removes_imported_evidence() {
    for (profile, operator) in [("curator", "discovery@1"), ("caretaker", "proofreading@1")] {
        check_prefix(profile, operator);
    }
}

fn check_prefix(actor_profile: &str, operator: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/offline-lab");
    let profile = ImportProfile::read(&root.join("profiles/nebius-openhands.yaml")).unwrap();
    let mut raw: Value = serde_json::from_slice(
        &std::fs::read(root.join("fixtures/nebius-row.synthetic.json")).unwrap(),
    )
    .unwrap();
    raw["trajectory"][0]["content"] = json!(format!("PREFIX_EVIDENCE {}", "λ".repeat(70_000)));
    raw["trajectory"][3]["content"] = json!(format!("SUFFIX_EVIDENCE {}", "b".repeat(20_000)));
    let mut episode = normalize(&profile, json!({"source":raw}), &Limits::default()).unwrap();
    episode.id = "episode-0000".into();
    let mut grant: Grant=decode("Grant",json!({"id":"owner","scope":{"client":"test","project":"import"},"mode":"observe","paths":[],"tools":[],"profiles":["curator","caretaker"],
        "budget":{"max_calls":30,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":1,"max_work_items":0,"max_depth":0,"deadline_ms":(now_ms()+60000).to_string()},
        "context":"import-test","visible_splits":["development"],"allow_export":false})).unwrap();
    let (source, events, snapshots) = project_episode(
        "pilot",
        "nebius",
        &episode,
        &grant.scope,
        &Split::Development,
    )
    .unwrap();
    for snapshot in &snapshots {
        let path = directory.path().join(&snapshot.artifact.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &snapshot.content).unwrap();
        grant.paths.push(snapshot.artifact.path.clone());
    }
    let store = Store::open(directory.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    assert_eq!(
        store
            .import_external_episode(&grant, &source, &events, &snapshots)
            .unwrap(),
        4
    );
    assert_eq!(
        store
            .import_external_episode(&grant, &source, &events, &snapshots)
            .unwrap(),
        0
    );
    let prefix = &snapshots[0];
    let suffix = snapshots.last().unwrap();
    let corpus:DiscoveryCorpus=decode("DiscoveryCorpus",json!({"id":"prefix","version":"1","visibility":"online",
        "source_windows":[{"execution":events[0].run_id,"event_refs":[events[0].id,events[1].id],"frontier":{format!("{}/external-import",events[0].run_id):"1"}}],
        "definition_refs":[],"artifacts":[{"artifact":prefix.artifact,"snapshot_id":prefix.snapshot_id}],"dependencies":[],"limitations":[]})).unwrap();
    store.register_discovery_corpus(&grant, &corpus).unwrap();
    let request:AgentRunRequest=decode("AgentRunRequest",json!({"run_id":"reader","profile":actor_profile,"operator":operator,"prompt":"Inspect assigned messages","provider":"openai","model":"fixture",
        "discovery_corpus":{"id":"prefix","version":"1"}})).unwrap();
    let runtime = Runtime::new(
        store,
        Box::new(LocalHost::new(directory.path(), BTreeMap::new()).unwrap()),
        directory.path().join("runtime"),
    )
    .unwrap();
    runtime
        .store
        .begin_run("reader", &grant.id, &request)
        .unwrap();
    let evidence = observe(&runtime, "evidence.read", json!({"cursor":"0","limit":100})).unwrap();
    assert_eq!(evidence["events"].as_array().unwrap().len(), 2);
    assert!(!evidence.to_string().contains("SUFFIX_EVIDENCE"));
    assert!(!evidence.to_string().contains("END_STATE_PATCH_CANARY"));
    let read = |snapshot: &ArtifactChunk| json!({"path":snapshot.artifact.path,"snapshot_id":snapshot.snapshot_id,"offset":0,"length":1000,"required_freshness":"historical"});
    let observed = observe(&runtime, "artifact.read", read(prefix)).unwrap();
    assert!(observed.to_string().contains("PREFIX_EVIDENCE"));
    assert!(observe(&runtime, "artifact.read", read(suffix)).is_err());
    assert!(observe(&runtime, "record.read", json!({"id":source.id})).is_err());
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: "1".into(),
                delete: true,
            },
        )
        .unwrap();
    assert!(observe(&runtime, "artifact.read", read(prefix)).is_err());
    assert!(observe(&runtime, "artifact.read", json!({"path":prefix.artifact.path,"snapshot_id":observed["snapshot_id"],"offset":0,"length":1000,"required_freshness":"historical"})).is_err());
    assert!(
        observe(&runtime, "evidence.read", json!({"cursor":"0","limit":100})).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        runtime
            .store
            .import_external_episode(&grant, &source, &events, &snapshots)
            .is_err()
    );
}

#[test]
fn failed_episode_publication_rolls_back_source_and_events() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/offline-lab");
    let profile = ImportProfile::read(&root.join("profiles/aec-release.yaml")).unwrap();
    let raw: Value = serde_json::from_slice(
        &std::fs::read(root.join("fixtures/aec-joined.synthetic.json")).unwrap(),
    )
    .unwrap();
    let mut episode = normalize(&profile, raw, &Limits::default()).unwrap();
    episode.id = "episode-0000".into();
    let grant: Grant=decode("Grant",json!({"id":"owner","scope":{"client":"test","project":"import"},"mode":"observe","paths":[],"tools":[],"profiles":["curator","caretaker"],
        "budget":{"max_calls":1,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":0,"max_work_items":0,"max_depth":0,"deadline_ms":(now_ms()+60000).to_string()},
        "context":"import-test","visible_splits":["development"],"allow_export":false})).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let (source, mut events, snapshots) =
        project_episode("pilot", "aec", &episode, &grant.scope, &Split::Development).unwrap();
    events[1].kind = "action_receipt".into();
    assert!(
        store
            .import_external_episode(&grant, &source, &events, &snapshots)
            .is_err()
    );
    assert!(store.record(&grant, &source.id).is_err());
    let evidence: EvidenceRequest =
        decode("EvidenceRequest", json!({"cursor":"0","limit":100})).unwrap();
    assert!(store.evidence(&grant, &evidence).unwrap().events.is_empty());
}
