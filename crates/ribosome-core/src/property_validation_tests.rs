use super::*;

fn binding(record: &RecordEnvelope) -> PropertyBinding {
    PropertyBinding {
        obligation: VersionRef {
            id: record.id.clone(),
            version: record.version.clone(),
        },
        path: "artifact.txt".into(),
    }
}

fn configure(
    mut runtime: Runtime,
    root: &std::path::Path,
    properties: &[RecordEnvelope],
) -> Runtime {
    let mut tool = recovery_tool();
    tool.validated_properties = properties.iter().map(binding).collect();
    drop(runtime.host);
    runtime.host = std::sync::Arc::new(
        LocalHost::new(root, BTreeMap::from([("check".into(), tool)])).unwrap(),
    );
    runtime
}

fn check(runtime: &Runtime, run: &str, operation: &str, tool: &str) -> ActionReceipt {
    runtime
        .execute(
            run,
            decode(
                "Action",
                json!({"kind":"check","operation_id":operation,"tool":tool}),
            )
            .unwrap(),
        )
        .unwrap()
}

fn validity(runtime: &Runtime, grant: &Grant) -> ArtifactValidity {
    runtime
        .artifact_validity(
            grant,
            &ArtifactValidityRequest {
                path: "artifact.txt".into(),
            },
        )
        .unwrap()
}

fn fixture() -> (tempfile::TempDir, Runtime, Grant, RecordEnvelope) {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    let property = runtime
        .store
        .record(
            &grant,
            &runtime
                .store
                .obligation_ids(&grant.scope, "artifact.txt")
                .unwrap()[0],
        )
        .unwrap();
    dependency(&runtime, &grant, "input.txt", "artifact.txt");
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    (dir, runtime, grant, property)
}

#[test]
fn partial_validation_preserves_unproven_properties_and_complete_validation_clears_only_the_target()
{
    let (dir, mut runtime, grant, first) = fixture();
    let second = property_record(
        &runtime.store,
        &grant,
        "Additional host criterion",
        &["artifact.txt"],
    );
    let apply = checked_application(&runtime, &grant, "apply");
    let receipt = runtime.execute("run", apply).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert!(receipt.restored_validity.unwrap().is_empty());
    assert_eq!(receipt.restored_properties.unwrap().len(), 1);
    assert_eq!(stale_paths(&runtime), vec!["artifact.txt", "consumer.txt"]);
    let view = validity(&runtime, &grant);
    assert_eq!(
        view.properties
            .iter()
            .find(|p| p.obligation.id == first.id)
            .unwrap()
            .state,
        PropertyAssessmentState::Validated
    );
    assert_eq!(
        view.properties
            .iter()
            .find(|p| p.obligation.id == second.id)
            .unwrap()
            .state,
        PropertyAssessmentState::Unproven
    );
    assert_eq!(
        runtime.store.record(&grant, &first.id).unwrap(),
        first,
        "Checks must not change the authored obligation or its state"
    );
    runtime = configure(runtime, dir.path(), &[first, second]);
    let receipt = check(&runtime, "run", "complete-check", "check");
    assert_eq!(receipt.restored_properties.as_ref().unwrap().len(), 2);
    assert_eq!(receipt.restored_validity.as_ref().unwrap().len(), 1);
    assert_eq!(stale_paths(&runtime), vec!["consumer.txt"]);
    let view = validity(&runtime, &grant);
    assert!(
        view.properties
            .iter()
            .all(|p| p.state == PropertyAssessmentState::Validated
                && p.evidence_refs == vec![receipt.evidence_ref.clone().unwrap()])
    );
    assert!(
        receipt
            .restored_properties
            .unwrap()
            .iter()
            .all(|p| p.artifact == view.artifact)
    );
}

#[test]
fn independent_checks_can_accumulate_property_coverage_on_the_same_version() {
    let (dir, mut runtime, mut grant, first) = fixture();
    let second = property_record(
        &runtime.store,
        &grant,
        "Input supports output",
        &["artifact.txt"],
    );
    grant.id = "property-checks".into();
    grant.tools.push("second".into());
    runtime.store.register_grant(&grant).unwrap();
    runtime.store.begin_run("property-run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"property-run","profile":"caretaker","operator":"proofreading@1","prompt":"check properties","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    let mut first_check = recovery_tool();
    first_check.validated_properties = vec![binding(&first)];
    let mut second_check = recovery_tool();
    second_check.validated_properties = vec![binding(&second)];
    drop(runtime.host);
    runtime.host = std::sync::Arc::new(
        LocalHost::new(
            dir.path(),
            BTreeMap::from([
                ("check".into(), first_check),
                ("second".into(), second_check),
            ]),
        )
        .unwrap(),
    );
    let one = check(&runtime, "property-run", "one", "check");
    assert!(one.restored_validity.unwrap().is_empty());
    let two = check(&runtime, "property-run", "two", "second");
    assert_eq!(two.restored_validity.unwrap().len(), 1);
    let view = validity(&runtime, &grant);
    assert!(
        view.properties
            .iter()
            .all(|p| p.state == PropertyAssessmentState::Validated)
    );
    assert_eq!(
        view.properties
            .iter()
            .find(|p| p.obligation.id == first.id)
            .unwrap()
            .evidence_refs,
        vec![one.evidence_ref.unwrap()]
    );
    assert_eq!(
        view.properties
            .iter()
            .find(|p| p.obligation.id == second.id)
            .unwrap()
            .evidence_refs,
        vec![two.evidence_ref.unwrap()]
    );
}

#[test]
fn revised_obligation_needs_a_new_authorized_binding_and_new_check() {
    let (dir, mut runtime, grant, property) = fixture();
    check(&runtime, "run", "before-revision", "check");
    let mut body = property.body.clone();
    body.insert("description".into(), json!("Revised host criterion"));
    let revised = runtime
        .store
        .submit(
            &grant,
            &RecordSubmission {
                kind: RecordKind::Obligation,
                id: Some(property.id.clone()),
                expected_version: Some(property.version.clone()),
                provenance: property.provenance,
                body,
            },
            false,
        )
        .unwrap();
    let view = validity(&runtime, &grant);
    assert_eq!(view.properties[0].obligation.version, revised.version);
    assert_eq!(view.properties[0].state, PropertyAssessmentState::Stale);
    let denied = check(&runtime, "run", "obsolete-binding", "check");
    assert_eq!(denied.status, EffectStatus::Stale);
    assert_eq!(
        denied.outcome_basis,
        Some(EffectOutcomeBasis::NotDispatched)
    );
    runtime = configure(runtime, dir.path(), &[revised]);
    assert_eq!(
        check(&runtime, "run", "revised-check", "check")
            .restored_properties
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        validity(&runtime, &grant).properties[0].state,
        PropertyAssessmentState::Validated
    );
}

#[test]
fn delayed_finalizer_cannot_replace_fresh_proof_after_newer_invalidation_was_cleared() {
    let (_dir, runtime, grant, _property) = fixture();
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_old BEFORE UPDATE OF phase ON effects WHEN NEW.id='old' AND NEW.phase='finalized' BEGIN SELECT RAISE(FAIL,'injected old finalization failure'); END").unwrap();
    assert!(
        runtime
            .execute(
                "run",
                decode(
                    "Action",
                    json!({"kind":"check","operation_id":"old","tool":"check"})
                )
                .unwrap()
            )
            .unwrap_err()
            .message
            .contains("injected old finalization failure")
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row("SELECT count(*) FROM property_validations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0,
        "Property facts must roll back with receipt finalization"
    );
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER fail_old")
        .unwrap();
    let fresh = check(&runtime, "run", "fresh", "check");
    assert_eq!(stale_paths(&runtime), vec!["consumer.txt"]);
    let old = runtime.lookup("run", "old").unwrap();
    assert_eq!(old.status, EffectStatus::Succeeded);
    assert!(old.restored_properties.unwrap().is_empty());
    assert!(old.restored_validity.unwrap().is_empty());
    let view = validity(&runtime, &grant);
    assert_eq!(view.properties[0].state, PropertyAssessmentState::Validated);
    assert_eq!(
        view.properties[0].evidence_refs,
        vec![fresh.evidence_ref.unwrap()]
    );
}

#[test]
fn retained_validity_is_historical_and_withdrawal_removes_its_source_context() {
    let (_dir, runtime, grant, property) = fixture();
    check(&runtime, "run", "checked", "check");
    let call = decode(
        "ToolCall",
        json!({"call_id":"view","method":"artifact.validity","arguments":{"path":"artifact.txt"}}),
    )
    .unwrap();
    let observation = runtime.observe_tool("run", &call).unwrap();
    let serialized = serde_json::to_string(&observation).unwrap();
    assert!(serialized.contains("validated"));
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: property.id,
                expected_version: property.version,
                delete: false,
            },
        )
        .unwrap();
    assert!(runtime.retained_tool("run", "view").is_err());
    assert!(validity(&runtime, &grant).properties.is_empty());
    let receipt = check(&runtime, "run", "retired-binding", "check");
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::NotDispatched)
    );
}

#[test]
fn validity_supports_binary_artifacts_without_inventing_text_content() {
    let (dir, runtime, grant, _property) = fixture();
    std::fs::write(dir.path().join("artifact.txt"), [0xff, 0xfe, 0, 1]).unwrap();
    let view = runtime
        .tool_call("run", "artifact.validity", json!({"path":"artifact.txt"}))
        .unwrap();
    assert_eq!(view["properties"][0]["state"], "unproven");
    let read=runtime.tool_call("run","artifact.read",json!({"snapshot_id":view["snapshot_id"],"path":"artifact.txt","offset":0,"length":100,"required_freshness":"historical"})).unwrap_err();
    assert!(read.message.contains("version only"));
    let mut outside = grant.clone();
    outside.paths.clear();
    assert!(
        runtime
            .artifact_validity(
                &outside,
                &ArtifactValidityRequest {
                    path: "artifact.txt".into()
                }
            )
            .is_err()
    );
    std::fs::remove_file(dir.path().join("artifact.txt")).unwrap();
    assert!(
        runtime
            .tool_call("run", "artifact.validity", json!({"path":"artifact.txt"}))
            .is_err()
    );
    assert!(
        runtime
            .store
            .require_source(&grant, "artifact", view["snapshot_id"].as_str().unwrap())
            .is_err()
    );
}

#[test]
fn property_bindings_cannot_cross_declared_targets_or_record_scope() {
    let (dir, mut runtime, grant, property) = fixture();
    let mut tool = recovery_tool();
    let mut wrong = binding(&property);
    wrong.path = "consumer.txt".into();
    tool.validated_properties = vec![wrong];
    drop(runtime.host);
    assert!(
        LocalHost::new(dir.path(), BTreeMap::from([("check".into(), tool.clone())]))
            .err()
            .unwrap()
            .message
            .contains("property bindings")
    );
    tool.validates.push("consumer.txt".into());
    tool.reads.push("consumer.txt".into());
    runtime.host = std::sync::Arc::new(
        LocalHost::new(dir.path(), BTreeMap::from([("check".into(), tool)])).unwrap(),
    );
    assert_eq!(
        check(&runtime, "run", "wrong-target", "check").status,
        EffectStatus::Denied
    );
    let mut foreign = grant.clone();
    foreign.scope.project = "different-project".into();
    assert!(
        runtime
            .store
            .require_property_binding(&foreign, &binding(&property))
            .is_err()
    );
    assert!(
        runtime
            .assess_properties(
                &foreign,
                &runtime.host.version(&grant, "artifact.txt", None).unwrap()
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn schema_fourteen_preserves_old_certificates_without_manufacturing_property_support() {
    let (dir, runtime, grant, _property) = fixture();
    check(&runtime, "run", "legacy", "check");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    runtime.store.db.execute_batch(&[include_str!("../tests/fixtures/remove-budget-schema.sql"), "UPDATE effects SET body=json_remove(body,'$.restored_properties','$.validations[0].properties'); ALTER TABLE effects DROP COLUMN settlement; DROP TABLE property_validations; DROP TABLE artifact_invalidation_generations; ALTER TABLE artifact_snapshots DROP COLUMN version_only; PRAGMA user_version=13; CREATE TABLE property_validations(collision TEXT)"].concat()).unwrap();
    let before: String = runtime
        .store
        .db
        .query_row("SELECT body FROM effects WHERE id='legacy'", [], |r| {
            r.get(0)
        })
        .unwrap();
    drop(runtime);
    assert!(Store::open(dir.path().join("state.db")).is_err());
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        13
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='artifact_invalidation_generations'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TABLE property_validations").unwrap();
    let (runtime, grant) = recovery_fixture(dir.path());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row("SELECT body FROM effects WHERE id='legacy'", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        before
    );
    assert_eq!(
        validity(&runtime, &grant).properties[0].state,
        PropertyAssessmentState::Unproven
    );
    assert!(
        runtime
            .store
            .artifact_invalidation_generation(&grant.scope, "artifact.txt")
            .unwrap()
            > 0
    );
}
