use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    store::Store,
    validation::{derived_split, id},
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

struct ArtifactAccess {
    path: String,
    branch: Option<String>,
    owner: String,
    split: String,
    available: bool,
    tool_result: bool,
    export: bool,
}

impl Runtime {
    /// Refresh filesystem availability at the host boundary. Grant-specific
    /// access is checked separately; a narrow grant cannot revoke another one.
    pub(crate) fn refresh_artifact_sources(&self, grant: &Grant) -> Result<()> {
        let locations = self.store.db.prepare("SELECT DISTINCT path,branch_id,grant_id FROM artifact_snapshots WHERE client=?1 AND project=?2 AND available=1 AND result_run_id IS NULL AND export_file IS NULL")?.query_map(params![grant.scope.client,grant.scope.project], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for (path, branch, owner) in locations {
            if !grant.paths.contains(&path) || (branch.is_some() && owner != grant.id) {
                continue;
            }
            let workspace = self.branch_path(grant, branch.as_deref())?;
            let observed = self.host.version(grant, &path, workspace.as_deref())?;
            let tx = self.store.write_transaction()?;
            if observed.version == "absent" {
                tx.execute("INSERT OR IGNORE INTO source_tombstones(kind,id,client,project,deleted) SELECT 'artifact',id,client,project,1 FROM artifact_snapshots WHERE client=?1 AND project=?2 AND path=?3 AND branch_id IS ?4 AND available=1", params![grant.scope.client,grant.scope.project,path,branch])?;
                tx.execute("INSERT OR IGNORE INTO source_cleanup(source_id,client,project,delete_content,status,source_kind) SELECT id,client,project,1,'pending','artifact' FROM artifact_snapshots WHERE client=?1 AND project=?2 AND path=?3 AND branch_id IS ?4 AND available=1", params![grant.scope.client,grant.scope.project,path,branch])?;
                tx.execute("INSERT INTO source_policy(client,project,generation) VALUES(?1,?2,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1",params![grant.scope.client,grant.scope.project])?;
            }
            self.store.db.execute("UPDATE artifact_snapshots SET current_version=?1,available=CASE WHEN ?1='absent' THEN 0 ELSE available END WHERE client=?2 AND project=?3 AND path=?4 AND branch_id IS ?5", params![observed.version,grant.scope.client,grant.scope.project,path,branch])?;
            tx.commit()?;
        }
        self.store.refresh_export_files(grant)?;
        self.store.cleanup_sources(grant, 20)?;
        Ok(())
    }

    pub(crate) fn read_artifact_context(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
    ) -> Result<ArtifactChunk> {
        let required_freshness = request.required_freshness.clone().unwrap_or_else(|| {
            if grant.discovery_corpus.is_some() {
                Freshness::Historical
            } else {
                Freshness::Current
            }
        });
        if grant.discovery_corpus.is_some()
            && (request.snapshot_id.is_none() || required_freshness != Freshness::Historical)
        {
            return Err(Error::denied(
                "assigned discovery requires an explicit historical artifact snapshot",
            ));
        }
        let mut chunk = if let Some(snapshot) = &request.snapshot_id {
            self.store.require_source(grant, "artifact", snapshot)?;
            let version_only: bool = self.store.db.query_row(
                "SELECT version_only FROM artifact_snapshots WHERE id=?1",
                [snapshot],
                |row| row.get(0),
            )?;
            if version_only {
                return Err(Error::invalid(
                    "snapshot records an artifact version only; request a fresh artifact read for content",
                ));
            }
            let (body, branch, current, result): (String, Option<String>, String, bool) =
                self.store.db.query_row(
                    "SELECT body,branch_id,current_version,(result_run_id IS NOT NULL OR export_file IS NOT NULL) FROM artifact_snapshots WHERE id=?1",
                    [snapshot],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?;
            if result {
                return Err(Error::denied(
                    "retained result or export requires its own managed artifact path",
                ));
            }
            let mut chunk: ArtifactChunk = serde_json::from_str(&body)?;
            if chunk.artifact.path != request.path || branch != request.branch_id {
                return Err(Error::denied(
                    "snapshot path or branch does not match the read",
                ));
            }
            if required_freshness == Freshness::Current && current != chunk.artifact.version {
                return Err(Error::conflict(
                    "snapshot is historical; reread the current artifact before acting",
                ));
            }
            let start = request.offset.checked_sub(chunk.offset).ok_or_else(|| {
                Error::invalid("requested bytes precede the retained snapshot window")
            })? as usize;
            let mut end = (start + request.length as usize).min(chunk.content.len());
            if start > chunk.content.len() || (start == chunk.content.len() && !chunk.eof) {
                return Err(Error::invalid(
                    "requested bytes exceed the retained snapshot window; read the current artifact for other bytes",
                ));
            }
            if !chunk.content.is_char_boundary(start) {
                return Err(Error::invalid("snapshot offset splits a UTF-8 character"));
            }
            while !chunk.content.is_char_boundary(end) {
                end -= 1;
            }
            if start == end && start < chunk.content.len() {
                return Err(Error::invalid(
                    "snapshot length is smaller than the next UTF-8 character",
                ));
            }
            let text = chunk
                .content
                .get(start..end)
                .ok_or_else(|| {
                    Error::invalid("snapshot byte offsets must align with UTF-8 characters")
                })?
                .to_owned();
            chunk.eof = chunk.eof && end == chunk.content.len();
            chunk.content = text;
            chunk.offset = request.offset;
            chunk
        } else {
            let branch = self.branch_path(grant, request.branch_id.as_deref())?;
            self.host.read(grant, request, branch.as_deref())?
        };
        self.store
            .require_discovery_artifact(grant, &chunk.artifact)?;
        let snapshot = id();
        chunk.snapshot_id = Some(snapshot.clone());
        chunk.required_freshness = Some(required_freshness.clone());
        let current = self.host.version(
            grant,
            &request.path,
            self.branch_path(grant, request.branch_id.as_deref())?
                .as_deref(),
        )?;
        let transaction = self
            .store
            .db
            .is_autocommit()
            .then(|| self.store.write_transaction())
            .transpose()?;
        self.store.db.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,branch_id,split,version,current_version,required_freshness,available,body) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![snapshot,grant.scope.client,grant.scope.project,grant.id,request.path,request.branch_id,serde_json::to_value(derived_split(&grant.visible_splits))?.as_str(),chunk.artifact.version,current.version,serde_json::to_value(required_freshness)?.as_str(),current.version!="absent",serde_json::to_string(&chunk)?])?;
        if let Some(parent) = &request.snapshot_id {
            crate::sources::source_edges(
                &self.store.db,
                "artifact",
                &snapshot,
                std::slice::from_ref(parent),
                &[],
            )?;
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(chunk)
    }
}

impl Store {
    pub(crate) fn require_artifact_source(
        &self,
        grant: &Grant,
        source: &str,
        lineage_only: bool,
    ) -> Result<()> {
        let row = self.db.query_row("SELECT path,branch_id,grant_id,split,available,result_run_id IS NOT NULL,export_file IS NOT NULL FROM artifact_snapshots WHERE id=?1 AND client=?2 AND project=?3",params![source,grant.scope.client,grant.scope.project],|r|Ok(ArtifactAccess{path:r.get(0)?,branch:r.get(1)?,owner:r.get(2)?,split:r.get(3)?,available:r.get(4)?,tool_result:r.get(5)?,export:r.get(6)?})).optional()?;
        let unavailable = || Error::denied("artifact observation is absent or inaccessible");
        let access = row.ok_or_else(unavailable)?;
        let split: Split = serde_json::from_value(Value::String(access.split))?;
        if !access.available
            || (!lineage_only
                && !access.tool_result
                && !access.export
                && !grant.paths.contains(&access.path))
            || (!lineage_only && access.export && !grant.allow_export)
            || !grant.visible_splits.contains(&split)
            || (!lineage_only && access.branch.is_some() && access.owner != grant.id)
        {
            return Err(unavailable());
        }
        Ok(())
    }

    pub(crate) fn context_is_current(&self, segment: &str) -> Result<bool> {
        let stale: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM context_sources s JOIN artifact_snapshots a ON s.kind='artifact' AND s.id=a.id WHERE s.segment_id=?1 AND a.required_freshness='current' AND a.version<>a.current_version)",[segment],|r|r.get(0))?;
        Ok(!stale)
    }
}
