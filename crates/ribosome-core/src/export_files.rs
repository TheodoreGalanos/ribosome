use crate::{
    contracts::*,
    error::{Error, Result},
    host::hash,
    store::Store,
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use std::{io::Read, path::PathBuf};

pub(crate) const EXPORT_PREFIX: &str = "ribosome-export:";
pub(crate) const MAX_EXPORT_BYTES: usize = crate::host::MAX_ARTIFACT as usize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyExportRow {
    product: Origin,
    scope: Scope,
    record: RecordEnvelope,
}

impl Store {
    /// Old exports have no recorded source closure. On source deletion, remove
    /// recognizable legacy products in that scope instead of inventing lineage.
    pub(crate) fn cleanup_legacy_exports(&self, grant: &Grant) -> Result<()> {
        let Some(directory) = &self.export_directory else {
            return Ok(());
        };
        if directory.canonicalize()? != *directory {
            return Err(Error::denied("managed exports directory changed"));
        }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let Some(snapshot) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
                || uuid::Uuid::parse_str(snapshot).is_err()
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let registered: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM artifact_snapshots WHERE export_file=?1)",
                [entry.file_name().to_string_lossy().as_ref()],
                |row| row.get(0),
            )?;
            if registered {
                continue;
            }
            let file = std::fs::File::open(&path)?;
            let mut bytes = Vec::new();
            file.take(MAX_EXPORT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_EXPORT_BYTES {
                return Err(Error::exhausted(
                    "unregistered export exceeds 16 MiB; owner inspection is required before cleanup",
                ));
            }
            let mut rows = 0;
            let mut scoped = true;
            for row in serde_json::Deserializer::from_slice(&bytes).into_iter::<LegacyExportRow>() {
                match row {
                    Ok(row)
                        if row.scope == grant.scope
                            && row.record.scope == grant.scope
                            && row.record.provenance.split == Split::Development
                            && row.product == row.record.provenance.origin =>
                    {
                        rows += 1
                    }
                    _ => {
                        scoped = false;
                        break;
                    }
                }
            }
            if scoped && rows > 0 {
                std::fs::remove_file(path)?;
                std::fs::File::open(directory)?.sync_all()?;
            }
        }
        Ok(())
    }

    pub(crate) fn export_path(&self, snapshot: &str, filename: &str) -> Result<PathBuf> {
        if uuid::Uuid::parse_str(snapshot).is_err() || filename != format!("{snapshot}.jsonl") {
            return Err(Error::denied("invalid managed export filename"));
        }
        let directory = self.export_directory.as_ref().ok_or_else(|| {
            Error::internal("export file cleanup requires a Runtime bound to its state directory")
        })?;
        if directory.canonicalize()? != *directory {
            return Err(Error::denied("managed exports directory changed"));
        }
        Ok(directory.join(filename))
    }

    /// File deletion may precede a failed SQLite commit. Retrying a missing
    /// file is safe; the already committed source tombstone still denies reads.
    pub(crate) fn remove_export_file(&self, snapshot: &str) -> Result<()> {
        let filename: Option<String> = self
            .db
            .query_row(
                "SELECT export_file FROM artifact_snapshots WHERE id=?1 AND split='development'",
                [snapshot],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if let Some(filename) = filename {
            let path = self.export_path(snapshot, &filename)?;
            match std::fs::remove_file(&path) {
                Ok(()) => std::fs::File::open(path.parent().unwrap())?.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            self.db.execute(
                "UPDATE artifact_snapshots SET available=0 WHERE id=?1",
                [snapshot],
            )?;
        }
        Ok(())
    }

    pub(crate) fn withdraw_export(&self, snapshot: &str) -> Result<()> {
        let tx = self.write_transaction()?;
        let changed = tx.execute("INSERT OR IGNORE INTO source_tombstones(kind,id,client,project,deleted) SELECT 'artifact',id,client,project,1 FROM artifact_snapshots WHERE id=?1 AND export_file IS NOT NULL",[snapshot])?;
        tx.execute(
            "UPDATE artifact_snapshots SET available=0 WHERE id=?1 AND export_file IS NOT NULL",
            [snapshot],
        )?;
        tx.execute("INSERT OR IGNORE INTO source_cleanup(source_id,client,project,delete_content,status,source_kind) SELECT id,client,project,1,'pending','artifact' FROM artifact_snapshots WHERE id=?1 AND export_file IS NOT NULL",[snapshot])?;
        if changed > 0 {
            tx.execute("INSERT INTO source_policy(client,project,generation) SELECT client,project,1 FROM artifact_snapshots WHERE id=?1 ON CONFLICT(client,project) DO UPDATE SET generation=generation+1",[snapshot])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn recover_export_files(&self) -> Result<()> {
        let rows = self.db.prepare("SELECT id,grant_id,available FROM artifact_snapshots WHERE export_file IS NOT NULL")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut owners = std::collections::HashSet::new();
        for (snapshot, owner, available) in rows {
            // A manifest commits before its file is created. A crash cannot
            // turn a partial or uncommitted file into a ready training product.
            if !available {
                self.withdraw_export(&snapshot)?;
            }
            owners.insert(owner);
        }
        for owner in owners {
            let grant = self.grant(&owner)?;
            self.refresh_export_files(&grant)?;
            self.cleanup_sources(&grant, 100)?;
        }
        Ok(())
    }

    pub(crate) fn refresh_export_files(&self, grant: &Grant) -> Result<()> {
        let rows=self.db.prepare("SELECT id,export_file FROM artifact_snapshots WHERE client=?1 AND project=?2 AND available=1 AND export_file IS NOT NULL")?.query_map(params![grant.scope.client,grant.scope.project],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for (snapshot, filename) in rows {
            let path = self.export_path(&snapshot, &filename)?;
            let present = match std::fs::symlink_metadata(path) {
                Ok(metadata) => metadata.is_file(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
            if !present {
                self.withdraw_export(&snapshot)?;
            }
        }
        Ok(())
    }

    pub(crate) fn read_export_artifact(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
    ) -> Result<ArtifactChunk> {
        if request.branch_id.is_some() || request.required_freshness == Some(Freshness::Current) {
            return Err(Error::conflict(
                "training exports are immutable historical products",
            ));
        }
        let snapshot = request
            .path
            .strip_prefix(EXPORT_PREFIX)
            .ok_or_else(|| Error::invalid("invalid export reference"))?;
        if request
            .snapshot_id
            .as_deref()
            .is_some_and(|id| id != snapshot)
        {
            return Err(Error::denied("snapshot does not match export reference"));
        }
        let _tx = self.db.unchecked_transaction()?;
        self.require_source(grant, "artifact", snapshot)?;
        let row: Option<(String,String)> = self.db.query_row("SELECT export_file,version FROM artifact_snapshots WHERE id=?1 AND export_file IS NOT NULL",[snapshot],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (filename, version) =
            row.ok_or_else(|| Error::denied("reference is not a training export"))?;
        let path = self.export_path(snapshot, &filename)?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > MAX_EXPORT_BYTES as u64 {
            return Err(Error::denied(
                "managed export is not a bounded regular file",
            ));
        }
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_EXPORT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_EXPORT_BYTES || hash(&bytes) != version {
            return Err(Error::denied(
                "managed export content differs from its recorded version",
            ));
        }
        let content =
            String::from_utf8(bytes).map_err(|_| Error::invalid("export content is not UTF-8"))?;
        let text = crate::tool_results::utf8_window(
            &content,
            request.offset as usize,
            (request.length as usize).min(8192),
        )?;
        Ok(ArtifactChunk {
            artifact: ArtifactRef {
                path: request.path.clone(),
                version,
            },
            content: text.into(),
            offset: request.offset,
            total_bytes: content.len().to_string(),
            eof: request.offset as usize + text.len() == content.len(),
            snapshot_id: Some(snapshot.into()),
            required_freshness: Some(Freshness::Historical),
        })
    }
}
