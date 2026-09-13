use crate::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use rusqlite::{OptionalExtension, params};
use serde_json::json;
use std::collections::BTreeMap;

#[path = "property_validation_tests.rs"]
mod properties;

#[path = "effect_settlement_tests.rs"]
mod settlement;

#[test]
fn persisted_intent_without_receipt_reconciles_or_remains_unknown_without_reexecution() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("artifact.txt"), "desired output").unwrap();
    let store = Store::open(d.path().join("state.db")).unwrap();
    let host = LocalHost::new(d.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), d.path().join("state")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"grant","scope":{"client":"client","project":"project"},"mode":"apply","paths":["artifact.txt"],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":1,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":10,"max_work_items":1,"max_depth":1,"deadline_ms":(now_ms()+10000).to_string()},"context":"test","visible_splits":["development"],"allow_export":false})).unwrap();
    runtime.store.register_grant(&grant).unwrap();
    runtime.store.begin_run("run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"run","profile":"caretaker","operator":"proofreading@1","prompt":"recover","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    for (operation, kind, content, expected) in [
        (
            "matching-edit",
            "edit",
            "desired output",
            EffectStatus::Unknown,
        ),
        (
            "matching-apply",
            "apply",
            "desired output",
            EffectStatus::Unknown,
        ),
        (
            "uncertain",
            "edit",
            "different output",
            EffectStatus::Unknown,
        ),
    ] {
        let action:Action=decode("Action",json!({"operation_id":operation,"kind":kind,"path":"artifact.txt","expected_version":"lost-old-version","content":content})).unwrap();
        let receipt = ActionReceipt {
            content_available: None,
            operation_id: operation.into(),
            run_id: "run".into(),
            status: EffectStatus::Started,
            action,
            before: vec![],
            after: vec![],
            output: String::new(),
            side_effects: vec![],
            started_ms: now_ms().to_string(),
            finished_ms: None,
            elapsed_ms: None,
            reconciled: false,
            evidence_ref: None,
            outcome_basis: None,
            validations: None,
            restored_validity: None,
            restored_properties: None,
            settlement: None,
        };
        runtime
            .store
            .db
            .execute(
                "INSERT INTO effects(id,run_id,grant_id,body) VALUES (?1,'run','grant',?2)",
                params![operation, serde_json::to_string(&receipt).unwrap()],
            )
            .unwrap();
        let result = runtime.lookup("run", operation).unwrap();
        assert_eq!(result.status, expected);
        assert!(result.reconciled);
        assert_eq!(
            result.outcome_basis,
            Some(if operation == "uncertain" {
                EffectOutcomeBasis::Unresolved
            } else {
                EffectOutcomeBasis::CurrentPostconditionObserved
            })
        );
        assert_eq!(runtime.lookup("run", operation).unwrap(), result);
        assert_eq!(
            std::fs::read_to_string(d.path().join("artifact.txt")).unwrap(),
            "desired output"
        );
    }
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_receipt_event BEFORE INSERT ON events WHEN NEW.producer='ribosome-host' BEGIN SELECT RAISE(FAIL, 'injected evidence write failure'); END;").unwrap();
    let branch = decode(
        "Action",
        json!({"kind":"branch","operation_id":"event-write-failure"}),
    )
    .unwrap();
    assert!(runtime.execute("run", branch).is_err());
    let body: String = runtime
        .store
        .db
        .query_row(
            "SELECT body FROM effects WHERE id='event-write-failure'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let intent: ActionReceipt = serde_json::from_str(&body).unwrap();
    assert_eq!(
        intent.status,
        EffectStatus::Started,
        "a settled receipt must not survive without its evidence event"
    );
    assert!(
        intent.evidence_ref.is_some(),
        "dispatch source context survives failed completion publication"
    );
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER fail_receipt_event")
        .unwrap();
    assert_eq!(
        runtime.lookup("run", "event-write-failure").unwrap().status,
        EffectStatus::Succeeded
    );
}

fn recovery_fixture(root: &std::path::Path) -> (Runtime, Grant) {
    recovery_fixture_with_tool(root, recovery_tool())
}

fn recovery_tool() -> crate::host::RegisteredTool {
    crate::host::RegisteredTool {
        program: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "test -s artifact.txt && test -s input.txt".into(),
        ],
        timeout_ms: 1000,
        reads: vec!["artifact.txt".into(), "input.txt".into()],
        validates: vec!["artifact.txt".into()],
        writes: vec![],
        code_files: vec![],
        validated_properties: vec![],
    }
}

fn recovery_fixture_with_tool(
    root: &std::path::Path,
    mut tool: crate::host::RegisteredTool,
) -> (Runtime, Grant) {
    let store = Store::open(root.join("state.db")).unwrap();
    let grant: Grant = if let Ok(grant) = store.grant("owner") {
        grant
    } else {
        let grant = decode("Grant",json!({"id":"owner","scope":{"client":"client","project":"project"},"mode":"apply","paths":["input.txt","artifact.txt","consumer.txt"],"tools":["check"],"profiles":["caretaker"],"budget":{"max_calls":5,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":20,"max_work_items":1,"max_depth":1,"deadline_ms":(now_ms()+60000).to_string()},"context":"test","visible_splits":["development"],"allow_export":false})).unwrap();
        store.register_grant(&grant).unwrap();
        grant
    };
    if tool.validated_properties.is_empty() && tool.validates.contains(&"artifact.txt".into()) {
        let existing: Option<String> = store.db.query_row("SELECT id FROM records WHERE kind='obligation' AND json_extract(body,'$.body.description')='Required files are nonempty'", [], |r| r.get(0)).optional().unwrap();
        let record = existing
            .map(|id| store.record(&grant, &id).unwrap())
            .unwrap_or_else(|| {
                property_record(
                    &store,
                    &grant,
                    "Required files are nonempty",
                    &["artifact.txt"],
                )
            });
        tool.validated_properties.push(PropertyBinding {
            obligation: VersionRef {
                id: record.id,
                version: record.version,
            },
            path: "artifact.txt".into(),
        });
    }
    let host = LocalHost::new(root, BTreeMap::from([("check".into(), tool)])).unwrap();
    let runtime = Runtime::new(store, Box::new(host), root.join("state")).unwrap();
    runtime.store.begin_run("run", &grant.id, &decode("AgentRunRequest",json!({"run_id":"run","profile":"caretaker","operator":"proofreading@1","prompt":"recover","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    (runtime, grant)
}

fn recovery_files() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in [
        ("input.txt", "input"),
        ("artifact.txt", "original"),
        ("consumer.txt", "consumer"),
    ] {
        std::fs::write(dir.path().join(path), content).unwrap();
    }
    dir
}

fn dependency(runtime: &Runtime, grant: &Grant, source: &str, dependent: &str) {
    let edge: Dependency = decode("Dependency",json!({"source":runtime.host.version(grant,source,None).unwrap(),"dependent":runtime.host.version(grant,dependent,None).unwrap(),"basis":"host","evidence_refs":[]})).unwrap();
    runtime.store.add_dependency(&grant.scope, &edge).unwrap();
}

fn edit_action(runtime: &Runtime, grant: &Grant, operation: &str) -> Action {
    decode("Action",json!({"kind":"edit","operation_id":operation,"path":"artifact.txt","expected_version":runtime.host.version(grant,"artifact.txt",None).unwrap().version,"content":"desired output"})).unwrap()
}

fn stale_paths(runtime: &Runtime) -> Vec<String> {
    runtime
        .store
        .db
        .prepare("SELECT path FROM invalidated ORDER BY path")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

#[test]
fn captured_edit_retries_atomic_finalization_after_restart_without_repeating_the_write() {
    for fail_receipt in [false, true] {
        let dir = recovery_files();
        let (runtime, grant) = recovery_fixture(dir.path());
        dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
        let action = edit_action(&runtime, &grant, "edit");
        let fault = if fail_receipt {
            "CREATE TEMP TRIGGER fail_finalization BEFORE UPDATE OF phase ON effects WHEN NEW.phase='finalized' BEGIN SELECT RAISE(FAIL,'injected finalization failure'); END"
        } else {
            "CREATE TEMP TRIGGER fail_finalization BEFORE INSERT ON events WHEN NEW.producer='ribosome-host' BEGIN SELECT RAISE(FAIL,'injected finalization failure'); END"
        };
        runtime.store.db.execute_batch(fault).unwrap();
        assert!(
            runtime
                .execute("run", action.clone())
                .unwrap_err()
                .message
                .contains("injected finalization failure")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
            "desired output"
        );
        assert!(
            stale_paths(&runtime).is_empty(),
            "Invalidation and terminal publication must roll back together"
        );
        let (phase, status): (String, String) = runtime
            .store
            .db
            .query_row(
                "SELECT phase,json_extract(body,'$.status') FROM effects WHERE id='edit'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((phase.as_str(), status.as_str()), ("observed", "started"));
        drop(runtime);
        std::fs::write(dir.path().join("artifact.txt"), "later owner version").unwrap();
        let (runtime, _) = recovery_fixture(dir.path());
        let receipt = runtime.execute("run", action.clone()).unwrap();
        assert_eq!(receipt.status, EffectStatus::Succeeded);
        assert_eq!(
            receipt.outcome_basis,
            Some(EffectOutcomeBasis::ExecutionEstablished)
        );
        assert_eq!(
            receipt.after[0].version,
            crate::host::hash(b"desired output")
        );
        assert_eq!(stale_paths(&runtime), vec!["consumer.txt"]);
        let generation = runtime.store.validity_generation(&grant.scope).unwrap();
        assert_eq!(runtime.lookup("run", "edit").unwrap(), receipt);
        assert_eq!(runtime.execute("run", action.clone()).unwrap(), receipt);
        assert_eq!(
            runtime.store.validity_generation(&grant.scope).unwrap(),
            generation
        );
        assert_eq!(
            runtime
                .store
                .db
                .query_row(
                    "SELECT count(*) FROM events WHERE producer='ribosome-host' AND json_extract(body,'$.correlation')='edit'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
            "later owner version"
        );
        let mut collision = action;
        collision.content = Some("different arguments".into());
        assert_eq!(runtime.execute("run", collision).unwrap_err().code, -32002);
    }
}

#[test]
fn process_exit_after_write_without_durable_outcome_invalidates_consumers_without_proving_execution()
 {
    const CHILD: &str = "RIBOSOME_EFFECT_CRASH_TEST_DIR";
    if let Some(root) = std::env::var_os(CHILD) {
        let (runtime, grant) = recovery_fixture(std::path::Path::new(&root));
        runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_capture BEFORE UPDATE OF observation ON effects BEGIN SELECT RAISE(FAIL,'injected capture failure'); END").unwrap();
        let result = runtime.execute("run", edit_action(&runtime, &grant, "crashed-edit"));
        assert!(
            result
                .unwrap_err()
                .message
                .contains("injected capture failure")
        );
        std::process::exit(73);
    }
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    drop(runtime);
    let child = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","recovery_tests::process_exit_after_write_without_durable_outcome_invalidates_consumers_without_proving_execution","--nocapture"]).env(CHILD,dir.path()).output().unwrap();
    assert_eq!(
        child.status.code(),
        Some(73),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
        "desired output"
    );
    let (runtime, grant) = recovery_fixture(dir.path());
    assert!(stale_paths(&runtime).is_empty());
    let receipt = runtime.lookup("run", "crashed-edit").unwrap();
    assert_eq!(receipt.status, EffectStatus::Unknown);
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::CurrentPostconditionObserved)
    );
    assert_eq!(stale_paths(&runtime), vec!["consumer.txt"]);
    let event: String = runtime
        .store
        .db
        .query_row(
            "SELECT body FROM events WHERE id=?1",
            [receipt.evidence_ref.as_ref().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&event).unwrap()["kind"],
        "action_reconciliation"
    );
    let next = runtime
        .execute("run", edit_action(&runtime, &grant, "new-write"))
        .unwrap();
    assert_eq!(
        next.status,
        EffectStatus::Denied,
        "Uncertain issued effects must retain live write ownership"
    );
    std::fs::write(dir.path().join("artifact.txt"), "later owner version").unwrap();
    assert_eq!(runtime.lookup("run", "crashed-edit").unwrap(), receipt);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
        "later owner version"
    );
}

#[test]
fn delayed_check_finalization_preserves_newer_invalidation_even_when_input_bytes_return() {
    for change in ["none", "generation", "version"] {
        let dir = recovery_files();
        let (runtime, grant) = recovery_fixture(dir.path());
        dependency(&runtime, &grant, "input.txt", "artifact.txt");
        dependency(&runtime, &grant, "input.txt", "consumer.txt");
        runtime
            .store
            .invalidate_dependents(&grant.scope, "input.txt")
            .unwrap();
        runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_finalization BEFORE INSERT ON events WHEN NEW.producer='ribosome-host' BEGIN SELECT RAISE(FAIL,'injected finalization failure'); END").unwrap();
        assert!(
            runtime
                .execute(
                    "run",
                    decode(
                        "Action",
                        json!({"kind":"check","operation_id":"check","tool":"check"})
                    )
                    .unwrap()
                )
                .is_err()
        );
        assert_eq!(stale_paths(&runtime), vec!["artifact.txt", "consumer.txt"]);
        runtime
            .store
            .db
            .execute_batch("DROP TRIGGER fail_finalization")
            .unwrap();
        if change != "none" {
            std::fs::write(dir.path().join("input.txt"), "changed input").unwrap();
            if change == "generation" {
                runtime
                    .store
                    .invalidate_dependents(&grant.scope, "input.txt")
                    .unwrap();
                std::fs::write(dir.path().join("input.txt"), "input").unwrap();
            }
        }
        let receipt = runtime.lookup("run", "check").unwrap();
        assert_eq!(receipt.status, EffectStatus::Succeeded);
        let expected = if change != "none" {
            vec!["artifact.txt", "consumer.txt"]
        } else {
            vec!["consumer.txt"]
        };
        assert_eq!(
            stale_paths(&runtime),
            expected,
            "Only the explicitly validated target at the checked generation can be cleared"
        );
        assert_eq!(runtime.lookup("run", "check").unwrap(), receipt);
    }
}

#[test]
fn effect_migration_rolls_back_and_preserves_legacy_outcomes_without_inventing_execution_evidence()
{
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    let receipt = runtime
        .execute(
            "run",
            decode(
                "Action",
                json!({"operation_id":"legacy-check","kind":"check","tool":"check"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    runtime.store.db.execute_batch(&[include_str!("../tests/fixtures/remove-budget-schema.sql"), "UPDATE effects SET body=json_remove(body,'$.outcome_basis'); ALTER TABLE effects DROP COLUMN settlement; DROP TABLE property_validations; DROP TABLE artifact_invalidation_generations; ALTER TABLE artifact_snapshots DROP COLUMN version_only; ALTER TABLE effects DROP COLUMN preflight; ALTER TABLE effects DROP COLUMN phase; ALTER TABLE effects DROP COLUMN authority; ALTER TABLE effects DROP COLUMN potential_writes; ALTER TABLE effects DROP COLUMN observation; ALTER TABLE effects DROP COLUMN validation_generation; ALTER TABLE invalidated DROP COLUMN generation; DROP TABLE validity_clock; PRAGMA user_version=11;"].concat()).unwrap();
    let mut intent = receipt;
    intent.operation_id = "legacy-intent".into();
    intent.action=decode("Action",json!({"operation_id":"legacy-intent","kind":"apply","path":"artifact.txt","expected_version":"lost","content":"original"})).unwrap();
    intent.status = EffectStatus::Started;
    intent.outcome_basis = None;
    intent.after = vec![];
    intent.evidence_ref = None;
    intent.finished_ms = None;
    runtime
        .store
        .db
        .execute(
            "INSERT INTO effects(id,run_id,grant_id,body) VALUES('legacy-intent','run',?1,?2)",
            params![grant.id, serde_json::to_string(&intent).unwrap()],
        )
        .unwrap();
    runtime
        .store
        .db
        .execute_batch("CREATE TABLE validity_clock(injected INTEGER)")
        .unwrap();
    drop(runtime);
    assert!(
        Store::open(dir.path().join("state.db"))
            .err()
            .unwrap()
            .message
            .contains("already exists")
    );
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        11
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('effects') WHERE name='phase'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TABLE validity_clock").unwrap();
    let (runtime, _) = recovery_fixture(dir.path());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    let (phase, missing): (String, bool) = db
        .query_row(
            "SELECT phase,observation IS NULL FROM effects WHERE id='legacy-check'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(phase, "finalized");
    assert!(missing);
    let legacy = runtime.lookup("run", "legacy-check").unwrap();
    assert_eq!(legacy.status, EffectStatus::Succeeded);
    assert_eq!(
        legacy.outcome_basis, None,
        "Old status labels cannot fabricate adapter evidence"
    );
    assert_eq!(
        db.query_row(
            "SELECT phase FROM effects WHERE id='legacy-intent'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "dispatching"
    );
    let pending = runtime.lookup("run", "legacy-intent").unwrap();
    assert_eq!(pending.status, EffectStatus::Unknown);
    assert_eq!(
        pending.outcome_basis,
        Some(EffectOutcomeBasis::CurrentPostconditionObserved)
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE producer='ribosome-host' AND json_extract(body,'$.correlation')='legacy-check'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn failed_dispatch_marker_prevents_external_mutation_and_finalizes_without_execution() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_dispatch BEFORE UPDATE OF phase ON effects WHEN NEW.phase='dispatching' BEGIN SELECT RAISE(FAIL,'injected dispatch persistence failure'); END").unwrap();
    let receipt = runtime
        .execute("run", edit_action(&runtime, &grant, "not-issued"))
        .unwrap();
    assert_eq!(receipt.status, EffectStatus::Failed);
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::NotDispatched)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
        "original"
    );
    assert!(stale_paths(&runtime).is_empty());
    assert_eq!(runtime.lookup("run", "not-issued").unwrap(), receipt);
}

fn checked_application(runtime: &Runtime, grant: &Grant, operation: &str) -> Action {
    let branch = runtime
        .execute(
            "run",
            decode(
                "Action",
                json!({"operation_id":format!("{operation}-branch"),"kind":"branch"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(branch.status, EffectStatus::Succeeded);
    let original = runtime.host.version(grant, "artifact.txt", None).unwrap();
    let mut edit = edit_action(runtime, grant, &format!("{operation}-edit"));
    edit.branch_id = Some(branch.output.clone());
    assert_eq!(
        runtime.execute("run", edit).unwrap().status,
        EffectStatus::Succeeded
    );
    let provenance = json!({"origin":"observed","source_refs":[],"scenario_family":"validation","split":"development","limitations":[]});
    let finding=runtime.store.submit(grant,&decode("RecordSubmission",json!({"kind":"finding","provenance":provenance,"body":{"subject":"artifact.txt","observation":"artifact requires correction","interpretation":"repair the artifact","evidence_refs":[],"uncertainty":[],"operator":"excision-repair@1"}})).unwrap(),false).unwrap();
    let path = runtime
        .branch_path(grant, Some(&branch.output))
        .unwrap()
        .unwrap();
    let reference = runtime
        .host
        .version(grant, "artifact.txt", Some(&path))
        .unwrap();
    let intervention=runtime.store.submit(grant,&decode("RecordSubmission",json!({"kind":"intervention","provenance":provenance,"body":{"kind":"repair","subject":"artifact.txt","finding_ref":finding.id,"read_versions":[reference],"preserve":["input.txt","consumer.txt"],"replace":["artifact.txt"],"invalidate":[],"recompute":[],"required_checks":["check"],"bindings":{},"requested_effects":["edit artifact"],"assumptions":[],"fallback":"abstain","operator":"excision-repair@1"}})).unwrap(),false).unwrap();
    decode("Action",json!({"operation_id":operation,"kind":"apply","branch_id":branch.output,"path":"artifact.txt","expected_version":original.version,"content":"desired output","intervention_ref":intervention.id})).unwrap()
}

#[test]
fn checked_application_restores_its_validated_artifact_and_leaves_consumers_stale() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    dependency(&runtime, &grant, "input.txt", "artifact.txt");
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    let apply = checked_application(&runtime, &grant, "apply");
    let receipt = runtime.execute("run", apply).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(
        stale_paths(&runtime),
        vec!["consumer.txt"],
        "Checked application must restore its validated artifact, while unvalidated consumers remain stale"
    );
}

#[test]
fn checking_an_input_is_not_validation_of_the_applied_artifact() {
    let dir = recovery_files();
    let mut tool = recovery_tool();
    tool.validates = vec!["input.txt".into()];
    let (runtime, grant) = recovery_fixture_with_tool(dir.path(), tool);
    let apply = checked_application(&runtime, &grant, "apply");
    let receipt = runtime.execute("run", apply).unwrap();
    assert_eq!(
        receipt.status,
        EffectStatus::Denied,
        "Reading the target cannot substitute for explicit validation coverage"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
        "original"
    );
}

#[test]
fn captured_application_rechecks_inputs_checker_and_policy_before_restoring_validity() {
    for change in [
        "none",
        "input",
        "generation",
        "checker",
        "policy",
        "intervention",
    ] {
        let dir = recovery_files();
        let code = dir.path().join("check.sh");
        std::fs::write(&code, "test -s artifact.txt && test -s input.txt\n").unwrap();
        let mut tool = recovery_tool();
        tool.args = vec![code.to_string_lossy().into_owned()];
        let (runtime, grant) = recovery_fixture_with_tool(dir.path(), tool.clone());
        dependency(&runtime, &grant, "input.txt", "artifact.txt");
        dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
        runtime
            .store
            .invalidate_dependents(&grant.scope, "input.txt")
            .unwrap();
        let action = checked_application(&runtime, &grant, "apply");
        runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_application_finalization BEFORE INSERT ON events WHEN NEW.producer='ribosome-host' AND json_extract(NEW.body,'$.correlation')='apply' BEGIN SELECT RAISE(FAIL,'injected application finalization failure'); END").unwrap();
        assert!(
            runtime
                .execute("run", action.clone())
                .unwrap_err()
                .message
                .contains("injected application finalization failure")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
            "desired output"
        );
        if change == "generation" {
            runtime
                .store
                .invalidate_dependents(&grant.scope, "input.txt")
                .unwrap();
        }
        if change == "policy" {
            runtime.store.db.execute_batch("UPDATE grants SET body=json_set(body,'$.required_checks',json('[\"check\"]')) WHERE id='owner'").unwrap();
        }
        if change == "intervention" {
            runtime.store.db.execute("UPDATE records SET version='2',body=json_set(body,'$.version','2') WHERE id=?1",[action.intervention_ref.as_ref().unwrap()]).unwrap();
        }
        drop(runtime);
        if change == "input" {
            std::fs::write(dir.path().join("input.txt"), "changed input").unwrap();
        }
        if change == "checker" {
            std::fs::write(
                &code,
                "test -s artifact.txt && test -s input.txt\n# changed checker\n",
            )
            .unwrap();
        }
        let (runtime, _) = recovery_fixture_with_tool(dir.path(), tool);
        let receipt = runtime.lookup("run", "apply").unwrap();
        assert_eq!(
            receipt.status,
            EffectStatus::Succeeded,
            "The adapter outcome remains an actual historical application"
        );
        let checks = receipt.validations.as_ref().unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].inputs.len(), 2);
        assert_eq!(
            checks[0].targets,
            vec![runtime.host.version(&grant, "artifact.txt", None).unwrap()]
        );
        assert_eq!(
            checks[0].receipt_ref,
            runtime
                .lookup("run", "apply/check/0")
                .unwrap()
                .evidence_ref
                .unwrap()
        );
        let expected = if change == "none" {
            vec!["consumer.txt"]
        } else {
            vec!["artifact.txt", "consumer.txt"]
        };
        assert_eq!(stale_paths(&runtime), expected, "change: {change}");
        assert_eq!(
            receipt.restored_validity.as_ref().unwrap().len(),
            usize::from(change == "none"),
            "change: {change}"
        );
        let saved_generation = runtime.store.validity_generation(&grant.scope).unwrap();
        assert_eq!(runtime.lookup("run", "apply").unwrap(), receipt);
        assert_eq!(
            runtime.store.validity_generation(&grant.scope).unwrap(),
            saved_generation
        );
    }
}

#[test]
fn changing_checker_code_during_execution_cannot_produce_passed_validation() {
    let dir = recovery_files();
    let code = dir.path().join("self-changing-check.sh");
    let stable = dir.path().join("checker-dependency.txt");
    std::fs::write(&stable, "dependency version one").unwrap();
    // The command exits successfully but changes a declared code dependency.
    std::fs::write(&code, format!("printf changed > '{}'\n", stable.display())).unwrap();
    let mut tool = recovery_tool();
    tool.args = vec![code.to_string_lossy().into_owned()];
    tool.code_files = vec![stable];
    let (runtime, grant) = recovery_fixture_with_tool(dir.path(), tool);
    dependency(&runtime, &grant, "input.txt", "artifact.txt");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    let receipt = runtime
        .execute(
            "run",
            decode(
                "Action",
                json!({"operation_id":"check","kind":"check","tool":"check"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(receipt.status, EffectStatus::Failed);
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::ExecutionEstablished)
    );
    assert_eq!(
        receipt.validations.unwrap()[0].outcome,
        ValidationEvidenceOutcome::Stale
    );
    assert_eq!(stale_paths(&runtime), vec!["artifact.txt"]);
}

#[test]
fn owner_policy_change_during_branch_checks_prevents_live_dispatch() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    let action = checked_application(&runtime, &grant, "apply");
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER change_owner_policy AFTER INSERT ON events WHEN json_extract(NEW.body,'$.correlation')='apply/check/0' BEGIN UPDATE grants SET body=json_set(body,'$.required_checks',json('[\"check\"]')) WHERE id='owner'; END").unwrap();
    let receipt = runtime.execute("run", action).unwrap();
    assert_eq!(receipt.status, EffectStatus::Denied);
    assert_eq!(
        receipt.outcome_basis,
        Some(EffectOutcomeBasis::NotDispatched)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
        "original"
    );
}

#[test]
fn application_requires_current_captured_check_evidence_and_never_replays_a_cached_check() {
    for state in ["current", "legacy", "changed-checker"] {
        let dir = recovery_files();
        let code = dir.path().join("check.sh");
        let counter_dir = tempfile::tempdir().unwrap();
        let counter = counter_dir.path().join("calls");
        // Count host invocations outside the granted task workspace.
        let script = format!(
            "printf 'run\\n' >> '{}'\ntest -s artifact.txt && test -s input.txt\n",
            counter.display()
        );
        std::fs::write(&code, &script).unwrap();
        let mut tool = recovery_tool();
        tool.args = vec![code.to_string_lossy().into_owned()];
        let (runtime, grant) = recovery_fixture_with_tool(dir.path(), tool);
        let apply = checked_application(&runtime, &grant, "apply");
        let check=runtime.execute("run",decode("Action",json!({"operation_id":"apply/check/0","kind":"check","tool":"check","branch_id":apply.branch_id})).unwrap()).unwrap();
        assert_eq!(check.status, EffectStatus::Succeeded);
        if state == "legacy" {
            runtime.store.db.execute_batch("UPDATE effects SET body=json_remove(body,'$.validations','$.restored_validity') WHERE id='apply/check/0'").unwrap();
        }
        if state == "changed-checker" {
            std::fs::write(&code, format!("{script}# code revision\n")).unwrap();
        }
        let receipt = runtime.execute("run", apply).unwrap();
        let expected = match state {
            "current" => EffectStatus::Succeeded,
            "legacy" => EffectStatus::Denied,
            _ => EffectStatus::Stale,
        };
        assert_eq!(receipt.status, expected, "check state: {state}");
        assert_eq!(
            std::fs::read_to_string(counter).unwrap(),
            "run\n",
            "Lookup must not execute a check again to manufacture current evidence"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("artifact.txt")).unwrap(),
            if state == "current" {
                "desired output"
            } else {
                "original"
            }
        );
    }
}

#[test]
fn schema_thirteen_preserves_uncertified_captured_checks_without_restoring_validity() {
    let dir = recovery_files();
    let (runtime, grant) = recovery_fixture(dir.path());
    dependency(&runtime, &grant, "input.txt", "artifact.txt");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_finalization BEFORE INSERT ON events WHEN NEW.producer='ribosome-host' BEGIN SELECT RAISE(FAIL,'injected check finalization failure'); END").unwrap();
    assert!(
        runtime
            .execute(
                "run",
                decode(
                    "Action",
                    json!({"operation_id":"legacy-check","kind":"check","tool":"check"})
                )
                .unwrap()
            )
            .is_err()
    );
    runtime.store.db.execute_batch(&[include_str!("../tests/fixtures/remove-budget-schema.sql"), "UPDATE effects SET observation=json_remove(json_set(observation,'$.validates',json('[\"artifact.txt\"]')),'$.preflight','$.receipt.validations','$.receipt.restored_validity') WHERE id='legacy-check'; ALTER TABLE effects DROP COLUMN settlement; DROP TABLE property_validations; DROP TABLE artifact_invalidation_generations; ALTER TABLE artifact_snapshots DROP COLUMN version_only; ALTER TABLE effects DROP COLUMN preflight; PRAGMA user_version=12"].concat()).unwrap();
    let before: String = runtime
        .store
        .db
        .query_row(
            "SELECT observation FROM effects WHERE id='legacy-check'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(runtime);
    let (runtime, _) = recovery_fixture(dir.path());
    assert_eq!(
        runtime
            .store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        runtime
            .store
            .db
            .query_row(
                "SELECT observation FROM effects WHERE id='legacy-check'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        before,
        "Migration preserves captured observations without supplementing historical evidence"
    );
    let receipt = runtime.lookup("run", "legacy-check").unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(receipt.validations, None);
    assert_eq!(receipt.restored_validity, Some(vec![]));
    assert_eq!(stale_paths(&runtime), vec!["artifact.txt"]);
}

fn property_record(
    store: &Store,
    grant: &Grant,
    description: &str,
    paths: &[&str],
) -> RecordEnvelope {
    store.submit(grant,&decode("RecordSubmission",json!({"kind":"obligation","provenance":{"origin":"observed","source_refs":[],"scenario_family":"property-validation","split":"development","limitations":[]},"body":{"description":description,"owner":"host","subject":"artifact.txt","state":"open","affected_outputs":paths,"created_ms":"1","consequence_boundary":"handoff","evidence_refs":[]}})).unwrap(),false).unwrap()
}

#[test]
fn artifact_validation_does_not_clear_unproven_obligation_properties() {
    let dir = recovery_files();
    let (mut runtime, grant) = recovery_fixture(dir.path());
    drop(runtime.host);
    runtime.host = std::sync::Arc::new(
        LocalHost::new(
            dir.path(),
            BTreeMap::from([("check".into(), recovery_tool())]),
        )
        .unwrap(),
    );
    property_record(
        &runtime.store,
        &grant,
        "Arithmetic is correct",
        &["artifact.txt"],
    );
    property_record(
        &runtime.store,
        &grant,
        "Attribution is complete",
        &["artifact.txt"],
    );
    dependency(&runtime, &grant, "input.txt", "artifact.txt");
    dependency(&runtime, &grant, "artifact.txt", "consumer.txt");
    runtime
        .store
        .invalidate_dependents(&grant.scope, "input.txt")
        .unwrap();
    let action = checked_application(&runtime, &grant, "apply");
    let receipt = runtime.execute("run", action).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(
        stale_paths(&runtime),
        vec!["artifact.txt", "consumer.txt"],
        "A declared artifact check without property coverage cannot clear the outstanding property invalidation"
    );
}
