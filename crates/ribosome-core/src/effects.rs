use crate::{
    contracts::*,
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
    pub host: Box<dyn HostAdapter>,
    pub state_dir: PathBuf,
    pub laboratory: crate::experiments::Laboratory,
    cancellations: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
}

impl Runtime {
    pub fn new(
        store: Store,
        host: Box<dyn HostAdapter>,
        state_dir: impl AsRef<Path>,
    ) -> Result<Self> {
        std::fs::create_dir_all(state_dir.as_ref().join("branches"))?;
        std::fs::create_dir_all(state_dir.as_ref().join("exports"))?;
        // Construction follows host ownership acquisition. Running rows left
        // by a previous host process are interrupted, never silently completed.
        store.db.execute("UPDATE work SET status='interrupted',body=json_set(body,'$.status','interrupted') WHERE status='running' AND id IN (SELECT id FROM runs WHERE status='running')",[])?;
        store.db.execute(
            "UPDATE runs SET status='interrupted' WHERE status='running'",
            [],
        )?;
        Ok(Self {
            store,
            host,
            state_dir: state_dir.as_ref().canonicalize()?,
            laboratory: crate::experiments::Laboratory::default(),
            cancellations: Mutex::new(std::collections::HashMap::new()),
        })
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
        let body = self
            .store
            .db
            .query_row(
                "SELECT body FROM effects WHERE id=?1 AND grant_id=?2",
                params![operation, grant.id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| Error::missing("operation not found"))?;
        let receipt: ActionReceipt = serde_json::from_str(&body)?;
        if receipt.status == EffectStatus::Started {
            self.reconcile(&grant, receipt)
        } else {
            Ok(receipt)
        }
    }

    fn reconcile(&self, grant: &Grant, mut receipt: ActionReceipt) -> Result<ActionReceipt> {
        receipt.reconciled = true;
        receipt.finished_ms = Some(now_ms().to_string());
        // A completed write whose receipt was lost can be identified from its
        // intended content. Later unrelated edits leave the outcome unknown.
        if matches!(receipt.action.kind, ActionKind::Edit | ActionKind::Apply) {
            let workspace = self.branch_path(
                grant,
                if receipt.action.kind == ActionKind::Edit {
                    receipt.action.branch_id.as_deref()
                } else {
                    None
                },
            )?;
            if let (Some(path), Some(content)) = (&receipt.action.path, &receipt.action.content) {
                let current = self.host.version(grant, path, workspace.as_deref())?;
                if current.version == hash(content.as_bytes()) {
                    receipt.status = EffectStatus::Succeeded;
                    receipt.after = vec![current];
                    receipt.output="Desired artifact content observed during reconciliation; original response was lost.".into();
                } else {
                    receipt.status = EffectStatus::Unknown;
                    receipt.output="Cannot determine whether the previous edit completed. No retry was dispatched.".into();
                }
            } else {
                receipt.status = EffectStatus::Unknown;
            }
        } else {
            receipt.status = EffectStatus::Unknown;
            receipt.output =
                "Host cannot reconcile this interrupted operation. No retry was dispatched.".into();
        }
        self.save_receipt(&mut receipt)?;
        Ok(receipt)
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
        let mut receipt = ActionReceipt {
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
            evidence_ref: None,
        };
        // Commit the intent before the adapter is allowed to execute an effect.
        self.store.db.execute(
            "INSERT INTO effects(id,run_id,grant_id,body) VALUES (?1,?2,?3,?4)",
            params![
                receipt.operation_id,
                run_id,
                grant.id,
                serde_json::to_string(&receipt)?
            ],
        )?;
        let result = self.dispatch(run_id, &grant, &mut receipt);
        if let Err(error) = result {
            receipt.status = match error.code {
                -32001 => EffectStatus::Denied,
                -32002 => EffectStatus::Stale,
                _ => EffectStatus::Failed,
            };
            receipt.output = error.message;
        }
        receipt.finished_ms = Some(now_ms().to_string());
        receipt.elapsed_ms = Some(
            now_ms()
                .saturating_sub(counter(&receipt.started_ms)?)
                .to_string(),
        );
        self.save_receipt(&mut receipt)?;
        Ok(receipt)
    }

    fn dispatch(&self, run_id: &str, grant: &Grant, receipt: &mut ActionReceipt) -> Result<()> {
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
        let action = &receipt.action;
        self.store.require_attachment_write(run_id, action)?;
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
                let versions = self.host.branch(grant, &path)?;
                self.store.db.execute(
                    "INSERT INTO branches(id,grant_id,path,versions) VALUES (?1,?2,?3,?4)",
                    params![
                        branch_id,
                        grant.id,
                        path.to_string_lossy(),
                        serde_json::to_string(&versions)?
                    ],
                )?;
                receipt.output = branch_id;
                receipt.after = versions;
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
                receipt.before = vec![self.host.version(grant, path, workspace.as_deref())?];
                receipt.after =
                    vec![
                        self.host
                            .edit(grant, path, expected, content, workspace.as_deref())?,
                    ];
                if workspace.is_none() {
                    receipt.side_effects = self.store.invalidate_dependents(&grant.scope, path)?;
                }
                receipt.output = "Artifact written; fresh checks remain required.".into();
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
                let result = self.host.run_tool(
                    grant,
                    &tool,
                    workspace.as_deref(),
                    action.kind == ActionKind::Check,
                    Some(&self.cancellation(run_id)),
                )?;
                receipt.before = result.before;
                receipt.after = result.after;
                receipt.output = result.output;
                if workspace.is_none() {
                    for path in &result.writes {
                        receipt
                            .side_effects
                            .extend(self.store.invalidate_dependents(&grant.scope, path)?);
                    }
                }
                if !result.success {
                    receipt.status = EffectStatus::Failed;
                    return Ok(());
                }
                if workspace.is_none() {
                    for path in result.validates {
                        self.store.db.execute(
                            "DELETE FROM invalidated WHERE client=?1 AND project=?2 AND path=?3",
                            params![grant.scope.client, grant.scope.project, path],
                        )?;
                    }
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
                let mut check_receipts = Vec::new();
                let mut checked_inputs = Vec::new();
                for (index, check) in required_checks.into_iter().enumerate() {
                    let check_action = Action {
                        operation_id: format!("{}/check/{index}", action.operation_id),
                        kind: ActionKind::Check,
                        path: None,
                        expected_version: None,
                        content: None,
                        tool: Some(check.clone()),
                        branch_id: action.branch_id.clone(),
                        intervention_ref: None,
                        implementation: None,
                    };
                    let result = self.execute(run_id, check_action)?;
                    if result.status != EffectStatus::Succeeded {
                        return Err(Error::denied(
                            "branch acceptance check failed; inspect its durable receipt",
                        ));
                    }
                    checked_inputs.extend(result.after);
                    check_receipts.push(result.operation_id);
                }
                let read = self.host.read(
                    grant,
                    &ArtifactRead {
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
                if !checked_inputs.iter().any(|input| input.path == path) {
                    return Err(Error::denied(
                        "acceptance checks did not inspect the applied artifact",
                    ));
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
                receipt.before = vec![self.host.version(grant, path, None)?];
                self.store.require_attachment_write(run_id, action)?;
                receipt.after = vec![self.host.edit(grant, path, expected, &read.content, None)?];
                receipt.side_effects = self.store.invalidate_dependents(&grant.scope, path)?;
                receipt.output = format!(
                    "Checked branch artifact applied. Check receipts: {}",
                    check_receipts.join(", ")
                );
            }
        }
        receipt.status = EffectStatus::Succeeded;
        Ok(())
    }

    fn save_receipt(&self, receipt: &mut ActionReceipt) -> Result<()> {
        // Operation IDs can fill their entire wire limit. Hash the evidence
        // identifier so even those operations have a valid, stable event ID.
        let evidence_ref = format!(
            "receipt:{}",
            crate::host::hash(receipt.operation_id.as_bytes())
        );
        receipt.evidence_ref = Some(evidence_ref.clone());
        let transaction = self.store.write_transaction()?;
        self.store.db.execute(
            "UPDATE effects SET body=?2 WHERE id=?1",
            params![receipt.operation_id, serde_json::to_string(receipt)?],
        )?;
        let grant = self.store.run_grant(&receipt.run_id)?;
        let sequence: i64 = self.store.db.query_row(
            "SELECT rowid FROM effects WHERE id=?1",
            [&receipt.operation_id],
            |r| r.get(0),
        )?;
        let event = Event {
            id: evidence_ref,
            scope: grant.scope,
            run_id: receipt.run_id.clone(),
            producer: "ribosome-host".into(),
            sequence: sequence.to_string(),
            kind: if receipt.action.kind == ActionKind::Check {
                "check_completion"
            } else {
                "action_completion"
            }
            .into(),
            timestamp_ms: receipt
                .finished_ms
                .clone()
                .unwrap_or_else(|| now_ms().to_string()),
            parents: vec![],
            correlation: receipt.operation_id.clone(),
            artifacts: receipt.after.clone(),
            payload: serde_json::to_value(receipt)?.as_object().unwrap().clone(),
            provenance: Provenance {
                origin: Origin::Observed,
                source_refs: vec![],
                scenario_family: "host-effects".into(),
                split: crate::validation::derived_split(&grant.visible_splits),
                limitations: vec![],
            },
        };
        self.store.ingest(&event)?;
        transaction.commit()?;
        Ok(())
    }
}
