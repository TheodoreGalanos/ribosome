use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn request(run: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","prompt":"Inspect work"})).unwrap()
}
fn fixture() -> (tempfile::TempDir, Runtime, Grant) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("report.txt"), "abéx").unwrap();
    let grant:Grant=decode("Grant",json!({"id":"owner","scope":{"client":"client","project":"project"},"mode":"observe","paths":["report.txt"],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":10,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":5,"max_work_items":5,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"context","visible_splits":["development"],"allow_export":false})).unwrap();
    let store = Store::open(dir.path().join("state.db")).unwrap();
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(dir.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), dir.path().join("runtime")).unwrap();
    runtime
        .store
        .begin_run("run", &grant.id, &request("run"))
        .unwrap();
    (dir, runtime, grant)
}
fn observe(runtime: &Runtime) -> Value {
    runtime.tool_call("run","tool.call",json!({"call_id":"read","method":"artifact.read","arguments":{"path":"report.txt","offset":0,"length":1000}})).unwrap()
}
fn retained(
    runtime: &Runtime,
    run: &str,
    path: &str,
    extra: Value,
) -> ribosome_core::error::Result<Value> {
    let mut params = json!({"path":path,"offset":0,"length":1000});
    params
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    runtime.tool_call(run, "artifact.read", params)
}

#[test]
fn retained_result_retries_preserve_original_observation_and_reject_changed_identity() {
    let (dir, runtime, _) = fixture();
    let observation = observe(&runtime);
    std::fs::write(dir.path().join("report.txt"), "new bytes").unwrap();
    assert_eq!(observe(&runtime), observation);
    assert_eq!(
        runtime
            .tool_call("run", "tool.result", json!({"id":"read"}))
            .unwrap(),
        observation
    );
    assert_eq!(runtime.tool_call("run","tool.call",json!({"call_id":"read","method":"artifact.read","arguments":{"path":"report.txt","offset":1,"length":1000}})).unwrap_err().code,-32002);
    let path = observation["artifact"]["path"].as_str().unwrap();
    let snapshot = path.strip_prefix("ribosome-result:").unwrap();
    let full = retained(&runtime, "run", path, json!({})).unwrap();
    let text = full["content"].as_str().unwrap();
    assert_eq!(text, observation["content"].as_str().unwrap());
    assert_eq!(full["required_freshness"], "historical");
    assert_eq!(
        retained(
            &runtime,
            "run",
            path,
            json!({"required_freshness":"current"})
        )
        .unwrap_err()
        .code,
        -32002
    );
    assert_eq!(
        retained(
            &runtime,
            "run",
            "report.txt",
            json!({"snapshot_id":snapshot})
        )
        .unwrap_err()
        .code,
        -32001
    );
    let physical: Value = serde_json::from_str(observation["content"].as_str().unwrap()).unwrap();
    let wrong = format!(
        "ribosome-result:{}",
        physical["snapshot_id"].as_str().unwrap()
    );
    assert_eq!(
        retained(&runtime, "run", &wrong, json!({}))
            .unwrap_err()
            .code,
        -32001
    );
    assert_eq!(
        retained(&runtime, "run", path, json!({"snapshot_id":"different"}))
            .unwrap_err()
            .code,
        -32001
    );
    let byte = text.find('é').unwrap();
    assert_eq!(
        retained(&runtime, "run", path, json!({"offset":byte,"length":2})).unwrap()["content"],
        "é"
    );
    assert_eq!(
        retained(&runtime, "run", path, json!({"offset":byte+1,"length":2}))
            .unwrap_err()
            .code,
        -32602
    );
    assert_eq!(
        retained(&runtime, "run", path, json!({"offset":byte,"length":1}))
            .unwrap_err()
            .code,
        -32602
    );
    assert!(
        retained(&runtime, "run", path, json!({"offset":text.len()})).unwrap()["eof"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn result_artifact_enforces_transitive_grants_and_deletion_before_cleanup() {
    let (dir, runtime, grant) = fixture();
    let observation = observe(&runtime);
    let path = observation["artifact"]["path"].as_str().unwrap();
    for (id, other_client) in [("other", true), ("narrow", false)] {
        let mut scoped = grant.clone();
        scoped.id = id.into();
        if other_client {
            scoped.scope.client = "different".into();
        } else {
            scoped.paths.clear();
        }
        runtime.store.register_grant(&scoped).unwrap();
        runtime
            .store
            .begin_run(id, &scoped.id, &request(id))
            .unwrap();
        assert_eq!(
            retained(&runtime, id, path, json!({})).unwrap_err().code,
            -32001
        );
        assert_eq!(
            runtime
                .tool_call(id, "tool.result", json!({"id":"read"}))
                .unwrap_err()
                .code,
            -32004
        );
    }
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_result_cleanup BEFORE UPDATE OF result_content ON artifact_snapshots BEGIN SELECT RAISE(FAIL,'injected result cleanup failure'); END").unwrap();
    std::fs::remove_file(dir.path().join("report.txt")).unwrap();
    assert_eq!(runtime.tool_call("run", "tool.call", json!({"call_id":"read","method":"artifact.read","arguments":{"path":"report.txt","offset":0,"length":1000}})).unwrap_err().code, -32001);
    assert_eq!(
        retained(&runtime, "run", path, json!({})).unwrap_err().code,
        -32001
    );
    assert_eq!(
        runtime
            .tool_call("run", "tool.result", json!({"id":"read"}))
            .unwrap_err()
            .code,
        -32001
    );
    assert!(
        db.query_row(
            "SELECT result_content IS NOT NULL FROM artifact_snapshots WHERE path=?1",
            [path],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    db.execute_batch("DROP TRIGGER fail_result_cleanup")
        .unwrap();
    runtime.store.cleanup_sources(&grant, 100).unwrap();
    assert!(
        db.query_row(
            "SELECT result_content IS NULL FROM artifact_snapshots WHERE path=?1",
            [path],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
}

#[test]
fn schema_eight_preserves_legacy_snapshots_and_rolls_back_partial_result_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    for migration in [
        include_str!("../src/store.sql"),
        include_str!("../src/migration-2.sql"),
        include_str!("../src/migration-3.sql"),
        include_str!("../src/migration-4.sql"),
        include_str!("../src/migration-5.sql"),
        include_str!("../src/migration-6.sql"),
        include_str!("../src/migration-7.sql"),
    ] {
        db.execute_batch(migration).unwrap();
    }
    let body = json!({"artifact":{"path":"report.txt","version":"v1"},"content":"legacy saved observation","offset":0,"total_bytes":"24","eof":true}).to_string();
    db.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,body) VALUES('snapshot','client','project','owner','report.txt','development','v1','v1','historical',?1)",[&body]).unwrap();
    db.execute_batch("CREATE INDEX artifact_result_call ON artifact_snapshots(path)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('artifact_snapshots') WHERE name LIKE 'result_%'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    db.execute_batch("DROP INDEX artifact_result_call").unwrap();
    let _store = Store::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM artifact_snapshots WHERE id='snapshot'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        body
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM artifact_snapshots WHERE result_run_id IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("legacy.db.before-schema-7-")
                .then_some(entry.path())
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 2);
    for backup in backups {
        let saved = rusqlite::Connection::open(backup).unwrap();
        assert_eq!(
            saved
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            saved
                .query_row(
                    "SELECT body FROM artifact_snapshots WHERE id='snapshot'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            body
        );
    }
}

#[test]
fn reading_a_large_result_returns_bytes_instead_of_another_excerpt_reference() {
    let (dir, runtime, _) = fixture();
    let original = "\\\"\né".repeat(12000);
    std::fs::write(dir.path().join("report.txt"), &original).unwrap();
    let observation=runtime.tool_call("run","tool.call",json!({"call_id":"large","method":"artifact.read","arguments":{"path":"report.txt","offset":0,"length":65536}})).unwrap();
    let mut restored = String::new();
    loop {
        let offset = restored.len();
        let page=runtime.tool_call("run","tool.call",json!({"call_id":format!("page-{offset}"),"method":"artifact.read","arguments":{"path":observation["artifact"]["path"],"offset":offset,"length":65536}})).unwrap();
        let chunk: Value = serde_json::from_str(page["content"].as_str().unwrap()).unwrap();
        let text = chunk["content"].as_str().expect(
            "A result artifact read must return actual bytes, not another excerpt reference",
        );
        assert!(!text.is_empty() && text.len() <= 8192);
        assert_eq!(chunk["artifact"], observation["artifact"]);
        assert_eq!(chunk["offset"], offset);
        restored.push_str(text);
        if chunk["eof"] == true {
            break;
        }
        assert!(restored.len() < 200000, "Reader must make bounded progress");
    }
    assert_eq!(
        restored.len().to_string(),
        observation["total_bytes"].as_str().unwrap()
    );
    let original_chunk: Value = serde_json::from_str(&restored).unwrap();
    assert_eq!(original_chunk["content"], original);
}

#[test]
fn training_export_accepts_authorized_artifact_lineage_and_rechecks_source_deletion() {
    let (dir, runtime, mut grant) = fixture();
    grant.id = "export-grant".into();
    grant.allow_export = true;
    runtime.store.register_grant(&grant).unwrap();
    runtime
        .store
        .begin_run("export-run", &grant.id, &request("export-run"))
        .unwrap();
    let observed=runtime.tool_call("export-run","tool.call",json!({"call_id":"source","method":"artifact.read","arguments":{"path":"report.txt","offset":0,"length":1000}})).unwrap();
    let source = observed["sources"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap();
    let record=runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[source],"scenario_family":"export-source","split":"development","limitations":[]},"body":{"subject":"report.txt","observation":"An authorized artifact observation","interpretation":"A source-derived finding","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap(),false).unwrap();
    let selection = ExportRequest {
        record_ids: vec![record.id],
        product: Origin::Observed,
    };
    let exported = runtime
        .export_training("export-run", &selection)
        .expect("Available tool-result and workspace snapshots are valid export lineage");
    assert_eq!(exported.count, 1);
    assert!(
        std::fs::read_to_string(&exported.path)
            .unwrap()
            .contains("source-derived finding")
    );
    std::fs::remove_file(dir.path().join("report.txt")).unwrap();
    assert!(runtime.export_training("export-run", &selection).is_err());
    assert!(
        !std::path::Path::new(&exported.path).exists(),
        "Deletion must remove the existing managed export file"
    );
}

#[test]
fn retained_observations_keep_source_ancestry_after_record_revision() {
    let (dir, runtime, grant) = fixture();
    let source=runtime.store.submit(&grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"revision","split":"development","limitations":[]},"body":{"kind":"episodic","content":"ORIGINAL-LINEAGE-MARKER","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let submission:RecordSubmission=decode("RecordSubmission",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[source.id],"scenario_family":"revision","split":"development","limitations":[]},"body":{"subject":"source","observation":"ORIGINAL-LINEAGE-MARKER","interpretation":"Original interpretation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap();
    let record = runtime.store.submit(&grant, &submission, false).unwrap();
    let observed=runtime.tool_call("run","tool.call",json!({"call_id":"historical-record","method":"record.read","arguments":{"id":record.id}})).unwrap();
    let context = runtime.store.authorize_context("run").unwrap();
    runtime.store.append_context("run",&decode("ContextAppend",json!({"segment_id":context.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","content":"ORIGINAL-LINEAGE-MARKER","timestamp":1},"sources":observed["sources"]}]})).unwrap()).unwrap();
    let mut revised = submission;
    revised.id = Some(record.id.clone());
    revised.expected_version = Some(record.version.clone());
    revised.provenance.source_refs.clear();
    revised.body.insert(
        "observation".into(),
        json!("Independent current observation"),
    );
    runtime.store.submit(&grant, &revised, false).unwrap();
    assert!(
        runtime
            .tool_call("run", "tool.result", json!({"id":"historical-record"}))
            .is_ok(),
        "Revision alone does not withdraw authorized historical evidence"
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(
        runtime.store.record(&grant, &record.id).is_ok(),
        "Independent current revision stays readable"
    );
    assert_eq!(
        runtime
            .tool_call("run", "tool.result", json!({"id":"historical-record"}))
            .unwrap_err()
            .code,
        -32001
    );
    assert_ne!(
        runtime.store.authorize_context("run").unwrap().segment_id,
        context.segment_id
    );
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    assert!(db.query_row("SELECT result_content IS NULL FROM artifact_snapshots WHERE result_call_id='historical-record'",[],|r|r.get::<_,bool>(0)).unwrap());
}

fn revision_chain(runtime: &Runtime, grant: &Grant) -> (RecordEnvelope, RecordEnvelope) {
    let source=runtime.store.submit(grant,&decode("RecordSubmission",json!({"kind":"memory","provenance":{"origin":"observed","source_refs":[],"scenario_family":"revision","split":"development","limitations":[]},"body":{"kind":"episodic","content":"REVISION-SOURCE-MARKER","applicability":"fixture","evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap(),false).unwrap();
    let record=runtime.store.submit(grant,&decode("RecordSubmission",json!({"kind":"finding","provenance":{"origin":"observed","source_refs":[source.id],"scenario_family":"revision","split":"development","limitations":[]},"body":{"subject":"source","observation":"REVISION-SOURCE-MARKER","interpretation":"Source-derived interpretation","evidence_refs":[],"uncertainty":[],"operator":"proofreading@1"}})).unwrap(),false).unwrap();
    (source, record)
}
fn revise_independently(runtime: &Runtime, grant: &Grant, record: &RecordEnvelope) {
    let mut submission:RecordSubmission=decode("RecordSubmission",json!({"id":record.id,"expected_version":record.version,"kind":record.kind,"provenance":record.provenance,"body":record.body})).unwrap();
    submission.provenance.source_refs.clear();
    submission.body.insert(
        "observation".into(),
        json!("Independent current observation"),
    );
    runtime.store.submit(grant, &submission, false).unwrap();
}

#[test]
fn revised_sources_preserve_plain_context_record_and_event_ancestry() {
    let (_dir, runtime, grant) = fixture();
    let (source, record) = revision_chain(&runtime, &grant);
    let state = runtime.store.authorize_context("run").unwrap();
    runtime.store.append_context("run",&decode("ContextAppend",json!({"segment_id":state.segment_id,"after":"0","entries":[{"message":{"role":"toolResult","content":"REVISION-SOURCE-MARKER","timestamp":1},"sources":[{"kind":"record","id":record.id,"version":record.version}]}]})).unwrap()).unwrap();
    let mut submission: RecordSubmission = decode(
        "RecordSubmission",
        json!({"kind":"finding","provenance":record.provenance,"body":record.body}),
    )
    .unwrap();
    submission.provenance.source_refs = vec![record.id.clone()];
    let derivative = runtime.store.submit(&grant, &submission, false).unwrap();
    runtime.store.ingest(&decode("Event",json!({"id":"derived-event","scope":grant.scope,"run_id":"external","producer":"host","sequence":"1","kind":"observation","timestamp_ms":"1","parents":[],"correlation":"test","artifacts":[],"payload":{"content":"REVISION-SOURCE-MARKER"},"provenance":{"origin":"observed","source_refs":[record.id],"scenario_family":"revision","split":"development","limitations":[]}})).unwrap()).unwrap();
    revise_independently(&runtime, &grant, &record);
    assert_eq!(
        runtime.store.authorize_context("run").unwrap().segment_id,
        state.segment_id,
        "Authorized historical content survives revision alone"
    );
    runtime
        .store
        .retire(
            &grant,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(runtime.store.record(&grant, &record.id).is_ok());
    assert!(runtime.store.record(&grant, &derivative.id).is_err());
    assert_ne!(
        runtime.store.authorize_context("run").unwrap().segment_id,
        state.segment_id
    );
    let evidence = runtime
        .store
        .evidence(
            &grant,
            &decode("EvidenceRequest", json!({"cursor":"0","limit":100})).unwrap(),
        )
        .unwrap();
    assert!(
        !serde_json::to_string(&evidence)
            .unwrap()
            .contains("REVISION-SOURCE-MARKER")
    );
}

#[test]
fn observation_retention_rejects_a_revision_changed_after_the_read() {
    let (dir, runtime, grant) = fixture();
    let (_source, record) = revision_chain(&runtime, &grant);
    let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER change_during_retention BEFORE INSERT ON artifact_snapshots WHEN NEW.result_call_id='changed-read' BEGIN UPDATE records SET version='2',body=json_set(body,'$.version','2','$.provenance.source_refs',json('[]')) WHERE id='{}'; DELETE FROM source_edges WHERE subject_kind='record' AND subject_id='{}'; END",record.id,record.id)).unwrap();
    let call =
        json!({"call_id":"changed-read","method":"record.read","arguments":{"id":record.id}});
    let error = runtime
        .tool_call("run", "tool.call", call.clone())
        .unwrap_err();
    assert_eq!(error.code, -32002);
    assert!(
        error
            .message
            .contains("before its observation was retained")
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM artifact_snapshots WHERE result_call_id='changed-read'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        runtime.store.record(&grant, &record.id).unwrap().version,
        "1",
        "The injected write and failed capture roll back together"
    );
    db.execute_batch("DROP TRIGGER change_during_retention")
        .unwrap();
    assert!(runtime.tool_call("run", "tool.call", call).is_ok());
}

#[test]
fn schema_eleven_preserves_proven_ancestry_and_withholds_ambiguous_legacy_copies() {
    for changed_before_upgrade in [false, true] {
        let (dir, runtime, grant) = fixture();
        let (source, record) = revision_chain(&runtime, &grant);
        let saved=runtime.tool_call("run","tool.call",json!({"call_id":"legacy-read","method":"record.read","arguments":{"id":record.id}})).unwrap();
        let snapshot = saved["artifact"]["path"]
            .as_str()
            .unwrap()
            .strip_prefix("ribosome-result:")
            .unwrap();
        let current_only_source = if changed_before_upgrade {
            let mut fresh: RecordSubmission = decode(
                "RecordSubmission",
                json!({"kind":"memory","provenance":source.provenance,"body":source.body}),
            )
            .unwrap();
            fresh
                .body
                .insert("content".into(), json!("Current-only evidence"));
            let fresh = runtime.store.submit(&grant, &fresh, false).unwrap();
            let mut revised:RecordSubmission=decode("RecordSubmission",json!({"id":record.id,"expected_version":record.version,"kind":record.kind,"provenance":record.provenance,"body":record.body})).unwrap();
            revised.provenance.source_refs = vec![fresh.id.clone()];
            revised.body.insert(
                "observation".into(),
                json!("Observation from current-only evidence"),
            );
            runtime.store.submit(&grant, &revised, false).unwrap();
            Some(fresh.id)
        } else {
            None
        };
        let db = rusqlite::Connection::open(dir.path().join("state.db")).unwrap();
        // Schema ten retained only the direct record reference for this result.
        db.execute("DELETE FROM source_edges WHERE subject_kind='artifact' AND subject_id=?1 AND source_id=?2",rusqlite::params![snapshot,source.id]).unwrap();
        db.execute_batch(&[include_str!("fixtures/remove-budget-schema.sql"), "ALTER TABLE effects DROP COLUMN settlement; DROP TABLE property_validations; DROP TABLE artifact_invalidation_generations; ALTER TABLE artifact_snapshots DROP COLUMN version_only; ALTER TABLE effects DROP COLUMN preflight; ALTER TABLE effects DROP COLUMN phase; ALTER TABLE effects DROP COLUMN authority; ALTER TABLE effects DROP COLUMN potential_writes; ALTER TABLE effects DROP COLUMN observation; ALTER TABLE effects DROP COLUMN validation_generation; ALTER TABLE invalidated DROP COLUMN generation; DROP TABLE validity_clock; PRAGMA user_version=10"].concat()).unwrap();
        drop(runtime);
        if changed_before_upgrade {
            db.execute_batch("CREATE TRIGGER fail_lineage_upgrade BEFORE INSERT ON source_tombstones BEGIN SELECT RAISE(FAIL,'injected lineage migration failure'); END").unwrap();
            assert!(Store::open(dir.path().join("state.db")).is_err());
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                10
            );
            assert!(
                db.query_row(
                    "SELECT available FROM artifact_snapshots WHERE id=?1",
                    [snapshot],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
            );
            db.execute_batch("DROP TRIGGER fail_lineage_upgrade")
                .unwrap();
        }
        let runtime = Runtime::new(
            Store::open(dir.path().join("state.db")).unwrap(),
            Box::new(LocalHost::new(dir.path(), BTreeMap::new()).unwrap()),
            dir.path().join("runtime"),
        )
        .unwrap();
        runtime
            .store
            .begin_run("run", &grant.id, &request("run"))
            .unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            21
        );
        assert!(runtime.store.record(&grant, &record.id).is_ok());
        let restored = runtime.tool_call("run", "tool.result", json!({"id":"legacy-read"}));
        if changed_before_upgrade {
            assert_eq!(restored.unwrap_err().code, -32001);
            assert_eq!(db.query_row("SELECT count(*) FROM source_edges WHERE subject_kind='artifact' AND subject_id=?1 AND source_id=?2",rusqlite::params![snapshot,current_only_source.unwrap()],|row|row.get::<_,i64>(0)).unwrap(),0,"Migration must not attach today's new dependencies to the older observation");
            let retained: String = db
                .query_row(
                    "SELECT result_content FROM artifact_snapshots WHERE id=?1",
                    [snapshot],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                retained.contains("REVISION-SOURCE-MARKER"),
                "Migration preserves the original observation without inventing missing history"
            );
        } else {
            assert!(restored.is_ok());
            revise_independently(&runtime, &grant, &record);
            runtime
                .store
                .retire(
                    &grant,
                    &RetireRequest {
                        id: source.id,
                        expected_version: source.version,
                        delete: true,
                    },
                )
                .unwrap();
            assert_eq!(
                runtime
                    .tool_call("run", "tool.result", json!({"id":"legacy-read"}))
                    .unwrap_err()
                    .code,
                -32001
            );
        }
    }
}
