use ribosome_import::{
    Limits,
    decode::decode,
    profile::{Decoder, ImportProfile},
    reader::{normalize, prepare},
};
use serde_json::{Value, json};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/offline-lab")
}
fn profile(name: &str) -> ImportProfile {
    ImportProfile::read(&root().join(format!("profiles/{name}.yaml"))).unwrap()
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(format!("fixtures/{name}"))).unwrap())
        .unwrap()
}

#[test]
fn equivalent_message_encodings_preserve_evidence() {
    let messages = json!([{"role":"user","content":"Check a total"},{"role":"assistant","content":"I need the units."}]);
    let mut results = Vec::new();
    for (encoding, value) in [
        ("array", messages.clone()),
        ("json_string", json!(messages.to_string())),
        (
            "jsonl_string",
            json!(
                messages
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        ),
    ] {
        results.push(
            decode(
                &json!({"messages":value}),
                &Decoder {
                    name: "chat-messages-v1".into(),
                    field: "/messages".into(),
                    value_encoding: encoding.into(),
                    fallback: None,
                },
                &Limits::default(),
            )
            .unwrap(),
        );
    }
    assert_eq!(results[0].messages, results[1].messages);
    assert_eq!(results[0].messages, results[2].messages);
    assert_eq!(results[0].coverage, results[2].coverage);
}

#[test]
fn aec_header_and_fallback_are_explicit() {
    let profile = profile("aec-release");
    let mut row = fixture("aec-joined.synthetic.json");
    let original = normalize(&profile, row.clone(), &Limits::default()).unwrap();
    assert_eq!(original.decoded.messages.len(), 2);
    assert!(original.decoded.fallback_reason.is_none());
    row["artifact"]["trajectory_jsonl"] =
        json!("{\"format\":\"aec-bench-trajectory\",\"version\":1}");
    row["artifact"]["conversation_jsonl"] = json!("{\"role\":\"user\",\"content\":\"fallback\"}");
    let fallback = normalize(&profile, row.clone(), &Limits::default()).unwrap();
    assert_eq!(
        fallback.decoded.fallback_reason.as_deref(),
        Some("header_only")
    );
    row["artifact"]["trajectory_jsonl"] = json!("{broken JSON");
    assert!(
        normalize(&profile, row, &Limits::default())
            .unwrap_err()
            .message
            .contains("malformed embedded JSONL")
    );
    assert!(
        !serde_json::to_string(&original.decoded)
            .unwrap()
            .contains("END_STATE_CANARY")
    );
    assert_eq!(original.annotations["end_state_output"], "END_STATE_CANARY");
}

#[test]
fn tool_evidence_reports_pending_and_unmatched_calls_without_inventing_results() {
    let profile = profile("nebius-openhands");
    let mut row = json!({"source":fixture("nebius-row.synthetic.json")});
    let original = normalize(&profile, row.clone(), &Limits::default()).unwrap();
    assert_eq!(original.decoded.coverage.paired_results, 1);
    assert_eq!(original.decoded.coverage.original_timestamps, 0);
    row["source"]["trajectory"]
        .as_array_mut()
        .unwrap()
        .remove(2);
    row["source"]["trajectory"][1]["tool_calls"][0]["function"]["arguments"] =
        json!("invalid args");
    let pending = normalize(&profile, row.clone(), &Limits::default()).unwrap();
    assert_eq!(pending.decoded.coverage.pending_calls, ["call-1"]);
    assert_eq!(
        pending.decoded.messages[1].tool_calls[0]["arguments"],
        "invalid args"
    );
    row["source"]["trajectory"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"tool","tool_call_id":"unseen","content":"recorded output"}));
    let unmatched = normalize(&profile, row, &Limits::default()).unwrap();
    assert_eq!(unmatched.decoded.coverage.unmatched_results, ["unseen"]);
    assert!(
        !serde_json::to_string(&original.decoded)
            .unwrap()
            .contains("END_STATE_PATCH_CANARY")
    );
}

#[test]
fn unknown_content_stays_owner_side_and_profile_typoes_are_rejected() {
    let profile = profile("nebius-openhands");
    let mut row = json!({"source":fixture("nebius-row.synthetic.json")});
    row["source"]["trajectory"][0]["content"] = json!([{"type":"image_url","image_url":{"url":"OWNER_ONLY_IMAGE"}},{"type":"text","text":"visible text"}]);
    let episode = normalize(&profile, row, &Limits::default()).unwrap();
    let actor = serde_json::to_string(&episode.decoded).unwrap();
    assert!(actor.contains("visible text"));
    assert!(!actor.contains("OWNER_ONLY_IMAGE"));
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("bad.yaml");
    std::fs::write(&invalid, "profile_version: '1'\nprofile_version: '1'\n").unwrap();
    assert!(ImportProfile::read(&invalid).is_err());
}

#[test]
fn local_prepare_is_repeatable_and_reports_conflicting_source_content() {
    let temp = tempfile::tempdir().unwrap();
    let input = root().join("profiles/local-chat.yaml");
    let first = prepare(&input, temp.path(), &Limits::default()).unwrap();
    let second = prepare(&input, temp.path(), &Limits::default()).unwrap();
    assert_eq!(first.episode_ids, second.episode_ids);
    assert!(!first.episode_ids.is_empty());
    let path = temp
        .path()
        .join("episodes")
        .join(format!("{}.json", first.episode_ids[0]));
    let mut episode: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    episode["raw"]["source"]["messages"][0]["content"] = json!("changed source");
    std::fs::write(path, episode.to_string()).unwrap();
    assert!(
        prepare(&input, temp.path(), &Limits::default())
            .unwrap_err()
            .message
            .contains("different content")
    );
}

#[test]
fn joined_tables_report_duplicates_missing_keys_and_disagreeing_identities() {
    let temp = tempfile::tempdir().unwrap();
    let row = fixture("aec-joined.synthetic.json");
    let mut profile = profile("aec-release");
    profile.input.kind = "local".into();
    profile.input.mode = "local_snapshot".into();
    profile.input.dataset = None;
    profile.input.revision = None;
    profile.input.pin_before_read = None;
    profile.input.credential_env = None;
    for table in &mut profile.input.tables {
        table.encoding = "json".into();
        table.path = Some(format!("{}.json", table.name));
        table.config = None;
        table.split = None;
    }
    let input = temp.path().join("profile.yaml");
    let save = |name: &str, value: &Value| {
        std::fs::write(temp.path().join(name), value.to_string()).unwrap();
    };
    save("profile.yaml", &serde_json::to_value(profile).unwrap());
    save("artifacts.json", &json!([row["artifact"], row["artifact"]]));
    save("rollouts.json", &json!([row["rollout"]]));
    save("tasks.json", &json!([row["task"]]));
    let duplicate = prepare(&input, &temp.path().join("duplicate"), &Limits::default()).unwrap();
    assert_eq!(duplicate.duplicate_rows, 1);
    assert_eq!(duplicate.episode_ids.len(), 1);
    assert!(duplicate.quarantined.is_empty());
    save("rollouts.json", &json!([row["rollout"], row["rollout"]]));
    assert!(
        prepare(&input, &temp.path().join("dimension"), &Limits::default())
            .unwrap_err()
            .message
            .contains("duplicate dimension key")
    );
    save("rollouts.json", &json!([]));
    let missing = prepare(&input, &temp.path().join("missing"), &Limits::default()).unwrap();
    assert!(missing.episode_ids.is_empty());
    assert!(
        missing.quarantined[0]
            .reason
            .contains("missing rollouts join key")
    );
    let mut conflict = row["rollout"].clone();
    conflict["task_id"] = json!("different-task");
    save("rollouts.json", &json!([conflict]));
    let conflict = prepare(&input, &temp.path().join("conflict"), &Limits::default()).unwrap();
    assert!(
        conflict.quarantined[0]
            .reason
            .contains("disagree on task_id")
    );
}
