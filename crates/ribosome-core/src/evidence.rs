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
        let transaction = if self.db.is_autocommit() {
            Some(self.write_transaction()?)
        } else {
            None
        };
        self.db.execute("INSERT INTO events(id,client,project,run_id,producer,sequence,split,body) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![event.id,event.scope.client,event.scope.project,event.run_id,event.producer,event.sequence,serde_json::to_value(&event.provenance.split)?.as_str(),body])?;
        crate::sources::source_edges(
            &self.db,
            "event",
            &event.id,
            &event.provenance.source_refs,
            &[],
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
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
        let through = request
            .through_cursor
            .as_deref()
            .map(counter)
            .transpose()?
            .map(|v| v.min(i64::MAX as u64) as i64);
        let splits = serde_json::to_string(&grant.visible_splits)?;
        let corpus = grant
            .discovery_corpus
            .as_ref()
            .map(|reference| self.discovery_corpus(grant, reference))
            .transpose()?;
        let corpus_events = corpus.as_ref().map(|corpus| {
            corpus
                .source_windows
                .iter()
                .flat_map(|window| window.event_refs.iter().cloned())
                .collect::<Vec<_>>()
        });
        let corpus_selection = corpus_events
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        if request.neighbors == Some(true) && request.event_refs.is_none() {
            return Err(Error::invalid(
                "neighbor selection requires explicit event_refs",
            ));
        }
        if let Some(references) = &request.event_refs {
            for reference in references {
                if corpus_events
                    .as_ref()
                    .is_some_and(|events| !events.contains(reference))
                {
                    return Err(Error::missing("selected source event is unavailable"));
                }
                let visible: bool = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND client=?2 AND project=?3 AND split IN (SELECT value FROM json_each(?4)) AND (?5 IS NULL OR run_id<>?5 OR producer NOT IN ('pi-agent','pi-compactor')))",
                    params![reference,grant.scope.client,grant.scope.project,splits,active_run], |r| r.get(0),
                )?;
                if !visible || !self.source_available(grant, "event", reference)? {
                    return Err(Error::missing("selected source event is unavailable"));
                }
            }
        }
        if let Some(source_run) = &request.run_id {
            let exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND split IN (SELECT value FROM json_each(?4)) AND (?5 IS NULL OR run_id<>?5 OR producer NOT IN ('pi-agent','pi-compactor')) AND (?6 IS NULL OR id IN (SELECT value FROM json_each(?6))))",
                params![grant.scope.client, grant.scope.project, source_run, splits, active_run, corpus_selection],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(Error::missing(
                    "No visible source events for this run_id; omit run_id to inspect all workflows in the granted scope",
                ));
            }
        }
        let references = request
            .event_refs
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let mut stmt = self.db.prepare(
            "SELECT cursor,body FROM events e
             WHERE client=?1 AND project=?2 AND cursor>?3
               AND split IN (SELECT value FROM json_each(?4))
               AND (?5 IS NULL OR run_id=?5)
               AND (?7 IS NULL OR run_id<>?7 OR producer NOT IN ('pi-agent','pi-compactor'))
               AND (?8 IS NULL OR cursor<=?8)
               AND (?15 IS NULL OR id IN (SELECT value FROM json_each(?15)))
               AND (?9 IS NULL OR json_extract(body,'$.kind')=?9)
               AND (?10 IS NULL OR instr(lower(body),lower(?10))>0)
               AND (?11 IS NULL OR EXISTS(SELECT 1 FROM json_each(body,'$.artifacts') a WHERE json_extract(a.value,'$.path')=?11 AND json_extract(a.value,'$.version')=?12))
               AND (?13 IS NULL OR id IN (SELECT value FROM json_each(?13)) OR (?14 AND (
                   id IN (SELECT p.value FROM events seed,json_each(seed.body,'$.parents') p WHERE seed.id IN (SELECT value FROM json_each(?13)))
                   OR EXISTS(SELECT 1 FROM json_each(e.body,'$.parents') p WHERE p.value IN (SELECT value FROM json_each(?13)))
               )))
             ORDER BY cursor LIMIT ?6",
        )?;
        let rows = stmt.query_map(
            params![
                grant.scope.client,
                grant.scope.project,
                cursor,
                splits,
                request.run_id,
                request.limit,
                active_run,
                through,
                request.kind,
                request.query,
                request.artifact.as_ref().map(|a| &a.path),
                request.artifact.as_ref().map(|a| &a.version),
                references,
                request.neighbors.unwrap_or(false),
                corpus_selection,
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
            if !self.source_available(grant, "event", &event.id)? {
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
        let targeted = request.event_refs.is_some()
            || request.kind.is_some()
            || request.artifact.is_some()
            || request.query.is_some();
        let selected_artifacts: Vec<_> = events.iter().flat_map(|event| &event.artifacts).collect();
        let mut invalidated_stmt = self
            .db
            .prepare("SELECT path FROM invalidated WHERE client=?1 AND project=?2 LIMIT 1000")?;
        let mut invalidated_paths = invalidated_stmt
            .query_map(params![grant.scope.client, grant.scope.project], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| {
                grant.paths.contains(p)
                    && (!targeted || selected_artifacts.iter().any(|a| &a.path == p))
            })
            .collect::<Vec<_>>();
        if corpus.is_some() {
            invalidated_paths.clear();
        }
        let dependencies = corpus
            .map(|corpus| Ok(corpus.dependencies))
            .unwrap_or_else(|| self.dependencies(&grant.scope))?
            .into_iter()
            .filter(|d| {
                grant.paths.contains(&d.source.path)
                    && grant.paths.contains(&d.dependent.path)
                    && (!targeted
                        || selected_artifacts.contains(&&d.source)
                        || selected_artifacts.contains(&&d.dependent))
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
        let tx = self
            .db
            .is_autocommit()
            .then(|| self.write_transaction())
            .transpose()?;
        for entry in &batch.entries {
            let payload = json!({"role":entry.role,"content":entry.content,"authority":"role-authored message; verify effects through host receipts"}).as_object().unwrap().clone();
            let previous: Option<String> = self.db.query_row("SELECT body FROM events WHERE client=?1 AND project=?2 AND run_id=?3 AND producer='pi-agent' AND sequence=?4", params![grant.scope.client,grant.scope.project,run_id,entry.sequence], |r|r.get(0)).optional()?;
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
                        "Observed Pi message, not verified task truth or a host effect receipt. References conservatively include earlier retrieved records, events and artifact observations."
                            .into(),
                    ],
                },
            })?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
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
        let transaction = self
            .db
            .is_autocommit()
            .then(|| self.write_transaction())
            .transpose()?;
        self.db.execute("INSERT INTO validity_clock(client,project,generation) VALUES(?1,?2,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1",params![scope.client,scope.project])?;
        let generation = self.validity_generation(scope)?;
        let mut stmt=self.db.prepare("WITH RECURSIVE affected(path) AS (SELECT dependent FROM dependencies WHERE client=?1 AND project=?2 AND source=?3 UNION SELECT d.dependent FROM dependencies d JOIN affected a ON d.source=a.path WHERE d.client=?1 AND d.project=?2) SELECT path FROM affected LIMIT 1000")?;
        let paths = stmt
            .query_map(params![scope.client, scope.project, path], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for path in &paths {
            self.db.execute("INSERT INTO artifact_invalidation_generations(client,project,path,generation) VALUES(?1,?2,?3,?4) ON CONFLICT(client,project,path) DO UPDATE SET generation=excluded.generation",params![scope.client,scope.project,path,generation])?;
            self.db.execute("INSERT INTO invalidated(client,project,path,generation) VALUES (?1,?2,?3,?4) ON CONFLICT(client,project,path) DO UPDATE SET generation=excluded.generation",params![scope.client,scope.project,path,generation])?;
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(paths)
    }

    pub(crate) fn validity_generation(&self, scope: &Scope) -> Result<i64> {
        Ok(self
            .db
            .query_row(
                "SELECT generation FROM validity_clock WHERE client=?1 AND project=?2",
                params![scope.client, scope.project],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub(crate) fn invalidate_artifact_and_dependents(
        &self,
        scope: &Scope,
        path: &str,
    ) -> Result<()> {
        let transaction = self
            .db
            .is_autocommit()
            .then(|| self.write_transaction())
            .transpose()?;
        self.invalidate_dependents(scope, path)?;
        let generation = self.validity_generation(scope)?;
        self.db.execute("INSERT INTO artifact_invalidation_generations(client,project,path,generation) VALUES(?1,?2,?3,?4) ON CONFLICT(client,project,path) DO UPDATE SET generation=excluded.generation",params![scope.client,scope.project,path,generation])?;
        self.db.execute("INSERT INTO invalidated(client,project,path,generation) VALUES(?1,?2,?3,?4) ON CONFLICT(client,project,path) DO UPDATE SET generation=excluded.generation",params![scope.client,scope.project,path,generation])?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }

    pub(crate) fn artifact_invalidation_generation(
        &self,
        scope: &Scope,
        path: &str,
    ) -> Result<i64> {
        Ok(self.db.query_row("SELECT generation FROM artifact_invalidation_generations WHERE client=?1 AND project=?2 AND path=?3",params![scope.client,scope.project,path],|row|row.get(0)).optional()?.unwrap_or(0))
    }
}
