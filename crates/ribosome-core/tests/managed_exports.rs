use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn request(run: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect work"})).unwrap()
}
fn open(dir: &Path) -> Runtime {
    Runtime::new(
        Store::open(dir.join("state.db")).unwrap(),
        Box::new(LocalHost::new(dir, BTreeMap::new()).unwrap()),
        dir.join("runtime"),
    )
    .unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Runtime,
    Grant,
    RecordEnvelope,
    RecordEnvelope,
) {
    let dir = tempfile::tempdir().unwrap();
    let runtime = open(dir.path());
    let grant:Grant=decode("Grant",json!({"id":"owner","scope":{"client":"client","project":"project"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":5,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"exports","visible_splits":["development"],"allow_export":true})).unwrap();
    runtime.store.register_grant(&grant).unwrap();
    runtime
        .store
        .begin_run("run", &grant.id, &request("run"))
        .unwrap();
    let source=runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"export","split":"development","limitations":[]},"body":{"kind":"episodic","content":"EXPORTED-SOURCE-MARKER é","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let record=runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[source.id],"scenario_family":"export","split":"development","limitations":[]},"body":{"subject":"source","observation":"EXPORTED-SOURCE-MARKER é","interpretation":"Derived observation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap(),false).unwrap();
    (dir, runtime, grant, source, record)
}
fn selection(record: &RecordEnvelope) -> ExportRequest {
    ExportRequest {
        record_ids: vec![record.id.clone()],
        product: Origin::Observed,
    }
}
fn read(
    runtime: &Runtime,
    run: &str,
    export: &ExportResult,
) -> ribosome_core::error::Result<Value> {
    runtime.tool_call(
        run,
        "artifact.read",
        json!({"path":export.artifact.path,"offset":0,"length":65536}),
    )
}
fn retire(runtime: &Runtime, grant: &Grant, source: &RecordEnvelope) {
    runtime
        .store
        .retire(
            grant,
            &RetireRequest {
                id: source.id.clone(),
                expected_version: source.version.clone(),
                delete: true,
            },
        )
        .unwrap();
}

#[test]
fn export_reads_are_denied_while_file_cleanup_fails_and_after_restart() {
    let (dir, runtime, grant, source, record) = fixture();
    let export = runtime.export_training("run", &selection(&record)).unwrap();
    let original = std::fs::read_to_string(&export.path).unwrap();
    let chunk = read(&runtime, "run", &export).unwrap();
    assert_eq!(chunk["content"], original);
    assert_eq!(
        chunk["artifact"],
        serde_json::to_value(&export.artifact).unwrap()
    );
    let snapshot = export
        .artifact
        .path
        .strip_prefix("ribosome-export:")
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER stop_export_cleanup BEFORE INSERT ON source_cleanup_items WHEN NEW.id='{snapshot}' BEGIN SELECT RAISE(FAIL,'injected cleanup interruption'); END")).unwrap();
    retire(&runtime, &grant, &source);
    assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
    assert_eq!(
        std::fs::read_to_string(&export.path).unwrap(),
        original,
        "The regression must exercise logical denial while physical content still exists"
    );
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "failed"
    );
    drop(runtime);
    let runtime = open(dir.path());
    runtime
        .store
        .begin_run("run", &grant.id, &request("run"))
        .unwrap();
    assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
    db.execute_batch("DROP TRIGGER stop_export_cleanup")
        .unwrap();
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    assert!(!Path::new(&export.path).exists());
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "complete"
    );
}

#[test]
fn exports_keep_the_reviewed_ancestry_when_the_selected_record_changes() {
    let (_dir, runtime, grant, source, record) = fixture();
    let export = runtime.export_training("run", &selection(&record)).unwrap();
    let mut changed:RecordSubmission=decode("RecordSubmission",json!({"id":record.id,"expected_version":record.version,"kind":"finding","provenance":record.provenance,"body":record.body})).unwrap();
    changed.provenance.source_refs.clear();
    changed
        .body
        .insert("observation".into(), json!("A new independent observation"));
    runtime.store.submit(&grant, &changed, false).unwrap();
    retire(&runtime, &grant, &source);
    assert!(
        runtime.store.record(&grant, &record.id).is_ok(),
        "The current record no longer depends on the withdrawn source"
    );
    assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
    assert!(
        !Path::new(&export.path).exists(),
        "The older exported bytes still depend on that source"
    );
}

#[test]
fn export_artifacts_enforce_scope_capability_integrity_and_file_ownership() {
    let (dir, runtime, grant, _source, record) = fixture();
    let export = runtime.export_training("run", &selection(&record)).unwrap();
    for (name, other_client, allow_export) in
        [("no-export", false, false), ("other-client", true, true)]
    {
        let mut restricted = grant.clone();
        restricted.id = name.into();
        restricted.allow_export = allow_export;
        if other_client {
            restricted.scope.client = name.into();
        }
        runtime.store.register_grant(&restricted).unwrap();
        runtime
            .store
            .begin_run(name, &restricted.id, &request(name))
            .unwrap();
        assert_eq!(read(&runtime, name, &export).unwrap_err().code, -32001);
    }
    assert_eq!(runtime.tool_call("run","artifact.read",json!({"path":export.artifact.path,"offset":0,"length":100,"required_freshness":"current"})).unwrap_err().code,-32002);
    std::fs::write(&export.path, "Altered product").unwrap();
    assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
    // A filesystem replacement cannot turn cleanup into deletion of its target.
    let external = dir.path().join("owner-file.txt");
    std::fs::write(&external, "owner data").unwrap();
    std::fs::remove_file(&export.path).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&external, &export.path).unwrap();
        assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
        assert_eq!(std::fs::read_to_string(&external).unwrap(), "owner data");
        assert!(!Path::new(&export.path).exists());
    }
}

#[test]
fn failed_export_publication_and_interrupted_file_writes_never_become_ready() {
    let (dir, runtime, grant, _source, record) = fixture();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_ready BEFORE UPDATE OF available ON artifact_snapshots WHEN NEW.export_file IS NOT NULL AND NEW.available=1 BEGIN SELECT RAISE(FAIL,'injected ready commit failure'); END").unwrap();
    assert!(runtime.export_training("run", &selection(&record)).is_err());
    assert_eq!(
        std::fs::read_dir(runtime.state_dir.join("exports"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM artifact_snapshots WHERE export_file IS NOT NULL AND available=1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_ready").unwrap();
    let export = runtime.export_training("run", &selection(&record)).unwrap();
    // Persist the state at the crash boundary: registered manifest, partial
    // file, and no committed ready flag. Reopen must remove it, not publish it.
    let snapshot = export
        .artifact
        .path
        .strip_prefix("ribosome-export:")
        .unwrap();
    db.execute(
        "UPDATE artifact_snapshots SET available=0 WHERE id=?1",
        [snapshot],
    )
    .unwrap();
    std::fs::write(&export.path, "{partial export").unwrap();
    drop(runtime);
    let runtime = open(dir.path());
    runtime
        .store
        .begin_run("run", &grant.id, &request("run"))
        .unwrap();
    assert!(!Path::new(&export.path).exists());
    assert_eq!(read(&runtime, "run", &export).unwrap_err().code, -32001);
    assert!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job["status"] == "complete")
    );
}

#[test]
fn export_tool_observations_carry_lineage_and_missing_files_rebuild_context() {
    let (_dir, runtime, grant, _source, record) = fixture();
    let observed = runtime
        .tool_call(
            "run",
            "tool.call",
            json!({"call_id":"export","method":"training.export","arguments":selection(&record)}),
        )
        .unwrap();
    let export: ExportResult = serde_json::from_str(observed["content"].as_str().unwrap()).unwrap();
    assert_eq!(observed["sources"].as_array().unwrap().len(), 2);
    let state = runtime.store.authorize_context("run").unwrap();
    runtime.store.append_context("run",&decode("ContextAppend",json!({"segment_id":state.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","content":"EXPORTED-SOURCE-MARKER","timestamp":1},"sources":observed["sources"]}]})).unwrap()).unwrap();
    std::fs::remove_file(&export.path).unwrap();
    let next = runtime
        .tool_call("run", "session.context", json!({}))
        .unwrap();
    assert_ne!(next["segment_id"], state.segment_id);
    assert!(!next.to_string().contains("EXPORTED-SOURCE-MARKER"));
    assert!(
        runtime
            .tool_call("run", "tool.result", json!({"id":"export"}))
            .is_err()
    );
    assert!(runtime.store.record(&grant, &record.id).is_ok());
}

#[test]
fn schema_ten_rolls_back_partial_export_migration_and_preserves_legacy_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    for migration in [
        include_str!("../src/store.sql"),
        include_str!("../src/migration-2.sql"),
        include_str!("../src/migration-3.sql"),
        include_str!("../src/migration-4.sql"),
        include_str!("../src/migration-5.sql"),
        include_str!("../src/migration-6.sql"),
        include_str!("../src/migration-7.sql"),
        include_str!("../src/migration-8.sql"),
        include_str!("../src/migration-9.sql"),
    ] {
        db.execute_batch(migration).unwrap();
    }
    db.execute_batch("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,body) VALUES('legacy','client','project','owner','source','development','1','1','historical','{\"content\":\"legacy observation\"}'); CREATE INDEX artifact_export_file ON artifact_snapshots(path)").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('artifact_snapshots') WHERE name='export_file'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP INDEX artifact_export_file").unwrap();
    let _store = Store::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM artifact_snapshots WHERE id='legacy'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "{\"content\":\"legacy observation\"}"
    );
    let exports = dir.path().join("runtime/exports");
    std::fs::create_dir_all(&exports).unwrap();
    let legacy = exports.join("unregistered.jsonl");
    std::fs::write(&legacy, "Legacy owner-held file without a manifest").unwrap();
    let _runtime = open(dir.path());
    assert_eq!(
        std::fs::read_to_string(legacy).unwrap(),
        "Legacy owner-held file without a manifest",
        "Unknown files must not be deleted or granted fabricated source identities"
    );
}

#[test]
fn released_access_edges_do_not_allow_protected_ancestry_into_exports() {
    let (dir, runtime, grant, source, record) = fixture();
    let mut wide = grant.clone();
    wide.id = "wide".into();
    wide.visible_splits.push(Split::Holdout);
    runtime.store.register_grant(&wide).unwrap();
    runtime
        .store
        .begin_run("wide", &wide.id, &request("wide"))
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    // Retained host evidence can include released access edges. Its complete
    // ancestry still forbids training export, even for a broad reader grant.
    db.execute("UPDATE records SET split='holdout',body=json_set(body,'$.provenance.split','holdout') WHERE id=?1",[&source.id]).unwrap();
    db.execute_batch("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,body,result_run_id) VALUES('protected-observation','client','project','wide','ribosome-result:protected-observation','development','v1','v1','historical','{}','wide')").unwrap();
    db.execute("UPDATE source_edges SET source_kind='artifact',source_id='protected-observation' WHERE subject_kind='record' AND subject_id=?1",[&record.id]).unwrap();
    db.execute("INSERT INTO source_edges(subject_kind,subject_id,source_kind,source_id,requires_access) VALUES('artifact','protected-observation','record',?1,0)",[&source.id]).unwrap();
    assert!(runtime.store.record(&wide, &record.id).is_ok());
    let error = runtime
        .export_training("wide", &selection(&record))
        .unwrap_err();
    assert_eq!(error.code, -32001);
    assert!(error.message.contains("holdout"));
    assert_eq!(
        std::fs::read_dir(runtime.state_dir.join("exports"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn oversized_export_selection_fails_before_writing_a_manifest_or_file() {
    let (dir, runtime, grant, _source, record) = fixture();
    let mut changed:RecordSubmission=decode("RecordSubmission",json!({"id":record.id,"expected_version":record.version,"kind":"finding","provenance":record.provenance,"body":record.body})).unwrap();
    changed
        .body
        .insert("observation".into(), json!("x".repeat(65536)));
    let record = runtime.store.submit(&grant, &changed, false).unwrap();
    let request = ExportRequest {
        record_ids: vec![record.id.clone(); 300],
        product: Origin::Observed,
    };
    let error = runtime.export_training("run", &request).unwrap_err();
    assert!(error.message.contains("16 MiB"));
    assert_eq!(
        std::fs::read_dir(runtime.state_dir.join("exports"))
            .unwrap()
            .count(),
        0
    );
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM artifact_snapshots WHERE export_file IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert!(runtime.export_training("run", &selection(&record)).is_ok());
}

#[test]
fn deletion_removes_scoped_legacy_exports_without_removing_foreign_or_unrecognized_files() {
    let (dir, runtime, grant, source, record) = fixture();
    let exports = dir.path().join("runtime/exports");
    let legacy = exports.join(format!("{}.jsonl", ribosome_core::validation::id()));
    std::fs::write(
        &legacy,
        format!(
            "{}\n",
            json!({"product":"observed","scope":grant.scope,"record":record})
        ),
    )
    .unwrap();
    let foreign = exports.join(format!("{}.jsonl", ribosome_core::validation::id()));
    let mut foreign_record = record.clone();
    foreign_record.scope.client = "another-client".into();
    std::fs::write(
        &foreign,
        format!(
            "{}\n",
            json!({"product":"observed","scope":foreign_record.scope,"record":foreign_record})
        ),
    )
    .unwrap();
    let unrelated = exports.join(format!("{}.jsonl", ribosome_core::validation::id()));
    std::fs::write(&unrelated, "{\"owner_document\":true}\n").unwrap();
    drop(runtime);
    let runtime = open(dir.path());
    assert!(
        legacy.exists(),
        "opening old state must preserve its observations"
    );
    retire(&runtime, &grant, &source);
    assert!(
        !legacy.exists(),
        "source deletion left an untracked legacy export behind"
    );
    assert!(foreign.exists());
    assert!(unrelated.exists());
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    assert_eq!(
        runtime.store.source_cleanup_status(&grant).unwrap()["jobs"][0]["status"],
        "complete"
    );
}
