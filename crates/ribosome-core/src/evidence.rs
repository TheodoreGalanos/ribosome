use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, id, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

impl Store {
    pub fn ingest(&self, event: &Event) -> Result<bool> {
        validate("Event", &serde_json::to_value(event)?)?;
        let body = serde_json::to_string(event)?;
        if body.len() > crate::validation::MAX_FRAME / 4 {
            return Err(Error::invalid(
                "event exceeds 256 KiB; reference large artifacts instead",
            ));
        }
        let previous: Option<String> = self.db.query_row("SELECT body FROM events WHERE id=?1 OR (client=?2 AND project=?3 AND run_id=?4 AND producer=?5 AND sequence=?6)",params![event.id,event.scope.client,event.scope.project,event.run_id,event.producer,event.sequence],|r|r.get(0)).optional()?;
        if let Some(previous) = previous {
            return if previous == body {
                Ok(false)
            } else {
                Err(Error::conflict(
                    "event identity reused with different evidence",
                ))
            };
        }
        self.db.execute("INSERT INTO events(id,client,project,run_id,producer,sequence,split,body) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![event.id,event.scope.client,event.scope.project,event.run_id,event.producer,event.sequence,serde_json::to_value(&event.provenance.split)?.as_str(),body])?;
        Ok(true)
    }

    pub fn evidence(&self, grant: &Grant, request: &EvidenceRequest) -> Result<EvidencePage> {
        self.evidence_excluding_activity(grant, request, None)
    }

    pub(crate) fn evidence_excluding_activity(
        &self,
        grant: &Grant,
        request: &EvidenceRequest,
        active_run: Option<&str>,
    ) -> Result<EvidencePage> {
        validate("EvidenceRequest", &serde_json::to_value(request)?)?;
        let cursor = counter(&request.cursor)?.min(i64::MAX as u64) as i64;
        let splits = serde_json::to_string(&grant.visible_splits)?;
        if let Some(source_run) = &request.run_id {
            let exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND split IN (SELECT value FROM json_each(?4)) AND (?5 IS NULL OR run_id<>?5 OR producer<>'pi-agent'))",
                params![grant.scope.client, grant.scope.project, source_run, splits, active_run],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(Error::missing(
                    "No visible source events for this run_id; omit run_id to inspect all workflows in the granted scope",
                ));
            }
        }
        let mut stmt = self.db.prepare("SELECT cursor,body FROM events WHERE client=?1 AND project=?2 AND cursor>?3 AND split IN (SELECT value FROM json_each(?4)) AND (?5 IS NULL OR run_id=?5) AND (?7 IS NULL OR run_id<>?7 OR producer<>'pi-agent') ORDER BY cursor LIMIT ?6")?;
        let rows = stmt.query_map(
            params![
                grant.scope.client,
                grant.scope.project,
                cursor,
                splits,
                request.run_id,
                request.limit,
                active_run
            ],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )?;
        let mut events = Vec::new();
        let mut next = cursor;
        let mut frontier = serde_json::Map::new();
        let mut bytes = 0;
        for row in rows {
            let (c, b) = row?;
            let event: Event = serde_json::from_str(&b)?;
            if !event.provenance.source_refs.is_empty()
                && self.require_reference(grant, &event.id).is_err()
            {
                next = c;
                continue;
            }
            if bytes + b.len() > crate::validation::MAX_FRAME / 2 {
                break;
            }
            bytes += b.len();
            next = c;
            let key = format!("{}/{}", event.run_id, event.producer);
            let seq = counter(&event.sequence)?;
            let old = frontier
                .get(&key)
                .and_then(Value::as_str)
                .map(counter)
                .transpose()?
                .unwrap_or(0);
            frontier.insert(key, Value::String(seq.max(old).to_string()));
            events.push(event);
        }
        let mut invalidated_stmt = self
            .db
            .prepare("SELECT path FROM invalidated WHERE client=?1 AND project=?2 LIMIT 1000")?;
        let invalidated_paths = invalidated_stmt
            .query_map(params![grant.scope.client, grant.scope.project], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| grant.paths.contains(p))
            .collect();
        let dependencies = self
            .dependencies(&grant.scope)?
            .into_iter()
            .filter(|d| {
                grant.paths.contains(&d.source.path) && grant.paths.contains(&d.dependent.path)
            })
            .collect();
        let page = EvidencePage {
            events,
            cursor: next.to_string(),
            frontier,
            dependencies,
            invalidated_paths,
        };
        if serde_json::to_vec(&page)?.len() > crate::validation::MAX_FRAME - 8192 {
            return Err(Error::exhausted(
                "evidence view exceeds transport capacity; reduce the event limit or narrow granted paths",
            ));
        }
        Ok(page)
    }

    /// The adapter reports visible message activity. Content is a statement by
    /// its role, never independent proof of a host effect or evaluation result.
    pub fn append_agent_activity(&self, run_id: &str, batch: &AgentActivityBatch) -> Result<()> {
        validate("AgentActivityBatch", &serde_json::to_value(batch)?)?;
        let grant = self.run_grant(run_id)?;
        let split = crate::validation::derived_split(&grant.visible_splits);
        let tx = self.write_transaction()?;
        for entry in &batch.entries {
            let payload = json!({"role":entry.role,"content":entry.content,"authority":"role-authored message; verify effects through host receipts"}).as_object().unwrap().clone();
            let previous: Option<String> = tx.query_row("SELECT body FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND producer='pi-agent' AND sequence=?4", params![grant.scope.client,grant.scope.project,run_id,entry.sequence], |r|r.get(0)).optional()?;
            if let Some(previous) = previous {
                let previous: Event = serde_json::from_str(&previous)?;
                if previous.payload != payload
                    || previous.timestamp_ms != entry.timestamp_ms
                    || previous.provenance.source_refs != entry.source_refs
                {
                    return Err(Error::conflict(
                        "agent activity sequence reused with different content",
                    ));
                }
                continue;
            }
            self.ingest(&Event {
                id: id(),
                scope: grant.scope.clone(),
                run_id: run_id.into(),
                producer: "pi-agent".into(),
                sequence: entry.sequence.clone(),
                kind: "agent_message".into(),
                timestamp_ms: entry.timestamp_ms.clone(),
                parents: vec![],
                correlation: run_id.into(),
                artifacts: vec![],
                payload,
                provenance: Provenance {
                    origin: Origin::Observed,
                    source_refs: entry.source_refs.clone(),
                    scenario_family: "pi-agent-activity".into(),
                    split: split.clone(),
                    limitations: vec![
                        "Observed Pi message, not verified task truth or a host effect receipt. References conservatively include earlier retrieved records and events."
                            .into(),
                    ],
                },
            })?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn add_dependency(&self, scope: &Scope, dependency: &Dependency) -> Result<()> {
        validate("Dependency", &serde_json::to_value(dependency)?)?;
        let body = serde_json::to_string(dependency)?;
        let tx = self.write_transaction()?;
        let (count, bytes): (i64, i64) = tx.query_row("SELECT count(*),coalesce(sum(length(CAST(body AS BLOB))),0) FROM dependencies WHERE client=?1 AND project=?2 AND NOT (source=?3 AND dependent=?4)", params![scope.client, scope.project, dependency.source.path, dependency.dependent.path], |r| Ok((r.get(0)?,r.get(1)?)))?;
        if count >= 1000 || bytes + body.len() as i64 > (crate::validation::MAX_FRAME / 4) as i64 {
            return Err(Error::exhausted(
                "dependency graph exceeds the local scope capacity of 1000 edges or 256 KiB",
            ));
        }
        tx.execute("INSERT INTO dependencies(client,project,source,dependent,body) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(client,project,source,dependent) DO UPDATE SET body=excluded.body",params![scope.client,scope.project,dependency.source.path,dependency.dependent.path,body])?;
        tx.commit()?;
        Ok(())
    }

    pub fn dependencies(&self, scope: &Scope) -> Result<Vec<Dependency>> {
        let mut stmt = self
            .db
            .prepare("SELECT body FROM dependencies WHERE client=?1 AND project=?2 LIMIT 1000")?;
        stmt.query_map(params![scope.client, scope.project], |r| {
            r.get::<_, String>(0)
        })?
        .map(|row| Ok(serde_json::from_str(&row?)?))
        .collect()
    }

    pub fn invalidate_dependents(&self, scope: &Scope, path: &str) -> Result<Vec<String>> {
        let mut stmt=self.db.prepare("WITH RECURSIVE affected(path) AS (SELECT dependent FROM dependencies WHERE client=?1 AND project=?2 AND source=?3 UNION SELECT d.dependent FROM dependencies d JOIN affected a ON d.source=a.path WHERE d.client=?1 AND d.project=?2) SELECT path FROM affected LIMIT 1000")?;
        let paths = stmt
            .query_map(params![scope.client, scope.project, path], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for path in &paths {
            self.db.execute("INSERT INTO invalidated(client,project,path) VALUES (?1,?2,?3) ON CONFLICT DO NOTHING",params![scope.client,scope.project,path])?;
        }
        Ok(paths)
    }
}
