use crate::{contracts::*, error::Result, store::Store};
use rusqlite::params;

impl Store {
    pub fn archive(&self, grant: &Grant) -> Result<ArchivePage> {
        let mut statement=self.db.prepare("SELECT policy_id,cell,implementation_id,quality,evaluation_id FROM archive WHERE client=?1 AND project=?2 AND context=?3 ORDER BY policy_id,cell LIMIT 1000")?;
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
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut cells = Vec::new();
        for (policy_id, cell, id, quality, evaluation_id) in rows {
            if let Ok(implementation) = self.record(grant, &id)
                && self.is_admitted(grant, &implementation)?
            {
                cells.push(ArchiveCell {
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
