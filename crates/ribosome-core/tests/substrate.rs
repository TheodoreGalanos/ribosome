use ribosome_core::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn grant(mode: &str) -> Grant {
    decode("Grant",json!({"id":"grant-one","scope":{"client":"client-a","project":"project-a"},"mode":mode,"paths":["report.txt","source.txt","unsafe.txt"],"tools":[],"profiles":["caretaker","curator","experimenter"],"budget":{"max_calls":3,"max_tokens":"10000","max_cost_microusd":"100000","max_actions":20,"max_work_items":4,"max_depth":2,"deadline_ms":(now_ms()+60000).to_string()},"context":"reports","visible_splits":["development"],"allow_export":false})).unwrap()
}
fn request(run: &str) -> AgentRunRequest {
    decode("AgentRunRequest",json!({"run_id":run,"profile":"caretaker","operator":"proofreading@1","prompt":"Inspect the report","provider":"openai","model":"test-model"})).unwrap()
}
fn fixture(mode: &str) -> (tempfile::TempDir, Runtime, Grant) {
    fixture_with_tools(mode, BTreeMap::new())
}
fn fixture_with_tools(
    mode: &str,
    tools: BTreeMap<String, ribosome_core::host::RegisteredTool>,
) -> (tempfile::TempDir, Runtime, Grant) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("report.txt"), "original").unwrap();
    std::fs::write(directory.path().join("source.txt"), "source").unwrap();
    let store = Store::open(directory.path().join("state.db")).unwrap();
    let mut grant = grant(mode);
    grant.tools = tools.keys().cloned().collect();
    if !tools.is_empty() {
        grant.required_checks = Some(grant.tools.clone());
        grant.writable_paths = Some(vec!["report.txt".into()]);
    }
    store.register_grant(&grant).unwrap();
    let host = LocalHost::new(directory.path(), tools).unwrap();
    let runtime = Runtime::new(store, Box::new(host), directory.path().join("state")).unwrap();
    runtime
        .store
        .begin_run("run-one", &grant.id, &request("run-one"))
        .unwrap();
    (directory, runtime, grant)
}
fn action(value: Value) -> Action {
    decode("Action", value).unwrap()
}

#[test]
fn session_grant_is_bound_to_the_run_and_cannot_be_overridden() {
    let (_directory, runtime, grant) = fixture("apply");
    let value = runtime
        .tool_call("run-one", "session.grant", json!({}))
        .unwrap();
    assert_eq!(value, serde_json::to_value(grant).unwrap());
    assert!(
        runtime
            .tool_call(
                "run-one",
                "session.grant",
                json!({"grant_id":"another-grant"})
            )
            .is_err()
    );
    assert!(
        runtime
            .tool_call("another-run", "session.grant", json!({}))
            .is_err()
    );
}
fn search(query: &str) -> SearchRequest {
    decode(
        "SearchRequest",
        json!({"query":query,"inventory":"evidence","limit":20,"offset":0}),
    )
    .unwrap()
}
fn provenance() -> Value {
    json!({"origin":"observed","source_refs":[],"scenario_family":"unit-test","split":"development","limitations":[]})
}
fn memory() -> RecordSubmission {
    decode("RecordSubmission",json!({"kind":"memory","provenance":provenance(),"body":{"kind":"failure","content":"Endpoint unavailable","applicability":"during the current run","evidence_refs":[],"counterexamples":["A slow request is not evidence of failure"],"responses":["inspect status"],"regression_cases":[],"conflicts":[],"supersedes":[]}})).unwrap()
}

#[test]
fn invalid_memory_reference_is_identified_and_not_saved() {
    let (_d, r, g) = fixture("observe");
    let mut input = memory();
    input.body.insert(
        "conflicts".into(),
        json!(["uncertainty is prose, not a record ID"]),
    );
    let error = r.store.submit(&g, &input, false).unwrap_err();
    assert!(
        error
            .message
            .contains("uncertainty is prose, not a record ID")
    );
    assert!(r.store.search(&g, &search("")).unwrap().records.is_empty());
}

#[test]
fn events_deduplicate_preserve_local_order_and_safe_counters() {
    let (_d, r, g) = fixture("observe");
    let make = |id: &str, sequence: &str| {
        decode::<Event>("Event",json!({"id":id,"scope":g.scope,"run_id":"host-run","producer":"worker-1","sequence":sequence,"kind":"handoff","timestamp_ms":"1","parents":[],"correlation":"task-1","artifacts":[],"payload":{},"provenance":provenance()})).unwrap()
    };
    let first = make("event-two", "9007199254740993");
    assert!(r.store.ingest(&first).unwrap());
    assert!(!r.store.ingest(&first).unwrap());
    assert!(r.store.ingest(&make("event-one", "1")).unwrap());
    let page = r
        .store
        .evidence(
            &g,
            &decode("EvidenceRequest", json!({"cursor":"0","limit":10})).unwrap(),
        )
        .unwrap();
    assert_eq!(page.events.len(), 2);
    assert_eq!(page.frontier["host-run/worker-1"], "9007199254740993");
    let mut changed = first;
    changed.payload.insert("changed".into(), json!(true));
    assert!(r.store.ingest(&changed).is_err());
}

#[test]
fn scope_filter_retirement_expiry_and_search_index_invalidation() {
    let (_d, r, g) = fixture("observe");
    let entry = r.store.submit(&g, &memory(), false).unwrap();
    assert_eq!(
        r.store
            .search(&g, &search("Endpoint"))
            .unwrap()
            .records
            .len(),
        1
    );
    let mut other = g.clone();
    other.scope.client = "client-b".into();
    assert!(
        r.store
            .search(&other, &search("Endpoint"))
            .unwrap()
            .records
            .is_empty()
    );
    assert!(r.store.record(&other, &entry.id).is_err());
    r.store
        .retire(
            &g,
            &RetireRequest {
                id: entry.id.clone(),
                expected_version: entry.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(
        r.store
            .search(&g, &search("Endpoint"))
            .unwrap()
            .records
            .is_empty()
    );
    assert!(r.store.record(&g, &entry.id).is_err());
    let mut expired = memory();
    expired.body.insert("expires_ms".into(), json!("1"));
    r.store.submit(&g, &expired, false).unwrap();
    assert!(
        r.store
            .search(&g, &search("Endpoint"))
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn edits_are_scoped_versioned_and_duplicate_operations_do_not_repeat() {
    let (d, r, g) = fixture("apply");
    let before = r.host.version(&g, "report.txt", None).unwrap();
    let edit = action(
        json!({"operation_id":"edit-1","kind":"edit","path":"report.txt","expected_version":before.version,"content":"corrected"}),
    );
    let receipt = r.execute("run-one", edit.clone()).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    std::fs::write(d.path().join("report.txt"), "later edit").unwrap();
    assert_eq!(r.execute("run-one", edit.clone()).unwrap(), receipt);
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "later edit"
    );
    let mut collision = edit.clone();
    collision.content = Some("different".into());
    assert!(r.execute("run-one", collision).is_err());
    let mut stale = edit;
    stale.operation_id = "edit-2".into();
    assert_eq!(
        r.execute("run-one", stale).unwrap().status,
        EffectStatus::Stale
    );
    let outside = action(
        json!({"operation_id":"edit-3","kind":"edit","path":"../secret","expected_version":"absent","content":"bad"}),
    );
    assert_eq!(
        r.execute("run-one", outside).unwrap().status,
        EffectStatus::Denied
    );
}

#[test]
fn observe_denies_effects_and_sandbox_requires_branch() {
    let (_d, r, _) = fixture("observe");
    assert_eq!(
        r.execute(
            "run-one",
            action(json!({"operation_id":"branch","kind":"branch"}))
        )
        .unwrap()
        .status,
        EffectStatus::Denied
    );
    let (d, r, g) = fixture("sandbox");
    let before = r.host.version(&g, "report.txt", None).unwrap();
    let mut edit = action(
        json!({"operation_id":"edit","kind":"edit","path":"report.txt","expected_version":before.version,"content":"branch edit"}),
    );
    assert_eq!(
        r.execute("run-one", edit.clone()).unwrap().status,
        EffectStatus::Denied
    );
    let branch = r
        .execute(
            "run-one",
            action(json!({"operation_id":"branch","kind":"branch"})),
        )
        .unwrap();
    assert_eq!(branch.status, EffectStatus::Succeeded);
    edit.operation_id = "edit-in-branch".into();
    edit.branch_id = Some(branch.output);
    assert_eq!(
        r.execute("run-one", edit).unwrap().status,
        EffectStatus::Succeeded
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "original"
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_denied() {
    let (d, r, g) = fixture("apply");
    std::os::unix::fs::symlink("/etc/passwd", d.path().join("unsafe.txt")).unwrap();
    assert!(r.host.version(&g, "unsafe.txt", None).is_err());
}

#[test]
fn grants_and_checkpoints_cannot_change_pinned_authority() {
    let (_d, r, g) = fixture("observe");
    let mut forged = g;
    forged.mode = Mode::Apply;
    assert!(r.store.register_grant(&forged).is_err());
    let checkpoint:Checkpoint=decode("Checkpoint",json!({"format":"pi-0.85.1/1","profile":"caretaker","operator":"proofreading@1","provider":"openai","model":"test-model","messages":[],"pending_operations":[],"event_cursor":"0"})).unwrap();
    r.store.checkpoint("run-one", &checkpoint).unwrap();
    assert_eq!(
        r.store.load_checkpoint("run-one").unwrap(),
        Some(checkpoint.clone())
    );
    let mut changed = checkpoint;
    changed.operator = "excision-repair@2".into();
    assert!(r.store.checkpoint("run-one", &changed).is_err());
}

#[test]
fn work_converges_on_subject_and_stops_repeated_self_triggering() {
    let (_d, r, g) = fixture("observe");
    let request:WorkRequest=decode("WorkRequest",json!({"subject":"report","profile":"curator","operator":"extraction@1","reason":"inspect useful fragment","evidence_refs":[]})).unwrap();
    let work = r.store.request_work("run-one", &request).unwrap();
    assert_eq!(work, r.store.request_work("run-one", &request).unwrap());
    let claimed = r
        .store
        .claim_work(&g.id, "worker-one", 10000)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, work.id);
    assert!(
        r.store
            .claim_work(&g.id, "worker-two", 10000)
            .unwrap()
            .is_none()
    );
    assert!(
        r.store
            .finish_work(&work.id, "worker-two", WorkItemStatus::Completed)
            .is_err()
    );
    r.store
        .finish_work(&work.id, "worker-one", WorkItemStatus::Completed)
        .unwrap();
    assert!(r.store.request_work("run-one", &request).is_err());
}

#[test]
fn model_permits_reserve_root_budget_and_usage_is_idempotent() {
    let (_d, r, _) = fixture("observe");
    let request: PermitRequest = decode(
        "PermitRequest",
        json!({"max_output_tokens":100,"input_tokens_bound":"100","cost_microusd_bound":"1000"}),
    )
    .unwrap();
    let permit = r.store.permit("run-one", &request).unwrap();
    let usage = Usage {
        permit_id: permit.id,
        input_tokens: "20".into(),
        output_tokens: "30".into(),
        cost_microusd: "100".into(),
        complete: true,
    };
    r.store.usage("run-one", &usage).unwrap();
    r.store.usage("run-one", &usage).unwrap();
    r.store.permit("run-one", &request).unwrap();
    r.store.permit("run-one", &request).unwrap();
    assert!(r.store.permit("run-one", &request).is_err());
    let mut forged = usage;
    forged.output_tokens = "0".into();
    assert!(r.store.usage("run-one", &forged).is_err());
}

#[test]
fn wire_contracts_reject_null_unknown_fields_and_numeric_counters() {
    assert!(
        decode::<ArtifactRead>(
            "ArtifactRead",
            json!({"path":"report.txt","offset":0,"length":10,"branch_id":null})
        )
        .is_err()
    );
    assert!(
        decode::<EvidenceRequest>(
            "EvidenceRequest",
            json!({"cursor":9007199254740992_f64,"limit":10})
        )
        .is_err()
    );
    assert!(
        decode::<ArtifactRead>(
            "ArtifactRead",
            json!({"path":"report.txt","offset":0,"length":10,"scope":{"client":"other"}})
        )
        .is_err()
    );
}

#[test]
fn database_version_mismatch_is_rejected_without_overwriting_the_version() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("future.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=99").unwrap();
    assert!(Store::open(&path).is_err());
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 99);
}

#[test]
fn retired_sources_invalidate_derived_records_and_cross_scope_references_are_denied() {
    let (_d, r, g) = fixture("observe");
    let source = r.store.submit(&g, &memory(), false).unwrap();
    let mut derived = memory();
    derived.provenance.source_refs = vec![source.id.clone()];
    let derived = r.store.submit(&g, &derived, false).unwrap();
    let mut other = g.clone();
    other.scope.client = "unrelated-client".into();
    let mut forged = memory();
    forged
        .body
        .insert("evidence_refs".into(), json!([source.id]));
    assert!(r.store.submit(&other, &forged, false).is_err());
    r.store
        .retire(
            &g,
            &RetireRequest {
                id: source.id,
                expected_version: source.version,
                delete: true,
            },
        )
        .unwrap();
    assert!(r.store.record(&g, &derived.id).is_err());
    assert!(
        r.store
            .search(&g, &search("Endpoint"))
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn subscriptions_batch_evidence_without_repeated_dispatch() {
    let (_d, r, g) = fixture("observe");
    r.store
        .subscribe(&ribosome_core::subscriptions::Subscription {
            id: "sub".into(),
            grant_id: g.id.clone(),
            kinds: vec!["handoff".into()],
            profile: Profile::Caretaker,
            operator: "proofreading@1".into(),
            subject: "report".into(),
            batch_size: 2,
            max_delay_ms: 10000,
        })
        .unwrap();
    for index in 1..=2 {
        let event:Event=decode("Event",json!({"id":format!("batch-{index}"),"scope":g.scope,"run_id":"host","producer":"planner","sequence":index.to_string(),"kind":"handoff","timestamp_ms":"1","parents":[],"correlation":"batch","artifacts":[],"payload":{},"provenance":provenance()})).unwrap();
        r.store.ingest(&event).unwrap();
        if index == 1 {
            assert!(r.store.poll_subscription("sub").unwrap().is_none());
        }
    }
    let work = r.store.poll_subscription("sub").unwrap().unwrap();
    assert_eq!(work.evidence_refs.len(), 2);
    assert!(r.store.poll_subscription("sub").unwrap().is_none());
}

#[test]
fn unknown_tool_and_forged_grant_requests_do_not_reach_infrastructure() {
    let (_d, r, _) = fixture("apply");
    assert!(
        r.tool_call(
            "run-one",
            "database.execute",
            json!({"sql":"DELETE FROM grants"})
        )
        .is_err()
    );
    assert!(
        r.tool_call(
            "run-one",
            "artifact.read",
            json!({"path":"report.txt","offset":0,"length":20,"grant":"unrestricted"})
        )
        .is_err()
    );
    let result = r
        .execute(
            "run-one",
            action(json!({"operation_id":"unavailable","kind":"check","tool":"arbitrary-shell"})),
        )
        .unwrap();
    assert_eq!(result.status, EffectStatus::Denied);
}

#[test]
fn messages_have_scoped_delivery_order_acknowledgements_and_bounded_redelivery() {
    let (_d, r, g) = fixture("observe");
    r.store
        .begin_run("run-two", &g.id, &request("run-two"))
        .unwrap();
    let request = MessageSend {
        recipient: "run-two".into(),
        topic: "handoff".into(),
        body: "Check current report version".into(),
        correlation: "report".into(),
    };
    let message = r.store.send_message("run-one", &request).unwrap();
    assert!(r.store.inbox("run-one").unwrap().messages.is_empty());
    for attempt in 1..=3 {
        let inbox = r.store.inbox("run-two").unwrap();
        assert_eq!(inbox.messages[0].id, message.id);
        assert_eq!(inbox.messages[0].attempts, attempt);
    }
    assert!(r.store.inbox("run-two").unwrap().messages.is_empty());
    r.store.acknowledge_message("run-two", &message.id).unwrap();
    r.store.acknowledge_message("run-two", &message.id).unwrap();
    assert!(r.store.acknowledge_message("run-one", &message.id).is_err());
}

#[test]
fn training_export_preserves_synthetic_origin_and_rejects_relabelling() {
    let (_d, r, g) = fixture("observe");
    let mut export_grant = g.clone();
    export_grant.id = "export-grant".into();
    export_grant.allow_export = true;
    r.store.register_grant(&export_grant).unwrap();
    r.store
        .begin_run("export-run", &export_grant.id, &request("export-run"))
        .unwrap();
    let mut record = memory();
    record.provenance.origin = Origin::Synthetic;
    let record = r.store.submit(&export_grant, &record, false).unwrap();
    let output = r
        .export_training(
            "export-run",
            &ExportRequest {
                record_ids: vec![record.id.clone()],
                product: Origin::Synthetic,
            },
        )
        .unwrap();
    let bytes = std::fs::read_to_string(output.path).unwrap();
    let exported: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(exported["product"], "synthetic");
    assert_eq!(exported["record"]["id"], record.id);
    assert!(
        r.export_training(
            "export-run",
            &ExportRequest {
                record_ids: vec![record.id],
                product: Origin::Observed
            }
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn registered_process_timeout_reaps_descendants_and_bounds_pipe_output() {
    let directory = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let result = ribosome_core::process::execute(
        std::path::Path::new("/bin/sh"),
        &["-c".into(), "sleep 30 & wait".into()],
        directory.path(),
        None,
        &BTreeMap::new(),
        std::time::Duration::from_millis(40),
        None,
    )
    .unwrap();
    assert!(result.timed_out);
    assert!(!result.success);
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn orphaned_host_run_is_interrupted_and_can_resume_under_its_original_grant() {
    let (d, r, g) = fixture("apply");
    drop(r);
    let store = Store::open(d.path().join("state.db")).unwrap();
    let host = LocalHost::new(d.path(), BTreeMap::new()).unwrap();
    let r = Runtime::new(store, Box::new(host), d.path().join("state")).unwrap();
    assert_eq!(
        r.store.inspect_run("run-one").unwrap()["status"],
        "interrupted"
    );
    r.store
        .begin_run("run-one", &g.id, &request("run-one"))
        .unwrap();
    assert_eq!(r.store.inspect_run("run-one").unwrap()["status"], "running");
}

#[test]
fn cancellation_rejects_queued_effects_before_dispatch() {
    let (d, r, g) = fixture("apply");
    let version = r.host.version(&g, "report.txt", None).unwrap();
    r.cancellation("run-one")
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let receipt=r.execute("run-one",action(json!({"operation_id":"cancelled-edit","kind":"edit","path":"report.txt","expected_version":version.version,"content":"must not be applied"}))).unwrap();
    assert_eq!(receipt.status, EffectStatus::Denied);
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "original"
    );
}

#[cfg(unix)]
#[test]
fn running_commands_honor_cancellation_before_their_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = cancelled.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(40));
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let start = std::time::Instant::now();
    let result = ribosome_core::process::execute(
        std::path::Path::new("/bin/sh"),
        &["-c".into(), "sleep 30 & wait".into()],
        directory.path(),
        None,
        &BTreeMap::new(),
        std::time::Duration::from_secs(30),
        Some(&cancelled),
    )
    .unwrap();
    trigger.join().unwrap();
    assert!(result.cancelled);
    assert!(!result.timed_out);
    assert!(!result.success);
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn checked_application_enforces_host_checks_and_preserves_live_dependency_versions() {
    let mut tools = BTreeMap::new();
    tools.insert(
        "report-check".into(),
        ribosome_core::host::RegisteredTool {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "test \"$(cat report.txt)\" = corrected".into()],
            timeout_ms: 1000,
            reads: vec!["report.txt".into(), "source.txt".into()],
            validates: vec!["report.txt".into()],
            writes: vec![],
        },
    );
    let (d, r, g) = fixture_with_tools("apply", tools);
    let original = r.host.version(&g, "report.txt", None).unwrap();
    let denied=r.execute("run-one",action(json!({"operation_id":"direct","kind":"edit","path":"report.txt","expected_version":original.version,"content":"corrected"}))).unwrap();
    assert_eq!(denied.status, EffectStatus::Denied);
    let branch = r
        .execute(
            "run-one",
            action(json!({"operation_id":"branch","kind":"branch"})),
        )
        .unwrap();
    assert_eq!(branch.status, EffectStatus::Succeeded);
    let source = r.host.version(&g, "source.txt", None).unwrap();
    let denied = r.execute("run-one", action(json!({"operation_id":"read-only-source","kind":"edit","branch_id":branch.output,"path":"source.txt","expected_version":source.version,"content":"changed input"}))).unwrap();
    assert_eq!(denied.status, EffectStatus::Denied);
    let edit=r.execute("run-one",action(json!({"operation_id":"branch-edit","kind":"edit","branch_id":branch.output,"path":"report.txt","expected_version":original.version,"content":"corrected"}))).unwrap();
    assert_eq!(edit.status, EffectStatus::Succeeded);
    let finding=r.store.submit(&g,&decode("RecordSubmission",json!({"kind":"finding","provenance":provenance(),"body":{"subject":"report","observation":"report requires correction","interpretation":"repair the report","evidence_refs":[],"uncertainty":[],"operator":"excision-repair@1"}})).unwrap(),false).unwrap();
    let path = r.branch_path(&g, Some(&branch.output)).unwrap().unwrap();
    let branch_report = r.host.version(&g, "report.txt", Some(&path)).unwrap();
    // The model omitted source.txt. The host checker reads it, so its actual
    // input version must still participate in application compatibility.
    let intervention=r.store.submit(&g,&decode("RecordSubmission",json!({"kind":"intervention","provenance":provenance(),"body":{"kind":"repair","subject":"report","finding_ref":finding.id,"read_versions":[branch_report],"preserve":["source.txt"],"replace":["report.txt"],"invalidate":[],"recompute":[],"required_checks":[],"bindings":{},"requested_effects":["edit report"],"assumptions":[],"fallback":"abstain","operator":"excision-repair@1"}})).unwrap(),false).unwrap();
    let apply = action(
        json!({"operation_id":"apply","kind":"apply","branch_id":branch.output,"path":"report.txt","expected_version":original.version,"content":"corrected","intervention_ref":intervention.id}),
    );
    std::fs::write(d.path().join("source.txt"), "concurrent source revision").unwrap();
    assert_eq!(
        r.execute("run-one", apply.clone()).unwrap().status,
        EffectStatus::Stale
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "original"
    );
    std::fs::write(d.path().join("source.txt"), "source").unwrap();
    std::fs::write(path.join("source.txt"), "changed only in the branch").unwrap();
    let mut incompatible_branch = apply.clone();
    incompatible_branch.operation_id = "apply-with-different-branch-input".into();
    assert_eq!(
        r.execute("run-one", incompatible_branch).unwrap().status,
        EffectStatus::Stale
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "original"
    );
    std::fs::write(path.join("source.txt"), "source").unwrap();
    let mut retry = apply;
    retry.operation_id = "apply-after-reinspection".into();
    let receipt = r.execute("run-one", retry).unwrap();
    assert_eq!(receipt.status, EffectStatus::Succeeded);
    assert_eq!(
        r.lookup("run-one", "apply-after-reinspection/check/0")
            .unwrap()
            .status,
        EffectStatus::Succeeded
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.txt")).unwrap(),
        "corrected"
    );
}

#[test]
fn dependency_graph_revisions_replace_current_versions_and_capacity_is_explicit() {
    let (_d, r, g) = fixture("observe");
    let dependency = |version: &str, dependent: &str, references: Vec<String>| {
        decode::<Dependency>("Dependency", json!({"source":{"path":"source.txt","version":version},"dependent":{"path":dependent,"version":"v1"},"basis":"host","evidence_refs":references})).unwrap()
    };
    r.store
        .add_dependency(&g.scope, &dependency("v1", "report.txt", vec![]))
        .unwrap();
    r.store
        .add_dependency(&g.scope, &dependency("v2", "report.txt", vec![]))
        .unwrap();
    assert_eq!(r.store.dependencies(&g.scope).unwrap().len(), 1);
    assert_eq!(
        r.store.dependencies(&g.scope).unwrap()[0].source.version,
        "v2"
    );
    let references = vec!["x".repeat(200); 1000];
    r.store
        .add_dependency(
            &g.scope,
            &dependency("v2", "report.txt", references.clone()),
        )
        .unwrap();
    let error = r
        .store
        .add_dependency(&g.scope, &dependency("v2", "unsafe.txt", references))
        .unwrap_err();
    assert_eq!(error.code, -32005);
    let page = r
        .store
        .evidence(
            &g,
            &decode("EvidenceRequest", json!({"cursor":"0","limit":20})).unwrap(),
        )
        .unwrap();
    assert_eq!(page.dependencies.len(), 1);
    assert!(serde_json::to_vec(&page).unwrap().len() < ribosome_core::validation::MAX_FRAME);
}

#[test]
fn agent_activity_is_scoped_idempotent_attributed_and_kept_out_of_its_own_investigation() {
    let (_d, r, g) = fixture("observe");
    let activity = |sequence: &str, text: &str| json!({"sequence":sequence,"role":"assistant","timestamp_ms":"1","source_refs":[],"content":{"parts":[{"type":"text","text":text}]}});
    let batch = json!({"entries":[activity("1", "I think this passed"), activity("2", "This remains an interpretation")]});
    r.tool_call("run-one", "session.events", batch.clone())
        .unwrap();
    let page = |grant: &Grant| {
        r.store
            .evidence(
                grant,
                &decode("EvidenceRequest", json!({"cursor":"0","limit":20})).unwrap(),
            )
            .unwrap()
    };
    let first = page(&g);
    r.tool_call("run-one", "session.events", batch).unwrap();
    assert_eq!(page(&g), first);
    assert_eq!(first.events.len(), 2);
    assert!(first.events.iter().all(|e| {
        e.kind == "agent_message"
            && e.provenance.origin == Origin::Observed
            && e.payload["authority"]
                .as_str()
                .unwrap()
                .contains("role-authored")
    }));
    assert_eq!(
        r.tool_call("run-one", "evidence.read", json!({"cursor":"0","limit":20}))
            .unwrap()["events"],
        json!([])
    );
    r.store
        .begin_run("later-run", &g.id, &request("later-run"))
        .unwrap();
    assert_eq!(
        r.tool_call(
            "later-run",
            "evidence.read",
            json!({"cursor":"0","limit":20})
        )
        .unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let conflict =
        json!({"entries":[activity("3", "must roll back"),activity("1", "changed statement")]});
    assert!(r.tool_call("run-one", "session.events", conflict).is_err());
    assert_eq!(page(&g), first);
    let mut other = g.clone();
    other.scope.client = "unrelated".into();
    assert!(page(&other).events.is_empty());
    let mut protected = g.clone();
    protected.id = "protected-activity".into();
    protected.visible_splits.push(Split::Holdout);
    r.store.register_grant(&protected).unwrap();
    r.store
        .begin_run("protected-run", &protected.id, &request("protected-run"))
        .unwrap();
    r.tool_call(
        "protected-run",
        "session.events",
        json!({"entries":[activity("1", "protected observation in context")]}),
    )
    .unwrap();
    assert_eq!(page(&g), first);
    assert_eq!(
        page(&protected).events.last().unwrap().provenance.split,
        Split::Holdout
    );
}

#[test]
fn work_redelivery_exhaustion_does_not_hide_following_queued_work() {
    let (_d, r, g) = fixture("observe");
    let request = |subject: &str| {
        decode("WorkRequest", json!({"subject":subject,"profile":"caretaker","operator":"proofreading@1","reason":"inspect","evidence_refs":[]})).unwrap()
    };
    let first = r.store.request_work("run-one", &request("first")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = r.store.request_work("run-one", &request("second")).unwrap();
    for attempt in 1..=3 {
        let claimed = r.store.claim_work(&g.id, "worker", 1).unwrap().unwrap();
        assert_eq!(claimed.id, first.id);
        assert_eq!(claimed.attempts, attempt);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let next = r
        .store
        .claim_work(&g.id, "worker", 1000)
        .unwrap()
        .expect("exhausted first item must not stop delivery of queued work");
    assert_eq!(next.id, second.id);
}

#[test]
fn late_work_completion_is_recorded_unless_another_owner_has_reclaimed_it() {
    let (_d, r, g) = fixture("observe");
    let request = |subject: &str| {
        decode("WorkRequest", json!({"subject":subject,"profile":"caretaker","operator":"proofreading@1","reason":"inspect","evidence_refs":[]})).unwrap()
    };
    let first = r.store.request_work("run-one", &request("first")).unwrap();
    r.store.claim_work(&g.id, "owner-one", 1).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    r.store
        .finish_work(&first.id, "owner-one", WorkItemStatus::Exhausted)
        .expect("deadline outcomes must settle after the work lease expires");
    let second = r.store.request_work("run-one", &request("second")).unwrap();
    r.store.claim_work(&g.id, "owner-one", 1).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    r.store
        .claim_work(&g.id, "owner-two", 1000)
        .unwrap()
        .unwrap();
    assert!(
        r.store
            .finish_work(&second.id, "owner-one", WorkItemStatus::Completed)
            .is_err()
    );
    r.store
        .finish_work(&second.id, "owner-two", WorkItemStatus::Completed)
        .unwrap();
}

#[test]
fn usage_can_settle_after_deadline_without_permitting_more_work() {
    let (_d, r, mut g) = fixture("observe");
    g.id = "settlement-grant".into();
    g.budget.deadline_ms = (now_ms() + 250).to_string();
    r.store.register_grant(&g).unwrap();
    r.store
        .begin_run("settlement", &g.id, &request("settlement"))
        .unwrap();
    let permit = r
        .tool_call(
            "settlement",
            "model.permit",
            json!({"max_output_tokens":10,"input_tokens_bound":"10","cost_microusd_bound":"1000"}),
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(
        g.budget
            .deadline_ms
            .parse::<u64>()
            .unwrap()
            .saturating_sub(now_ms())
            + 1,
    ));
    r.store
        .finish_run(
            "settlement",
            &AgentResult {
                disposition: Disposition::Exhausted,
                summary: "Deadline reached".into(),
            },
        )
        .unwrap();
    r.tool_call("settlement", "model.usage", json!({"permit_id":permit["id"],"input_tokens":"4","output_tokens":"2","cost_microusd":"10","complete":true})).unwrap();
    assert!(
        r.tool_call(
            "settlement",
            "model.permit",
            json!({"max_output_tokens":1,"input_tokens_bound":"1","cost_microusd_bound":"1"})
        )
        .is_err()
    );
    assert_eq!(
        r.store.inspect_run("settlement").unwrap()["model_usage"]["observed_cost_microusd"],
        "10"
    );
    assert_eq!(
        r.store.inspect_run("settlement").unwrap()["status"],
        "exhausted"
    );
}

#[test]
fn retired_memory_cannot_reappear_through_agent_activity_or_derived_records() {
    for delete in [false, true] {
        let (_d, r, g) = fixture("observe");
        let source = r.store.submit(&g, &memory(), false).unwrap();
        r.tool_call("run-one", "session.events", json!({"entries":[{"sequence":"1","role":"assistant","timestamp_ms":"1","source_refs":[source.id],"content":{"parts":[{"type":"text","text":"Copied content from the memory"}]}}]})).unwrap();
        let view = || {
            r.store
                .evidence(
                    &g,
                    &decode("EvidenceRequest", json!({"cursor":"0","limit":20})).unwrap(),
                )
                .unwrap()
        };
        let activity = view().events[0].clone();
        assert_eq!(activity.provenance.source_refs, vec![source.id.clone()]);
        let mut summary = memory();
        summary.provenance.source_refs = vec![activity.id.clone()];
        let derived = r.store.submit(&g, &summary, false).unwrap();
        r.store
            .retire(
                &g,
                &RetireRequest {
                    id: source.id,
                    expected_version: "1".into(),
                    delete,
                },
            )
            .unwrap();
        assert!(view().events.is_empty());
        assert!(r.store.require_reference(&g, &activity.id).is_err());
        assert!(r.store.record(&g, &derived.id).is_err());
        assert!(
            r.store
                .search(&g, &search("Endpoint"))
                .unwrap()
                .records
                .is_empty()
        );
    }
}

#[test]
fn completed_effect_returns_a_scoped_evidence_reference() {
    let (_d, r, g) = fixture("apply");
    let operation = "o".repeat(200);
    let receipt = r
        .execute(
            "run-one",
            action(json!({"kind":"branch", "operation_id":operation})),
        )
        .unwrap();
    let encoded = serde_json::to_value(&receipt).unwrap();
    let reference = encoded["evidence_ref"]
        .as_str()
        .expect("receipt must expose its host event ID");
    r.store.require_reference(&g, reference).unwrap();
    let mut other = g.clone();
    other.scope.client = "unrelated".into();
    assert!(r.store.require_reference(&other, reference).is_err());
    let mut finding = memory();
    finding.provenance.source_refs.push(reference.into());
    r.store.submit(&g, &finding, false).unwrap();
    assert_eq!(receipt, r.lookup("run-one", &operation).unwrap());
}

#[test]
fn derived_records_and_receipts_cannot_downgrade_protected_evidence() {
    let (_d, r, g) = fixture("apply");
    let mut protected = g.clone();
    protected.id = "protected-grant".into();
    protected.visible_splits.push(Split::Holdout);
    r.store.register_grant(&protected).unwrap();
    r.store
        .begin_run("protected-run", &protected.id, &request("protected-run"))
        .unwrap();
    let mut secret = memory();
    secret.provenance.split = Split::Holdout;
    let source = r.store.submit(&protected, &secret, false).unwrap();
    let unreferenced_copy = serde_json::to_value(memory()).unwrap();
    assert!(
        r.tool_call("protected-run", "record.submit", unreferenced_copy)
            .is_err(),
        "omitting references must not let a worker declassify its output"
    );
    for provenance_reference in [true, false] {
        let mut derived = memory();
        if provenance_reference {
            derived.provenance.source_refs.push(source.id.clone());
        } else {
            derived
                .body
                .insert("evidence_refs".into(), json!([source.id]));
        }
        assert!(
            r.store.submit(&protected, &derived, false).is_err(),
            "an agent must not relabel protected lineage as development"
        );
        derived.provenance.split = Split::Holdout;
        let saved = r.store.submit(&protected, &derived, false).unwrap();
        assert!(r.store.record(&g, &saved.id).is_err());
    }
    let receipt = r
        .execute(
            "protected-run",
            action(json!({"kind":"branch","operation_id":"protected-branch"})),
        )
        .unwrap();
    let reference = receipt.evidence_ref.unwrap();
    assert!(
        r.store.require_reference(&g, &reference).is_err(),
        "host receipts must retain the run's evidence restriction"
    );
    r.store.require_reference(&protected, &reference).unwrap();
}

#[test]
fn unknown_source_run_is_distinct_from_an_exhausted_evidence_page() {
    let (_d, r, g) = fixture("observe");
    let event: Event = decode("Event", json!({"id":"source-event","scope":g.scope,"run_id":"source-workflow","producer":"worker","sequence":"1","kind":"progress","timestamp_ms":"1","parents":[],"correlation":"task","artifacts":[],"payload":{},"provenance":provenance()})).unwrap();
    r.store.ingest(&event).unwrap();
    let error = r
        .tool_call(
            "run-one",
            "evidence.read",
            json!({"cursor":"0","limit":20,"run_id":"run-one"}),
        )
        .unwrap_err();
    assert!(error.message.contains("omit run_id"));
    let page = r
        .tool_call(
            "run-one",
            "evidence.read",
            json!({"cursor":"0","limit":20,"run_id":"source-workflow"}),
        )
        .unwrap();
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    let end = r
        .tool_call(
            "run-one",
            "evidence.read",
            json!({"cursor":page["cursor"],"limit":20,"run_id":"source-workflow"}),
        )
        .unwrap();
    assert!(end["events"].as_array().unwrap().is_empty());
}
