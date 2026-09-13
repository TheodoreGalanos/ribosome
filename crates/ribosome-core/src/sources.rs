use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::collections::HashSet;

/// Populate explicit edges from the existing declared record/event contracts.
/// Stored bodies remain the observation; migration does not invent missing lineage.
pub(crate) fn source_edges(
    db: &Connection,
    kind: &str,
    id: &str,
    references: &[String],
    released: &[String],
) -> Result<()> {
    let mut pending = Vec::new();
    for reference in references {
        let source_kind: Option<String> = db.query_row(
            "SELECT kind FROM (SELECT 'record' AS kind FROM records WHERE id=?1 UNION ALL SELECT 'event' FROM events WHERE id=?1 UNION ALL SELECT 'artifact' FROM artifact_snapshots WHERE id=?1) LIMIT 1",
            [reference], |row| row.get(0),
        ).optional()?;
        pending.push((
            source_kind.unwrap_or_else(|| "unknown".into()),
            reference.clone(),
            None::<String>,
            !released.contains(reference),
        ));
    }
    capture_mutable_ancestry(db, kind, id, pending)
}

type SourceEdge = (String, String, Option<String>, bool);

fn capture_mutable_ancestry(
    db: &Connection,
    kind: &str,
    id: &str,
    mut pending: Vec<SourceEdge>,
) -> Result<()> {
    let mut captured =
        std::collections::BTreeMap::<(String, String), (Option<String>, bool)>::new();
    let mut visited = HashSet::new();
    while let Some((source_kind, source, version, access)) = pending.pop() {
        captured
            .entry((source_kind.clone(), source.clone()))
            .and_modify(|(previous_version, previous_access)| {
                if *previous_version != version {
                    *previous_version = None;
                }
                *previous_access |= access;
            })
            .or_insert((version, access));
        // Event and artifact observations have immutable edges. Pin the
        // ancestry of mutable records before their current edges can change.
        if source_kind != "record" || !visited.insert((source.clone(), access)) {
            continue;
        }
        if visited.len() > 1000 {
            return Err(Error::exhausted(
                "record ancestry exceeds the bounded source capture",
            ));
        }
        let unavailable: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM source_tombstones WHERE kind='record' AND id=?1)",
            [&source],
            |row| row.get(0),
        )?;
        if unavailable {
            continue;
        }
        let mut edges=db.prepare("SELECT source_kind,source_id,source_version,requires_access FROM source_edges WHERE subject_kind='record' AND subject_id=?1")?;
        let parents = edges
            .query_map([&source], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (parent_kind, parent, parent_version, parent_access) in parents {
            pending.push((parent_kind, parent, parent_version, access && parent_access));
        }
    }
    db.execute(
        "DELETE FROM source_edges WHERE subject_kind=?1 AND subject_id=?2",
        params![kind, id],
    )?;
    for ((source_kind, source), (version, access)) in captured {
        db.execute("INSERT INTO source_edges(subject_kind,subject_id,source_kind,source_id,source_version,requires_access) VALUES(?1,?2,?3,?4,?5,?6)",params![kind,id,source_kind,source,version,access])?;
        db.execute("UPDATE source_cleanup SET status='pending',error=NULL WHERE source_id IN (SELECT job_id FROM source_cleanup_items WHERE kind=?1 AND id=?2)",params![source_kind,source])?;
        db.execute("UPDATE source_cleanup_items SET complete=0,after_edge=0,payload_done=0 WHERE kind=?1 AND id=?2",params![source_kind,source])?;
    }
    Ok(())
}

/// Upgrade only ancestry still supported by retained revisions. The migration
/// tombstones ambiguous copies before following any of today's record edges.
pub(crate) fn upgrade_source_lineage(db: &Connection) -> Result<()> {
    let subjects=db.prepare("SELECT DISTINCT subject_kind,subject_id FROM source_edges e WHERE NOT EXISTS(SELECT 1 FROM source_tombstones t WHERE t.kind=e.subject_kind AND t.id=e.subject_id) AND NOT (subject_kind='artifact' AND subject_id IN (SELECT id FROM artifact_snapshots WHERE export_file IS NOT NULL))")?.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
    for (kind, id) in subjects {
        let roots=db.prepare("SELECT source_kind,source_id,source_version,requires_access FROM source_edges WHERE subject_kind=?1 AND subject_id=?2")?.query_map(params![kind,id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,bool>(3)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        capture_mutable_ancestry(db, &kind, &id, roots)?;
    }
    Ok(())
}

pub(crate) fn record_sources(db: &Connection, record: &RecordEnvelope) -> Result<()> {
    let mut references = record.provenance.source_refs.clone();
    crate::exports::collect_references(&Value::Object(record.body.clone()), &mut references);
    // Released aggregate decisions may depend on protected measurements without
    // granting their reader access to those measurements. Availability still applies.
    let mut released = Vec::new();
    if matches!(
        record.kind,
        RecordKind::Recommendation | RecordKind::Admission
    ) {
        for reference in &references {
            let aggregate: Option<bool> = db
                .query_row(
                    "SELECT kind IN ('evaluation','recommendation') FROM records WHERE id=?1",
                    [reference],
                    |r| r.get(0),
                )
                .optional()?;
            if aggregate.unwrap_or(false) {
                released.push(reference.clone());
            }
        }
    }
    // Preserve withdrawal ancestry after redaction, including for late derivatives.
    let has_edges: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM source_edges WHERE subject_kind='record' AND subject_id=?1)",
        [&record.id],
        |r| r.get(0),
    )?;
    if !record.retired || !has_edges {
        source_edges(db, "record", &record.id, &references, &released)?;
    }
    if record.retired {
        let deleted = record.body.is_empty();
        db.execute("INSERT INTO source_tombstones(kind,id,client,project,deleted) VALUES('record',?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET deleted=max(deleted,excluded.deleted)",params![record.id,record.scope.client,record.scope.project,deleted])?;
        db.execute("INSERT INTO source_policy(client,project,generation) VALUES(?1,?2,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1",params![record.scope.client,record.scope.project])?;
        if deleted {
            db.execute("UPDATE source_cleanup_items SET complete=0,payload_done=0 WHERE job_id=?1 AND EXISTS(SELECT 1 FROM source_cleanup WHERE source_id=?1 AND delete_content=0)",[&record.id])?;
        }
        db.execute("INSERT INTO source_cleanup(source_id,client,project,delete_content,status) VALUES(?1,?2,?3,?4,'pending') ON CONFLICT(source_id) DO UPDATE SET delete_content=max(delete_content,excluded.delete_content),status='pending',error=NULL",params![record.id,record.scope.client,record.scope.project,deleted])?;
    }
    Ok(())
}

pub(crate) fn migrate_sources(db: &Connection) -> Result<()> {
    let records = db
        .prepare("SELECT body FROM records")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for body in records {
        record_sources(db, &serde_json::from_str(&body)?)?;
    }
    let events = db
        .prepare("SELECT body FROM events")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for body in events {
        let event: Event = serde_json::from_str(&body)?;
        source_edges(db, "event", &event.id, &event.provenance.source_refs, &[])?;
    }
    Ok(())
}

impl Store {
    /// The host may deliver selected prepared records into an isolated study
    /// namespace. Ancestor access is checked separately from root delivery.
    pub(crate) fn evaluation_source_grant(
        &self,
        grant: &Grant,
        kind: &str,
        source: &str,
        root: bool,
    ) -> Result<Grant> {
        let mapping: Option<(String,bool)> = self.db.query_row(
            "SELECT owner_grant_id,deliverable FROM evaluation_sources WHERE grant_id=?1 AND kind=?2 AND source_id=?3",
            params![grant.id,kind,source], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        let mut effective = grant.clone();
        if let Some((owner, deliverable)) = mapping {
            if root && !deliverable {
                return Err(Error::denied(
                    "evaluation donor ancestry is not deliverable",
                ));
            }
            if kind == "record" {
                let frozen: Option<String> = self.db.query_row("SELECT json_extract(m.value,'$.version') FROM budget_allocations a JOIN budget_allocations p ON a.parent_id=p.id JOIN experiments e ON e.id=json_extract(p.body,'$.cause_id') JOIN json_each(e.policy,'$.memory_start') m WHERE a.grant_id=?1 AND json_extract(a.body,'$.purpose')='evaluation-subject' AND json_extract(m.value,'$.id')=?2",params![grant.id,source],|r|r.get(0)).optional()?;
                if let Some(version) = frozen {
                    let current: Option<String> = self
                        .db
                        .query_row("SELECT version FROM records WHERE id=?1", [source], |r| {
                            r.get(0)
                        })
                        .optional()?;
                    if current.as_ref() != Some(&version) {
                        return Err(Error::denied(
                            "starting memory changed after study selection",
                        ));
                    }
                }
            }
            effective.scope = self.grant(&owner)?.scope;
        }
        Ok(effective)
    }

    pub(crate) fn source_available(&self, grant: &Grant, kind: &str, id: &str) -> Result<bool> {
        match self.require_source(grant, kind, id) {
            Ok(()) => Ok(true),
            Err(error) if matches!(error.code, -32001 | -32004) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn require_source(&self, grant: &Grant, kind: &str, id: &str) -> Result<()> {
        self.require_sources(grant, [(kind.to_owned(), id.to_owned())])
    }

    /// Check shared ancestors once within this authorization snapshot. The
    /// visited set never survives into a later request or another grant.
    pub(crate) fn require_sources(
        &self,
        grant: &Grant,
        sources: impl IntoIterator<Item = (String, String)>,
    ) -> Result<()> {
        let sources: Vec<_> = sources.into_iter().collect();
        // Corpus selection controls delivered roots. Pinned definitions may
        // depend on donor evidence the curator cannot open; ordinary ancestry
        // availability and split checks still apply below.
        self.require_discovery_sources(grant, &sources)?;
        self.require_prepared_sources(grant, &sources)?;
        let roots: HashSet<_> = sources.iter().cloned().collect();
        let mut pending = sources
            .into_iter()
            .map(|(kind, id)| (kind, id, true))
            .collect::<Vec<_>>();
        let mut visited = HashSet::new();
        while let Some((kind, reference, access)) = pending.pop() {
            if !visited.insert((kind.clone(), reference.clone(), access)) {
                continue;
            }
            if visited.len() > 1000 {
                return Err(Error::exhausted(
                    "source lineage exceeds the local reference bound",
                ));
            }
            let unavailable = || {
                Error::denied(format!(
                    "source reference {reference:?} is absent or inaccessible"
                ))
            };
            let tombstone: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM source_tombstones WHERE kind=?1 AND id=?2)",
                params![kind, reference],
                |r| r.get(0),
            )?;
            if tombstone {
                return Err(unavailable());
            }
            let effective = self.evaluation_source_grant(
                grant,
                &kind,
                &reference,
                roots.contains(&(kind.clone(), reference.clone())),
            )?;
            let grant = &effective;
            match kind.as_str() {
                "artifact" => self.require_artifact_source(
                    grant,
                    &reference,
                    grant.prepared_run.is_some()
                        && !roots.contains(&(kind.clone(), reference.clone())),
                )?,
                "record" => {
                    let body: Option<String> = self
                        .db
                        .query_row(
                            "SELECT body FROM records WHERE id=?1 AND client=?2 AND project=?3",
                            params![reference, grant.scope.client, grant.scope.project],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let record: RecordEnvelope =
                        serde_json::from_str(&body.ok_or_else(unavailable)?)?;
                    if record.retired
                        || crate::records::expired(&record)?
                        || (access && !grant.visible_splits.contains(&record.provenance.split))
                    {
                        return Err(unavailable());
                    }
                }
                "event" => {
                    let body: Option<String> = self
                        .db
                        .query_row(
                            "SELECT body FROM events WHERE id=?1 AND client=?2 AND project=?3",
                            params![reference, grant.scope.client, grant.scope.project],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let event: Event = serde_json::from_str(&body.ok_or_else(unavailable)?)?;
                    if event.producer == "ribosome-host"
                        && event.payload.get("content_available")
                            != Some(&serde_json::Value::Bool(true))
                    {
                        return Err(unavailable());
                    }
                    if access && !grant.visible_splits.contains(&event.provenance.split) {
                        return Err(unavailable());
                    }
                }
                _ => return Err(unavailable()),
            }
            let mut edges=self.db.prepare("SELECT source_kind,source_id,requires_access FROM source_edges WHERE subject_kind=?1 AND subject_id=?2")?;
            for row in edges.query_map(params![kind, reference], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })? {
                let (kind, id, required) = row?;
                pending.push((kind, id, access && required));
            }
        }
        Ok(())
    }
}
