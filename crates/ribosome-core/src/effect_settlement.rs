use crate::{
    contracts::*,
    effect_finalization::EffectObservation,
    effects::Runtime,
    error::{Error, Result},
    host::hash,
    validation::{derived_split, now_ms, validate},
};
use rusqlite::{OptionalExtension, params};

impl Runtime {
    fn require_effect_owner(&self, owner: &Grant, operation: &str) -> Result<String> {
        if self.store.grant(&owner.id)? != *owner {
            return Err(Error::denied(
                "owner grant does not match the stored policy",
            ));
        }
        self.store
            .db
            .query_row(
                "SELECT run_id FROM effects WHERE id=?1 AND grant_id=?2",
                params![operation, owner.id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::missing("operation not found in owner grant"))
    }

    fn workspace_versions(&self, owner: &Grant) -> Result<Vec<ArtifactRef>> {
        let mut paths = owner.paths.clone();
        paths.sort();
        paths.dedup();
        paths
            .iter()
            .map(|path| self.host.version(owner, path, None))
            .collect()
    }

    fn receipt_version(&self, operation: &str) -> Result<String> {
        let body: String = self.store.db.query_row(
            "SELECT body FROM effects WHERE id=?1",
            [operation],
            |row| row.get(0),
        )?;
        Ok(hash(body.as_bytes()))
    }

    /// Owner interface; deliberately absent from the worker tool protocol.
    pub fn inspect_effect(&self, owner: &Grant, operation: &str) -> Result<EffectInspection> {
        let run = self.require_effect_owner(owner, operation)?;
        let active:bool=self.store.db.query_row("SELECT phase<>'finalized' AND EXISTS(SELECT 1 FROM runs WHERE runs.id=effects.run_id AND status='running') FROM effects WHERE id=?1",[operation],|row|row.get(0))?;
        if active {
            return Err(Error::conflict(
                "stop the owning run before inspecting an unsettled effect",
            ));
        }
        let receipt = self.lookup(&run, operation)?;
        Ok(EffectInspection {
            receipt,
            receipt_version: self.receipt_version(operation)?,
            workspace_versions: self.workspace_versions(owner)?,
        })
    }

    /// The owner attests that the external executor has stopped. This releases
    /// ownership uncertainty without changing the original execution evidence.
    pub fn settle_effect(
        &self,
        owner: &Grant,
        request: &EffectSettlementRequest,
    ) -> Result<ActionReceipt> {
        validate("EffectSettlementRequest", &serde_json::to_value(request)?)?;
        if serde_json::to_vec(request)?.len() > crate::validation::MAX_FRAME / 4 {
            return Err(Error::invalid("settlement exceeds 256 KiB"));
        }
        let run = self.require_effect_owner(owner, &request.operation_id)?;
        let active:bool=self.store.db.query_row("SELECT settlement IS NULL AND EXISTS(SELECT 1 FROM runs WHERE runs.id=effects.run_id AND status='running') FROM effects WHERE id=?1",[&request.operation_id],|row|row.get(0))?;
        if active {
            return Err(Error::conflict(
                "stop the owning run before settling its executor",
            ));
        }
        let mut receipt = self.lookup_for_recovery(&run, &request.operation_id)?;
        if let Some(saved) = &receipt.settlement {
            if saved.request != *request {
                return Err(Error::conflict(
                    "operation already settled with a different decision",
                ));
            }
            return self.store.receipt_for_delivery(owner, receipt);
        }
        if receipt.status != EffectStatus::Unknown
            || self.receipt_version(&request.operation_id)? != request.expected_receipt_version
        {
            return Err(Error::conflict(
                "settlement requires the current unknown receipt",
            ));
        }
        let mut expected = request.workspace_versions.clone();
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        if expected != self.workspace_versions(owner)? {
            return Err(Error::conflict(
                "workspace changed or settlement omitted a granted artifact; inspect again",
            ));
        }
        for reference in &request.source_refs {
            self.store.require_reference(owner, reference)?;
        }
        let tx = self.store.write_transaction()?;
        self.require_effect_owner(owner, &request.operation_id)?;
        let (phase,settled,running,captured):(String,bool,bool,Option<String>)=tx.query_row("SELECT phase,settlement IS NOT NULL,EXISTS(SELECT 1 FROM runs WHERE runs.id=effects.run_id AND status='running'),observation FROM effects WHERE id=?1",[&request.operation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
        if settled {
            let saved = self.lookup_for_recovery(&run, &request.operation_id)?;
            if saved
                .settlement
                .as_ref()
                .is_some_and(|s| s.request == *request)
            {
                return self.store.receipt_for_delivery(owner, saved);
            }
            return Err(Error::conflict(
                "operation already settled with a different decision",
            ));
        }
        if phase != "finalized"
            || running
            || self.receipt_version(&request.operation_id)? != request.expected_receipt_version
        {
            return Err(Error::conflict(
                "stop the owning run and settle its pending bookkeeping before releasing the executor",
            ));
        }
        // The executor may have mutated outputs after the first unknown
        // observation. Invalidate again at the owner's settlement boundary.
        let writes = if let Some(captured) = captured {
            let observation: EffectObservation = serde_json::from_str(&captured)?;
            observation.writes
        } else {
            match receipt.action.kind {
                ActionKind::Apply => receipt.action.path.iter().cloned().collect(),
                ActionKind::Edit if receipt.action.branch_id.is_none() => {
                    receipt.action.path.iter().cloned().collect()
                }
                ActionKind::Execute if receipt.action.branch_id.is_none() => owner
                    .writable_paths
                    .clone()
                    .unwrap_or_else(|| owner.paths.clone()),
                _ => vec![],
            }
        };
        for path in writes {
            self.store
                .invalidate_artifact_and_dependents(&owner.scope, &path)?;
        }
        let settlement = EffectSettlement {
            request: request.clone(),
            evidence_ref: format!("settlement:{}", hash(request.operation_id.as_bytes())),
            recorded_ms: now_ms().to_string(),
        };
        let sequence: i64 = tx.query_row(
            "SELECT rowid FROM effects WHERE id=?1",
            [&request.operation_id],
            |row| row.get(0),
        )?;
        let mut references = request.source_refs.clone();
        references.extend(receipt.evidence_ref.clone());
        references.sort();
        references.dedup();
        self.store.ingest(&Event { id:settlement.evidence_ref.clone(),scope:owner.scope.clone(),run_id:run,producer:"ribosome-host-settlement".into(),sequence:sequence.to_string(),kind:"effect_settled".into(),timestamp_ms:settlement.recorded_ms.clone(),parents:receipt.evidence_ref.iter().cloned().collect(),correlation:request.operation_id.clone(),artifacts:request.workspace_versions.clone(),payload:serde_json::to_value(&settlement)?.as_object().unwrap().clone(),provenance:Provenance { origin:Origin::Observed,source_refs:references,scenario_family:"host-effect-settlement".into(),split:derived_split(&owner.visible_splits),limitations:vec!["The owner attests that the executor has stopped. This does not establish the original effect or certify current artifacts; fresh validation remains required.".into()] } })?;
        tx.execute(
            "UPDATE effects SET settlement=?2 WHERE id=?1",
            params![request.operation_id, serde_json::to_string(&settlement)?],
        )?;
        tx.commit()?;
        receipt.settlement = Some(settlement);
        self.store.receipt_for_delivery(owner, receipt)
    }
}

impl ActionReceipt {
    pub fn requires_reconciliation(&self) -> bool {
        self.status == EffectStatus::Started
            || (self.status == EffectStatus::Unknown && self.settlement.is_none())
    }
}
