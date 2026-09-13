use super::*;

fn stop(runtime: &Runtime) {
    runtime
        .store
        .finish_run(
            "run",
            &AgentResult {
                disposition: Disposition::Interrupted,
                summary: "Owner stopped maintenance for reconciliation".into(),
            },
        )
        .unwrap();
}

fn unknown(runtime: &Runtime, grant: &Grant, operation: &str) -> ActionReceipt {
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER lose_outcome BEFORE UPDATE OF observation ON effects BEGIN SELECT RAISE(FAIL,'injected lost outcome'); END").unwrap();
    assert!(
        runtime
            .execute("run", edit_action(runtime, grant, operation))
            .unwrap_err()
            .message
            .contains("injected lost outcome")
    );
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER lose_outcome")
        .unwrap();
    runtime.lookup("run", operation).unwrap()
}

fn request(runtime: &Runtime, grant: &Grant, operation: &str) -> EffectSettlementRequest {
    let view = runtime.inspect_effect(grant, operation).unwrap();
    EffectSettlementRequest {
        operation_id: operation.into(),
        expected_receipt_version: view.receipt_version,
        executor_stopped: true,
        workspace_versions: view.workspace_versions,
        reason:
            "Owner verified the executor is stopped; investigate and freshly validate its outputs"
                .into(),
        source_refs: vec![],
    }
}

#[test]
fn host_settlement_preserves_unknown_execution_and_requires_fresh_validation() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    let original = unknown(&runtime, &grant, "lost");
    assert!(original.requires_reconciliation());
    assert_eq!(
        runtime
            .execute("run", edit_action(&runtime, &grant, "blocked"))
            .unwrap()
            .status,
        EffectStatus::Denied
    );
    stop(&runtime);
    let event_before: String = runtime
        .store
        .db
        .query_row(
            "SELECT body FROM events WHERE id=?1",
            [original.evidence_ref.as_ref().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let decision = request(&runtime, &grant, "lost");
    let settled = runtime.settle_effect(&grant, &decision).unwrap();
    assert_eq!(settled.status, EffectStatus::Unknown);
    assert_eq!(
        settled.outcome_basis,
        Some(EffectOutcomeBasis::CurrentPostconditionObserved)
    );
    assert!(!settled.requires_reconciliation());
    assert!(settled.restored_properties.as_ref().unwrap().is_empty());
    assert_eq!(stale_paths(&runtime), vec!["artifact.txt", "consumer.txt"]);
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT body FROM events WHERE id=?1",
                [original.evidence_ref.as_ref().unwrap()],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        event_before
    );
    assert_eq!(
        runtime.store.inspect_run("run").unwrap()["effects"][0]["settlement"]["evidence_ref"],
        settled.settlement.as_ref().unwrap().evidence_ref
    );
    drop(runtime);
    let (runtime, grant) = recovery_fixture(dir.path());
    assert_eq!(runtime.lookup("run", "lost").unwrap(), settled);
    assert_eq!(
        runtime.settle_effect(&grant, &decision).unwrap(),
        settled,
        "A retry remains a historical decision even after the run resumes"
    );
    let check = runtime
        .execute(
            "run",
            decode(
                "Action",
                json!({"kind":"check","operation_id":"fresh-check","tool":"check"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(check.restored_properties.unwrap().len(), 1);
    assert_eq!(stale_paths(&runtime), vec!["consumer.txt"]);
    assert_eq!(
        runtime
            .execute("run", edit_action(&runtime, &grant, "new-operation"))
            .unwrap()
            .status,
        EffectStatus::Succeeded
    );
    assert_eq!(
        runtime.lookup("run", "lost").unwrap().status,
        EffectStatus::Unknown
    );
}

#[test]
fn settlement_rejects_active_run_stale_versions_wrong_owner_and_worker_tools() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    unknown(&runtime, &grant, "lost");
    let mut decision = request(&runtime, &grant, "lost");
    assert!(
        runtime
            .settle_effect(&grant, &decision)
            .unwrap_err()
            .message
            .contains("stop the owning run")
    );
    assert!(
        runtime
            .tool_call(
                "run",
                "effect.settle",
                serde_json::to_value(&decision).unwrap()
            )
            .is_err()
    );
    assert!(runtime.observe_tool("run",&decode("ToolCall",json!({"call_id":"fake","method":"action.lookup","arguments":{"id":"lost","settlement":decision}})).unwrap()).is_err());
    stop(&runtime);
    decision.executor_stopped = false;
    assert!(runtime.settle_effect(&grant, &decision).is_err());
    decision.executor_stopped = true;
    std::fs::write(dir.path().join("input.txt"), "later input").unwrap();
    assert!(
        runtime
            .settle_effect(&grant, &decision)
            .unwrap_err()
            .message
            .contains("workspace changed")
    );
    decision = request(&runtime, &grant, "lost");
    let mut other = grant.clone();
    other.id = "different-owner".into();
    runtime.store.register_grant(&other).unwrap();
    assert_eq!(
        runtime.settle_effect(&other, &decision).unwrap_err().code,
        -32004
    );
    decision.expected_receipt_version = "wrong-receipt".into();
    assert!(runtime.settle_effect(&grant, &decision).is_err());
    decision = request(&runtime, &grant, "lost");
    decision.source_refs.push("not-visible-evidence".into());
    assert!(runtime.settle_effect(&grant, &decision).is_err());
    assert!(
        runtime
            .lookup("run", "lost")
            .unwrap()
            .requires_reconciliation()
    );
}

#[test]
fn failed_settlement_rolls_back_its_event_invalidation_and_fence_release() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    unknown(&runtime, &grant, "lost");
    stop(&runtime);
    let decision = request(&runtime, &grant, "lost");
    let generation = runtime.store.validity_generation(&grant.scope).unwrap();
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_settlement BEFORE UPDATE OF settlement ON effects BEGIN SELECT RAISE(FAIL,'injected settlement failure'); END").unwrap();
    assert!(
        runtime
            .settle_effect(&grant, &decision)
            .unwrap_err()
            .message
            .contains("injected settlement failure")
    );
    assert_eq!(
        runtime.store.validity_generation(&grant.scope).unwrap(),
        generation
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM events WHERE producer='ribosome-host-settlement'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(
        runtime
            .lookup("run", "lost")
            .unwrap()
            .requires_reconciliation()
    );
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER fail_settlement")
        .unwrap();
    let settled = runtime.settle_effect(&grant, &decision).unwrap();
    let generation = runtime.store.validity_generation(&grant.scope).unwrap();
    assert_eq!(runtime.settle_effect(&grant, &decision).unwrap(), settled);
    assert_eq!(
        runtime.store.validity_generation(&grant.scope).unwrap(),
        generation
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT count(*) FROM events WHERE producer='ribosome-host-settlement'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let mut changed = decision;
    changed.reason = "Different decision".into();
    assert_eq!(
        runtime.settle_effect(&grant, &changed).unwrap_err().code,
        -32002
    );
}

#[test]
fn expired_grants_can_settle_but_cannot_authorize_another_effect() {
    let dir = recovery_files();
    let (runtime, mut grant) = recovery_fixture(dir.path());
    unknown(&runtime, &grant, "lost");
    stop(&runtime);
    let decision = request(&runtime, &grant, "lost");
    grant.budget.deadline_ms = "0".into();
    runtime
        .store
        .db
        .execute(
            "UPDATE grants SET body=?1 WHERE id=?2",
            params![serde_json::to_string(&grant).unwrap(), grant.id],
        )
        .unwrap();
    assert!(
        !runtime
            .settle_effect(&grant, &decision)
            .unwrap()
            .requires_reconciliation()
    );
    let effect = runtime
        .execute("run", edit_action(&runtime, &grant, "expired-operation"))
        .unwrap();
    assert_eq!(
        effect.outcome_basis,
        Some(EffectOutcomeBasis::NotDispatched)
    );
}

#[test]
fn settlement_observation_inherits_owner_evidence_withdrawal() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    let evidence = property_record(&runtime.store, &grant, "Owner inspection reference", &[]);
    unknown(&runtime, &grant, "lost");
    stop(&runtime);
    let mut decision = request(&runtime, &grant, "lost");
    decision.source_refs.push(evidence.id.clone());
    runtime.settle_effect(&grant, &decision).unwrap();
    drop(runtime);
    let (runtime, grant) = recovery_fixture(dir.path());
    let observation=runtime.observe_tool("run",&decode("ToolCall",json!({"call_id":"settled-lookup","method":"action.lookup","arguments":{"id":"lost"}})).unwrap()).unwrap();
    assert!(
        serde_json::to_string(&observation)
            .unwrap()
            .contains("settlement")
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: evidence.id,
                expected_version: evidence.version,
                delete: false,
            },
        )
        .unwrap();
    assert!(runtime.retained_tool("run", "settled-lookup").is_err());
    assert!(
        !runtime
            .lookup("run", "lost")
            .unwrap()
            .requires_reconciliation(),
        "Withdrawal does not make a stopped executor run again"
    );
}

#[test]
fn schema_fifteen_does_not_release_legacy_unknown_effects() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    let mut original = unknown(&runtime, &grant, "legacy");
    runtime.store.db.execute_batch("UPDATE effects SET observation=NULL,body=json_remove(body,'$.evidence_ref','$.outcome_basis') WHERE id='legacy'").unwrap();
    original.evidence_ref = None;
    original.outcome_basis = None;
    runtime
        .store
        .db
        .execute_batch(
            &[
                include_str!("../tests/fixtures/remove-budget-schema.sql"),
                "ALTER TABLE effects DROP COLUMN settlement; PRAGMA user_version=14",
            ]
            .concat(),
        )
        .unwrap();
    drop(runtime);
    let (runtime, grant) = recovery_fixture(dir.path());
    let migrated = runtime.lookup("run", "legacy").unwrap();
    assert_eq!(
        runtime.lookup_for_recovery("run", "legacy").unwrap(),
        original
    );
    assert_eq!(migrated.status, original.status);
    assert_eq!(migrated.content_available, Some(false));
    assert!(migrated.action.content.is_none());
    assert!(migrated.requires_reconciliation());
    stop(&runtime);
    let decision = request(&runtime, &grant, "legacy");
    let settled = runtime.settle_effect(&grant, &decision).unwrap();
    assert_eq!(settled.evidence_ref, None);
    assert_eq!(settled.outcome_basis, None);
    assert!(!settled.requires_reconciliation());
    assert_eq!(stale_paths(&runtime), vec!["artifact.txt"]);
}
