use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::validate,
};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

impl Store {
    /// Owner-side publication of one external episode. A source memory provides
    /// withdrawal through the existing source graph. No agent RPC exposes this.
    /// Snapshot files must be present in the host workspace before publication.
    pub fn import_external_episode(
        &self,
        grant: &Grant,
        source: &RecordEnvelope,
        events: &[Event],
        snapshots: &[ArtifactChunk],
    ) -> Result<usize> {
        validate("RecordEnvelope", &serde_json::to_value(source)?)?;
        validate("Memory", &serde_json::Value::Object(source.body.clone()))?;
        if source.scope != grant.scope
            || source.kind != RecordKind::Memory
            || source.version != "1"
            || source.retired
            || source.provenance.origin != Origin::Observed
            || !source.provenance.source_refs.is_empty()
            || !grant.visible_splits.contains(&source.provenance.split)
            || grant.discovery_corpus.is_some()
        {
            return Err(Error::denied(
                "external episode requires an owner-scoped observed source memory",
            ));
        }
        let memory: Memory =
            serde_json::from_value(serde_json::Value::Object(source.body.clone()))?;
        if !memory.evidence_refs.is_empty()
            || !memory.conflicts.is_empty()
            || !memory.supersedes.is_empty()
        {
            return Err(Error::invalid(
                "external source memory must be an independent source",
            ));
        }
        let bytes = serde_json::to_vec(&(source, events, snapshots))?.len();
        if events.is_empty() || events.len() > 4000 || bytes > 32 * 1024 * 1024 {
            return Err(Error::invalid(
                "external episode exceeds 4000 events or 32 MiB, or has no events",
            ));
        }
        let transaction = self.write_transaction()?;
        let previous: Option<String> = self
            .db
            .query_row("SELECT body FROM records WHERE id=?1", [&source.id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(previous) = previous {
            if serde_json::from_str::<RecordEnvelope>(&previous)? != *source {
                return Err(Error::conflict(
                    "external source identity already exists with different content or was withdrawn",
                ));
            }
            let retained: i64 = self.db.query_row("SELECT count(*) FROM source_edges WHERE subject_kind='event' AND source_kind='record' AND source_id=?1",[&source.id],|row|row.get(0))?;
            if retained != events.len() as i64 {
                return Err(Error::conflict(
                    "external episode identity has a different message count",
                ));
            }
        } else {
            self.save_record_for_run(source, None, None)?;
        }
        let mut snapshot_ids = HashSet::new();
        for chunk in snapshots {
            validate("ArtifactChunk", &serde_json::to_value(chunk)?)?;
            let id = chunk
                .snapshot_id
                .as_ref()
                .ok_or_else(|| Error::invalid("external snapshot requires an identity"))?;
            if !snapshot_ids.insert(id)
                || !grant.paths.contains(&chunk.artifact.path)
                || chunk.offset != 0
                || !chunk.eof
                || chunk.total_bytes != chunk.content.len().to_string()
                || chunk.required_freshness != Some(Freshness::Historical)
            {
                return Err(Error::invalid(
                    "external snapshot must be a complete, granted historical projection",
                ));
            }
            let body = serde_json::to_string(chunk)?;
            let previous: Option<(String, String, String)> = self
                .db
                .query_row(
                    "SELECT client,project,body FROM artifact_snapshots WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some(previous) = previous {
                if previous
                    != (
                        grant.scope.client.clone(),
                        grant.scope.project.clone(),
                        body,
                    )
                {
                    return Err(Error::conflict(
                        "external snapshot identity has different content",
                    ));
                }
            } else {
                self.db.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,available,body) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,'historical',1,?8)",params![id,grant.scope.client,grant.scope.project,grant.id,chunk.artifact.path,serde_json::to_value(&source.provenance.split)?.as_str(),chunk.artifact.version,body])?;
                crate::sources::source_edges(
                    &self.db,
                    "artifact",
                    id,
                    std::slice::from_ref(&source.id),
                    &[],
                )?;
            }
        }
        let mut inserted = 0;
        for event in events {
            if event.scope != grant.scope
                || event.kind != "external_message"
                || event.provenance.origin != Origin::Observed
                || event.provenance.split != source.provenance.split
                || event.provenance.source_refs != vec![source.id.clone()]
                || event.payload.get("authority").and_then(|v| v.as_str())
                    != Some("external_record")
                || event
                    .artifacts
                    .iter()
                    .any(|a| !snapshots.iter().any(|s| s.artifact == *a))
            {
                return Err(Error::invalid(
                    "external events must cite their source and projected snapshots with external_record authority",
                ));
            }
            inserted += usize::from(self.ingest(event)?);
        }
        transaction.commit()?;
        Ok(inserted)
    }
}
