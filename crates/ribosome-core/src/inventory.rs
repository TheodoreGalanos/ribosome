use crate::{contracts::*, error::Result, store::Store};
use rusqlite::params;

impl Store {
    pub fn archive(&self, grant: &Grant) -> Result<ArchivePage> {
        let mut statement=self.db.prepare("SELECT policy_id,cell,implementation_id,quality,evaluation_id,evidence FROM archive WHERE client=?1 AND project=?2 AND context=?3 ORDER BY policy_id,cell LIMIT 1000")?;
        let rows = statement
            .query_map(
                params![grant.scope.client, grant.scope.project, grant.context],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut cells = Vec::new();
        for (policy_id, cell, id, quality, evaluation_id, evidence) in rows {
            // Pre-aggregate archive rows remain readable on disk, but are not
            // represented as supported aggregate elites.
            let Some(evidence) = evidence else {
                continue;
            };
            let evidence: serde_json::Value = serde_json::from_str(&evidence)?;
            let Some(admission_ref) = evidence["admission_ref"].as_str() else {
                continue;
            };
            if !self.source_available(grant, "record", admission_ref)? {
                continue;
            }
            if let Ok(implementation) = self.record(grant, &id)
                && self.is_admitted(grant, &implementation)?
            {
                cells.push(ArchiveCell {
                    admission_ref: Some(admission_ref.into()),
                    evaluation_refs: Some(serde_json::from_value(
                        evidence["evaluation_refs"].clone(),
                    )?),
                    implementation_version: Some(serde_json::from_value(
                        evidence["implementation_version"].clone(),
                    )?),
                    limitations: Some(serde_json::from_value(evidence["limitations"].clone())?),
                    context: grant.context.clone(),
                    policy_id,
                    cell,
                    implementation,
                    quality,
                    evaluation_id,
                });
            }
        }
        Ok(ArchivePage { cells })
    }
}
