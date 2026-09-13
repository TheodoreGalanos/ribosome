use crate::{
    contracts::*,
    error::Result,
    store::Store,
    validation::{counter, validate},
};
use rusqlite::{OptionalExtension, params};

impl Store {
    /// References are navigation metadata. Callers must retrieve current
    /// records, work status or receipts before relying on their contents.
    pub fn continuation(&self, run: &str, request: &ContinuationRead) -> Result<ContinuationPage> {
        validate("ContinuationRead", &serde_json::to_value(request)?)?;
        let grant = self.run_grant(run)?;
        let after = counter(&request.after)?.min(i64::MAX as u64) as i64;
        let kind = serde_json::to_value(&request.kind)?;
        let query = "SELECT position,id,version FROM (
            SELECT rowid AS position,id,version FROM records
            WHERE ?7='obligation' AND kind='obligation' AND client=?4 AND project=?5
              AND json_extract(body,'$.body.state')<>'satisfied'
              AND id IN (SELECT id FROM context_sources WHERE kind='record' AND segment_id IN (SELECT id FROM context_segments WHERE run_id=?2))
            UNION ALL SELECT rowid,id,NULL FROM effects WHERE ?7='effect' AND run_id=?2 AND grant_id=?6
            UNION ALL SELECT rowid,id,NULL FROM work WHERE ?7='work' AND json_extract(body,'$.parent_id')=?2 AND grant_id=?6
        ) WHERE position>?1 ORDER BY position LIMIT ?3";
        let rows = self
            .db
            .prepare(query)?
            .query_map(
                params![
                    after,
                    run,
                    request.limit,
                    grant.scope.client,
                    grant.scope.project,
                    grant.id,
                    kind.as_str()
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let next = rows.last().map_or(after, |row| row.0).to_string();
        let complete = rows.len() < request.limit as usize;
        let mut references = Vec::new();
        for (_, id, version) in rows {
            if request.kind == ContinuationKind::Obligation {
                match self.record(&grant, &id) {
                    Ok(_) => {}
                    Err(error) if matches!(error.code, -32001 | -32004) => continue,
                    Err(error) => return Err(error),
                }
            }
            references.push(ContinuationReference { id, version });
        }
        let evidence_cursor = self
            .db
            .query_row(
                "SELECT json_extract(body,'$.event_cursor') FROM checkpoints WHERE run_id=?1",
                [run],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| "0".into());
        Ok(ContinuationPage {
            kind: request.kind.clone(),
            references,
            next,
            complete,
            evidence_cursor,
        })
    }
}
