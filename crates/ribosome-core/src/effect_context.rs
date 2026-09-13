use crate::{
    contracts::{ActionReceipt, Grant},
    effect_finalization::EffectObservation,
    error::{Error, Result},
    store::Store,
};
use rusqlite::params;

impl ActionReceipt {
    fn withhold_content(&mut self) {
        self.content_available = Some(false);
        self.action.content = None;
        self.output =
            "Receipt content withheld because its sources are unavailable or unverified.".into();
        self.side_effects.clear();
        self.validations = None;
        self.restored_validity = None;
        self.restored_properties = None;
        if let Some(settlement) = &mut self.settlement {
            settlement.request.reason = "Source content withheld.".into();
            settlement.request.source_refs.clear();
        }
    }
}

impl Store {
    pub(crate) fn receipt_for_delivery(
        &self,
        grant: &Grant,
        mut receipt: ActionReceipt,
    ) -> Result<ActionReceipt> {
        let mut available = receipt.content_available == Some(true);
        if let Some(reference) = &receipt.evidence_ref {
            available &= self.source_available(grant, "event", reference)?;
        } else {
            available = false;
        }
        if let Some(settlement) = &receipt.settlement {
            available &= self.source_available(grant, "event", &settlement.evidence_ref)?;
        }
        if !available {
            receipt.withhold_content();
        }
        Ok(receipt)
    }

    pub(crate) fn cleanup_effect_content(&self, source: &str) -> Result<()> {
        self.cleanup_receipt_copies(Some(source), None)
    }

    pub(crate) fn cleanup_legacy_effect_content(&self, grant: &Grant) -> Result<()> {
        self.cleanup_receipt_copies(None, Some(grant))
    }

    fn cleanup_receipt_copies(
        &self,
        source: Option<&str>,
        legacy_grant: Option<&Grant>,
    ) -> Result<()> {
        let rows = self.db.prepare("SELECT id,phase,body,observation,settlement FROM effects WHERE json_extract(body,'$.evidence_ref')=?1 OR json_extract(settlement,'$.evidence_ref')=?1 OR (json_extract(body,'$.content_available') IS NULL AND grant_id IN (SELECT id FROM grants WHERE json_extract(body,'$.scope.client')=?2 AND json_extract(body,'$.scope.project')=?3 AND json_array_length(body,'$.visible_splits')=1 AND json_extract(body,'$.visible_splits[0]')='development'))")?
            .query_map(params![source,legacy_grant.map(|g| &g.scope.client),legacy_grant.map(|g| &g.scope.project)], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<String>>(4)?)))?
            .collect::<std::result::Result<Vec<_>,_>>()?;
        for (id, phase, body, observation, settlement) in rows {
            let mut receipt: ActionReceipt = serde_json::from_str(&body)?;
            if phase != "finalized" {
                return Err(Error::conflict(
                    "receipt cleanup awaits effect reconciliation",
                ));
            }
            receipt.withhold_content();
            let observation = observation
                .map(|body| -> Result<String> {
                    let mut observed: EffectObservation = serde_json::from_str(&body)?;
                    observed.receipt.withhold_content();
                    Ok(serde_json::to_string(&observed)?)
                })
                .transpose()?;
            let settlement = settlement
                .map(|body| -> Result<String> {
                    let mut settled: crate::contracts::EffectSettlement =
                        serde_json::from_str(&body)?;
                    settled.request.reason = "Source content withheld.".into();
                    settled.request.source_refs.clear();
                    Ok(serde_json::to_string(&settled)?)
                })
                .transpose()?;
            self.db.execute(
                "UPDATE effects SET body=?2,observation=?3,settlement=?4 WHERE id=?1",
                params![
                    id,
                    serde_json::to_string(&receipt)?,
                    observation,
                    settlement
                ],
            )?;
        }
        Ok(())
    }
}
