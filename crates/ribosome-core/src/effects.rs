use crate::{
    contracts::*,
    effect_finalization::{CreatedBranch, EffectObservation},
    error::{Error, Result},
    host::{HostAdapter, hash},
    store::Store,
    validation::{counter, id, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::path::{Path, PathBuf};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub struct Runtime {
    pub store: Store,
    pub host: Arc<dyn HostAdapter>,
    pub state_dir: PathBuf,
    pub laboratory: crate::experiments::Laboratory,
    cancellations: Arc<Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>>,
}

impl Runtime {
    pub fn new(
        mut store: Store,
        host: Box<dyn HostAdapter>,
        state_dir: impl AsRef<Path>,
    ) -> Result<Self> {
        std::fs::create_dir_all(state_dir.as_ref().join("branches"))?;
        std::fs::create_dir_all(state_dir.as_ref().join("exports"))?;
        let state_dir = state_dir.as_ref().canonicalize()?;
        let exports = state_dir.join("exports");
        if exports.canonicalize()? != exports {
            return Err(Error::denied(
                "managed exports directory must not be a symlink",
            ));
        }
        store.export_directory = Some(exports);
        store.recover_export_files()?;
        // Construction follows host ownership acquisition. Running rows left
        // by a previous host process are interrupted, never silently completed.
        store.db.execute("UPDATE work SET status='interrupted',body=json_set(body,'$.status','interrupted') WHERE status='running'",[])?;
        store.db.execute(
            "UPDATE runs SET status='interrupted' WHERE status IN ('running','waiting')",
            [],
        )?;
        store.recover_experiment_files()?;
        Ok(Self {
            store,
            host: host.into(),
            state_dir,
            laboratory: crate::experiments::Laboratory::default(),
            cancellations: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }

    pub(crate) fn service_connection(&self) -> Result<Self> {
        Ok(Self {
            store: self.store.service_connection()?,
            host: self.host.clone(),
            state_dir: self.state_dir.clone(),
            laboratory: self.laboratory.clone(),
            cancellations: self.cancellations.clone(),
        })
    }

    /// An evaluation owns a fresh workspace, but shares the existing host
    /// lifecycle and ledger. Construction must not recover active parent runs.
    pub(crate) fn evaluation_runtime(
        &self,
        host: Box<dyn HostAdapter>,
        state: &Path,
    ) -> Result<Self> {
        std::fs::create_dir_all(state.join("branches"))?;
        std::fs::create_dir_all(state.join("exports"))?;
        let mut runtime = self.service_connection()?;
        runtime.host = host.into();
        runtime.state_dir = state.canonicalize()?;
        runtime.store.export_directory = Some(runtime.state_dir.join("exports"));
        runtime.laboratory = crate::experiments::Laboratory::default();
        Ok(runtime)
    }

    pub fn cancellation(&self, run_id: &str) -> Arc<AtomicBool> {
        self.cancellations
            .lock()
            .expect("cancellation lock")
            .entry(run_id.to_owned())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    pub fn branch_path(&self, grant: &Grant, branch_id: Option<&str>) -> Result<Option<PathBuf>> {
        branch_id
            .map(|id| {
                self.store
                    .db
                    .query_row(
                        "SELECT path FROM branches WHERE id=?1 AND grant_id=?2",
                        params![id, grant.id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                    .map(PathBuf::from)
                    .ok_or_else(|| Error::missing("branch not found in grant"))
            })
            .transpose()
    }

    pub fn lookup(&self, run_id: &str, operation: &str) -> Result<ActionReceipt> {
        let grant = self.store.run_grant(run_id)?;
        let receipt = self.lookup_for_recovery(run_id, operation)?;
        self.refresh_artifact_sources(&grant)?;
        self.store.receipt_for_delivery(&grant, receipt)
    }

    pub(crate) fn lookup_for_recovery(
        &self,
        run_id: &str,
        operation: &str,
    ) -> Result<ActionReceipt> {
        let grant = self.store.run_grant(run_id)?;
        let (body, phase, settlement): (String, String, Option<String>) = self
            .store
            .db
            .query_row(
                "SELECT body,phase,settlement FROM effects WHERE id=?1 AND grant_id=?2",
                params![operation, grant.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| Error::missing("operation not found"))?;
        let mut receipt: ActionReceipt = serde_json::from_str(&body)?;
        receipt.settlement = settlement
            .map(|value| serde_json::from_str(&value))
            .transpose()?;
        match phase.as_str() {
            "finalized" => Ok(receipt),
            "observed" => self.finalize_effect(operation),
            _ => self.reconcile(&grant, receipt),
        }
    }

    fn reconcile(&self, grant: &Grant, mut receipt: ActionReceipt) -> Result<ActionReceipt> {
        receipt.reconciled = true;
        receipt.finished_ms = Some(now_ms().to_string());
        receipt.status = EffectStatus::Unknown;
        receipt.outcome_basis = Some(EffectOutcomeBasis::Unresolved);
        receipt.output = "Host has no captured adapter outcome. No retry was dispatched; owner reconciliation remains required.".into();
        let (writes, generation): (String, i64) = self.store.db.query_row(
            "SELECT potential_writes,validation_generation FROM effects WHERE id=?1",
            [&receipt.operation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut writes: Vec<String> = serde_json::from_str(&writes)?;
        if writes.is_empty()
            && receipt.action.kind == ActionKind::Execute
            && receipt.action.branch_id.is_none()
        {
            writes = grant
                .writable_paths
                .clone()
                .unwrap_or_else(|| grant.paths.clone());
        }
        if matches!(receipt.action.kind, ActionKind::Edit | ActionKind::Apply) {
            let workspace = self.branch_path(
                grant,
                if receipt.action.kind == ActionKind::Edit {
                    receipt.action.branch_id.as_deref()
                } else {
                    None
                },
            );
            if let (Ok(workspace), Some(path), Some(content)) =
                (workspace, &receipt.action.path, &receipt.action.content)
            {
                if workspace.is_none() {
                    writes.push(path.clone());
                }
                if let Ok(current) = self.host.version(grant, path, workspace.as_deref())
                    && current.version == hash(content.as_bytes())
                {
                    receipt.outcome_basis = Some(EffectOutcomeBasis::CurrentPostconditionObserved);
                    receipt.after = vec![current];
                    receipt.output = "Desired current content observed. This does not establish dispatch, execution, or required checks. Owner reconciliation and fresh validation remain required.".into();
                }
            }
        }
        let mut observation = EffectObservation::new(receipt);
        observation.writes = writes;
        observation.validation_generation = generation;
        self.capture_effect(&observation)?;
        self.finalize_effect(&observation.receipt.operation_id)
    }

    pub fn reconcile_run(&self, run_id: &str) -> Result<Vec<ActionReceipt>> {
        let mut stmt = self
            .store
            .db
            .prepare("SELECT id FROM effects WHERE run_id=?1")?;
        let ids = stmt
            .query_map([run_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| self.lookup(run_id, id)).collect()
    }

    pub fn execute(&self, run_id: &str, action: Action) -> Result<ActionReceipt> {
        validate("Action", &serde_json::to_value(&action)?)?;
        let grant = self.store.run_grant(run_id)?;
        let existing: Option<(String, String)> = self
            .store
            .db
            .query_row(
                "SELECT grant_id,body FROM effects WHERE id=?1",
                [&action.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((owner, body)) = existing {
            let previous: ActionReceipt = serde_json::from_str(&body)?;
            if owner != grant.id || previous.action != action {
                return Err(Error::conflict(
                    "operation ID reused with different grant or arguments",
                ));
            }
            return self.lookup(run_id, &action.operation_id);
        }
        let source_ref = id();
        let receipt = ActionReceipt {
            operation_id: action.operation_id.clone(),
            run_id: run_id.into(),
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
            evidence_ref: Some(source_ref.clone()),
            content_available: Some(true),
            outcome_basis: None,
            validations: None,
            restored_validity: None,
            restored_properties: None,
            settlement: None,
        };
        // Commit the intent before the adapter is allowed to execute an effect.
        let tx = self.store.write_transaction()?;
        let allocation = self.store.run_allocation_in(run_id)?;
        self.store.db.execute(
            "INSERT INTO effects(id,run_id,grant_id,body,authority,allocation_id) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                receipt.operation_id,
                run_id,
                grant.id,
                serde_json::to_string(&receipt)?,
                serde_json::to_string(&grant)?,
                allocation.id
            ],
        )?;
        let sequence: i64 = tx.query_row(
            "SELECT rowid FROM effects WHERE id=?1",
            [&receipt.operation_id],
            |row| row.get(0),
        )?;
        let mut references = self.store.run_context_sources(run_id)?;
        references.extend(receipt.action.intervention_ref.iter().cloned());
        references.extend(
            receipt
                .action
                .implementation
                .iter()
                .map(|implementation| implementation.id.clone()),
        );
        self.store.ingest(&Event {
            id: source_ref,
            scope: grant.scope.clone(),
            run_id: run_id.into(),
            producer: "effect-context".into(),
            sequence: sequence.to_string(),
            kind: "effect_context".into(),
            timestamp_ms: receipt.started_ms.clone(),
            parents: vec![],
            correlation: receipt.operation_id.clone(),
            artifacts: vec![],
            payload: serde_json::json!({"operation_id":receipt.operation_id})
                .as_object()
                .unwrap()
                .clone(),
            provenance: Provenance {
                origin: Origin::Observed,
                source_refs: references,
                scenario_family: "host-effects".into(),
                split: crate::validation::derived_split(&grant.visible_splits),
                limitations: vec![],
            },
        })?;
        tx.commit()?;
        let mut observation = EffectObservation::new(receipt);
        let result = self.dispatch(run_id, &grant, &mut observation);
        if let Err(error) = result {
            let phase: String = self.store.db.query_row(
                "SELECT phase FROM effects WHERE id=?1",
                [&observation.receipt.operation_id],
                |row| row.get(0),
            )?;
            let no_effect = phase == "prepared";
            observation.receipt.status = if no_effect {
                match error.code {
                    -32001 => EffectStatus::Denied,
                    -32002 => EffectStatus::Stale,
                    _ => EffectStatus::Failed,
                }
            } else {
                EffectStatus::Unknown
            };
            observation.receipt.outcome_basis = Some(if no_effect {
                EffectOutcomeBasis::NotDispatched
            } else {
                EffectOutcomeBasis::Unresolved
            });
            if no_effect {
                observation.writes.clear();
            }
            observation.receipt.output = error.message;
        }
        observation.receipt.finished_ms = Some(now_ms().to_string());
        observation.receipt.elapsed_ms = Some(
            now_ms()
                .saturating_sub(counter(&observation.receipt.started_ms)?)
                .to_string(),
        );
        self.capture_effect(&observation)?;
        let receipt = self.finalize_effect(&observation.receipt.operation_id)?;
        self.store.receipt_for_delivery(&grant, receipt)
    }

    fn dispatch(
        &self,
        run_id: &str,
        grant: &Grant,
        observation: &mut EffectObservation,
    ) -> Result<()> {
        self.store.require_active(run_id)?;
        if self.cancellation(run_id).load(Ordering::SeqCst) {
            return Err(Error::denied("run cancelled; effect not dispatched"));
        }
        if grant.mode == Mode::Observe {
            return Err(Error::denied("observe grants cannot dispatch effects"));
        }
        let attempts: u32 = self.store.db.query_row(
            "SELECT count(*) FROM effects WHERE grant_id=?1",
            [&grant.id],
            |r| r.get(0),
        )?;
        if attempts > grant.budget.max_actions {
            return Err(Error::exhausted("root action budget exhausted"));
        }
        let allocation = self.store.run_allocation_in(run_id)?;
        for owner in self.store.allocation_ancestry(grant, &allocation.id)? {
            if now_ms() >= counter(&owner.budget.deadline_ms)?
                || self
                    .store
                    .allocation_ledger_status(&self.store.grant(&owner.grant_id)?, &owner.id)?
                    .usage
                    .actions
                    > owner.budget.max_actions
            {
                return Err(Error::exhausted(
                    "child action budget or deadline exhausted",
                ));
            }
        }
        let action = observation.receipt.action.clone();
        self.store.require_attachment_write(run_id, &action)?;
        let workspace = self.branch_path(grant, action.branch_id.as_deref())?;
        if let Some(intervention) = &action.intervention_ref {
            let record = self.store.record(grant, intervention)?;
            if record.kind != RecordKind::Intervention {
                return Err(Error::invalid("intervention_ref is not an intervention"));
            }
            let intervention: Intervention = serde_json::from_value(Value::Object(record.body))?;
            for reference in intervention.read_versions {
                if self
                    .host
                    .version(grant, &reference.path, workspace.as_deref())?
                    .version
                    != reference.version
                {
                    return Err(Error::conflict("intervention read versions are stale"));
                }
            }
        }
        match action.kind {
            ActionKind::Branch => {
                let branch_id = id();
                let path = self.state_dir.join("branches").join(&branch_id);
                self.start_dispatch(grant, observation, vec![])?;
                let versions = self.host.branch(grant, &path)?;
                observation.branch = Some(CreatedBranch {
                    id: branch_id.clone(),
                    path,
                    versions: versions.clone(),
                });
                observation.receipt.output = branch_id;
                observation.receipt.after = versions;
            }
            ActionKind::Edit => {
                if workspace.is_none()
                    && grant
                        .required_checks
                        .as_ref()
                        .is_some_and(|checks| !checks.is_empty())
                {
                    return Err(Error::denied(
                        "host-required checks require edits in a branch followed by checked application",
                    ));
                }
                let path = action
                    .path
                    .as_deref()
                    .ok_or_else(|| Error::invalid("edit requires path"))?;
                let expected = action
                    .expected_version
                    .as_deref()
                    .ok_or_else(|| Error::invalid("edit requires expected_version"))?;
                let content = action
                    .content
                    .as_deref()
                    .ok_or_else(|| Error::invalid("edit requires content"))?;
                observation.receipt.before =
                    vec![
                        self.host
                            .prepare_edit(grant, path, expected, workspace.as_deref())?,
                    ];
                self.start_dispatch(
                    grant,
                    observation,
                    if workspace.is_none() {
                        vec![path.into()]
                    } else {
                        vec![]
                    },
                )?;
                observation.receipt.after =
                    vec![
                        self.host
                            .edit(grant, path, expected, content, workspace.as_deref())?,
                    ];
                observation.receipt.output =
                    "Artifact written; fresh checks remain required.".into();
            }
            ActionKind::Check | ActionKind::Execute => {
                let mut tool = action
                    .tool
                    .clone()
                    .ok_or_else(|| Error::invalid("check/execute requires a registered tool"))?;
                if action.kind == ActionKind::Execute {
                    let reference = action.implementation.as_ref().ok_or_else(|| {
                        Error::invalid("execute requires an admitted implementation")
                    })?;
                    let record = self.store.record(grant, &reference.id)?;
                    let implementation: Implementation =
                        serde_json::from_value(Value::Object(record.body.clone()))?;
                    if !self.store.is_admitted(grant, &record)?
                        || reference.version != implementation.version
                    {
                        return Err(Error::denied(
                            "implementation is not admitted in this context",
                        ));
                    }
                    if implementation.format != ImplementationFormat::RegisteredTool
                        || implementation.material != tool
                    {
                        return Err(Error::invalid(
                            "instruction implementations must be adapted by the agent; code requires a suitable host adapter",
                        ));
                    }
                    tool = implementation.material;
                }
                let prepared = self.host.prepare_tool(
                    grant,
                    &tool,
                    workspace.as_deref(),
                    action.kind == ActionKind::Check,
                )?;
                self.require_property_bindings(grant, &prepared.properties)?;
                observation.preflight.properties = prepared.properties;
                observation.preflight.checks =
                    vec![(tool.clone(), prepared.checker_version.clone())];
                self.pin_effect_checks(observation)?;
                let potential_writes = if workspace.is_none() {
                    prepared.writes
                } else {
                    vec![]
                };
                self.start_dispatch(grant, observation, potential_writes)?;
                let mut result = self.host.run_tool(
                    grant,
                    &tool,
                    workspace.as_deref(),
                    action.kind == ActionKind::Check,
                    Some(&self.cancellation(run_id)),
                )?;
                if result.checker_version != prepared.checker_version {
                    result.success = false;
                    result.validation_outcome = ValidationEvidenceOutcome::Stale;
                    result.output.push_str(
                        "\nChecker identity changed after preflight; validation is stale.",
                    );
                }
                observation.receipt.validations =
                    Some(vec![crate::effect_validation::check_evidence(
                        grant,
                        &observation.receipt,
                        &result,
                        observation.validation_generation,
                    )?]);
                observation.receipt.before = result.before;
                observation.receipt.after = result.after;
                observation.receipt.output = result.output;
                observation.receipt.outcome_basis = Some(EffectOutcomeBasis::ExecutionEstablished);
                if workspace.is_none() {
                    observation.writes = result.writes;
                }
                if !result.success {
                    observation.receipt.status = EffectStatus::Failed;
                    return Ok(());
                }
            }
            ActionKind::Apply => {
                if grant.mode != Mode::Apply {
                    return Err(Error::denied("live application requires apply mode"));
                }
                let path = action
                    .path
                    .as_deref()
                    .ok_or_else(|| Error::invalid("apply requires path"))?;
                let branch = workspace
                    .as_deref()
                    .ok_or_else(|| Error::invalid("apply requires branch_id"))?;
                let expected = action
                    .expected_version
                    .as_deref()
                    .ok_or_else(|| Error::invalid("apply requires live expected_version"))?;
                let intervention_id = action
                    .intervention_ref
                    .as_ref()
                    .ok_or_else(|| Error::invalid("apply requires intervention_ref"))?;
                let intervention = self.store.record(grant, intervention_id)?;
                observation.preflight.intervention = Some(VersionRef {
                    id: intervention.id.clone(),
                    version: intervention.version.clone(),
                });
                let intervention: Intervention =
                    serde_json::from_value(Value::Object(intervention.body))?;
                let base: String = self.store.db.query_row(
                    "SELECT versions FROM branches WHERE id=?1 AND grant_id=?2",
                    params![action.branch_id, grant.id],
                    |r| r.get(0),
                )?;
                let base: Vec<ArtifactRef> = serde_json::from_str(&base)?;
                for reference in &intervention.read_versions {
                    let original =
                        base.iter()
                            .find(|r| r.path == reference.path)
                            .ok_or_else(|| {
                                Error::conflict(
                                    "intervention references state outside branch snapshot",
                                )
                            })?;
                    if self.host.version(grant, &reference.path, None)?.version != original.version
                    {
                        return Err(Error::conflict(
                            "live dependency changed since branch creation",
                        ));
                    }
                }
                let required_checks = intervention
                    .required_checks
                    .iter()
                    .chain(grant.required_checks.iter().flatten())
                    .collect::<std::collections::BTreeSet<_>>();
                if required_checks.is_empty() {
                    return Err(Error::denied("apply requires named acceptance checks"));
                }
                for name in &required_checks {
                    let prepared = self.host.prepare_tool(grant, name, Some(branch), true)?;
                    self.require_property_bindings(grant, &prepared.properties)?;
                    observation
                        .preflight
                        .checks
                        .push(((*name).clone(), prepared.checker_version));
                    observation.preflight.properties.extend(prepared.properties);
                }
                self.pin_effect_checks(observation)?;
                let mut checked_inputs = vec![];
                let mut check_receipts = vec![];
                observation.receipt.validations = Some(vec![]);
                for (index, name) in required_checks.into_iter().enumerate() {
                    let result = self.execute(
                        run_id,
                        crate::validation::decode(
                            "Action",
                            serde_json::json!({
                                "operation_id": format!("{}/check/{index}", action.operation_id),
                                "kind": "check", "tool": name, "branch_id": action.branch_id
                            }),
                        )?,
                    )?;
                    if result.status != EffectStatus::Succeeded
                        || result.outcome_basis != Some(EffectOutcomeBasis::ExecutionEstablished)
                    {
                        return Err(Error::denied(
                            "application requires established successful acceptance checks",
                        ));
                    }
                    let certificate = result
                        .validations
                        .as_ref()
                        .filter(|checks| checks.len() == 1)
                        .and_then(|checks| checks.first())
                        .ok_or_else(|| {
                            Error::denied("acceptance check lacks captured validation evidence")
                        })?;
                    let planned = observation
                        .preflight
                        .checks
                        .iter()
                        .find(|(check, _)| check == name)
                        .unwrap();
                    if certificate.check_ref != *name
                        || certificate.branch_id != action.branch_id
                        || certificate.outcome != ValidationEvidenceOutcome::Passed
                        || certificate.policy_version
                            != crate::effect_validation::policy_version(grant)?
                        || certificate.checker_version != planned.1
                    {
                        return Err(Error::conflict(
                            "acceptance check evidence no longer matches the pinned checker, branch or policy",
                        ));
                    }
                    checked_inputs.extend(certificate.inputs.clone());
                    observation
                        .receipt
                        .validations
                        .as_mut()
                        .unwrap()
                        .push(certificate.clone());
                    check_receipts.push(result.operation_id);
                }
                let read = self.host.read(
                    grant,
                    &ArtifactRead {
                        snapshot_id: None,
                        required_freshness: None,
                        path: path.into(),
                        offset: 0,
                        length: 65536,
                        branch_id: None,
                    },
                    Some(branch),
                )?;
                if !read.eof {
                    return Err(Error::invalid("apply supports text artifacts up to 64 KiB"));
                }
                if action.content.as_deref() != Some(&read.content) {
                    return Err(Error::conflict(
                        "apply content must match checked branch content",
                    ));
                }
                if !observation
                    .receipt
                    .validations
                    .as_ref()
                    .unwrap()
                    .iter()
                    .flat_map(|check| &check.targets)
                    .any(|target| target == &read.artifact)
                {
                    return Err(Error::denied(
                        "acceptance checks did not explicitly validate the applied artifact",
                    ));
                }
                for (name, version) in &observation.preflight.checks {
                    if self.host.checker_version(name)? != *version {
                        return Err(Error::conflict(
                            "required checker changed during branch validation",
                        ));
                    }
                }
                // Checker inputs are host observations. An incomplete agent
                // proposal cannot remove them from compatibility checks.
                for input in &checked_inputs {
                    if self.host.version(grant, &input.path, Some(branch))?.version != input.version
                    {
                        return Err(Error::conflict(
                            "branch input changed after acceptance checks",
                        ));
                    }
                    if input.path != path
                        && self.host.version(grant, &input.path, None)?.version != input.version
                    {
                        return Err(Error::conflict(
                            "checked dependency differs from the live recipient",
                        ));
                    }
                }
                for reference in &intervention.read_versions {
                    let original = base.iter().find(|r| r.path == reference.path).unwrap();
                    if self.host.version(grant, &reference.path, None)?.version != original.version
                    {
                        return Err(Error::conflict(
                            "live dependency changed during branch checks",
                        ));
                    }
                }
                observation.receipt.before =
                    vec![self.host.prepare_edit(grant, path, expected, None)?];
                self.store.require_attachment_write(run_id, &action)?;
                self.start_dispatch(grant, observation, vec![path.into()])?;
                observation.receipt.after =
                    vec![self.host.edit(grant, path, expected, &read.content, None)?];
                observation.receipt.output = format!(
                    "Checked branch artifact applied. Check receipts: {}",
                    check_receipts.join(", ")
                );
            }
        }
        observation.receipt.outcome_basis = Some(EffectOutcomeBasis::ExecutionEstablished);
        observation.receipt.status = EffectStatus::Succeeded;
        Ok(())
    }
}
