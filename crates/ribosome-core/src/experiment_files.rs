use crate::{
    contracts::Grant,
    effects::Runtime,
    error::{Error, Result},
    store::Store,
};
use rusqlite::{OptionalExtension, params};
use std::path::PathBuf;

impl Store {
    pub(crate) fn cleanup_experiment_files(&self, grant: &Grant, experiment: &str) -> Result<()> {
        let saved: Option<(Option<String>, bool, bool)> = self.db.query_row(
            "SELECT json_extract(e.policy,'$.workspace.path'),e.result IS NULL AND coalesce(json_extract(e.policy,'$.workspace.executor_stopped'),0)=0,coalesce(json_extract(e.policy,'$.workspace_cleanup_confirmed'),0) FROM experiments e JOIN grants g ON g.id=e.grant_id WHERE e.id=?1 AND json_extract(g.body,'$.scope.client')=?2 AND json_extract(g.body,'$.scope.project')=?3",
            params![experiment,grant.scope.client,grant.scope.project], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional()?;
        let Some((path, active, cleanup_confirmed)) = saved else {
            return Ok(());
        };
        let Some(path) = path else {
            return if active && !cleanup_confirmed {
                Err(Error::conflict(
                    "legacy evaluator workspace location was not recorded; the owner must confirm executors stopped and copied files were removed",
                ))
            } else {
                Ok(())
            };
        };
        if active {
            return Err(Error::conflict(
                "evaluator still owns its workspace; the owner must confirm its executor stopped before cleanup",
            ));
        }
        let root = self
            .export_directory
            .as_ref()
            .and_then(|path| path.parent())
            .ok_or_else(|| {
                Error::internal(
                    "experiment file cleanup requires a Runtime bound to its state directory",
                )
            })?;
        let path = PathBuf::from(path);
        if root.canonicalize()? != root
            || path.parent() != Some(root)
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("ribosome-study-"))
        {
            return Err(Error::denied(
                "experiment workspace is outside the managed state directory",
            ));
        }
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path)?,
            Ok(_) => {
                return Err(Error::denied(
                    "experiment workspace is no longer a directory",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.db.execute("UPDATE experiments SET policy=json_set(policy,'$.workspace_cleanup_confirmed',json('true')) WHERE id=?1",[experiment])?;
        Ok(())
    }

    pub(crate) fn recover_experiment_files(&self) -> Result<()> {
        let rows = self.db.prepare("SELECT id,grant_id FROM experiments WHERE json_extract(policy,'$.workspace.path') IS NOT NULL AND (result IS NOT NULL OR json_extract(policy,'$.workspace.executor_stopped')=1)")?
            .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?
            .collect::<std::result::Result<Vec<_>,_>>()?;
        for (experiment, owner) in rows {
            let grant = self.grant(&owner)?;
            self.cleanup_experiment_files(&grant, &experiment)?;
            self.cleanup_sources(&grant, 100)?;
        }
        Ok(())
    }
}

impl Runtime {
    /// Owner attestation for an interrupted legacy study with no recorded
    /// workspace. The owner must stop its executors and remove their copied
    /// inputs first. This method cannot locate or verify those external files.
    pub fn confirm_legacy_experiment_cleanup(&self, owner: &Grant, experiment: &str) -> Result<()> {
        if self.store.grant(&owner.id)? != *owner {
            return Err(Error::denied(
                "legacy cleanup requires the stored owner grant",
            ));
        }
        let changed = self.store.db.execute("UPDATE experiments SET policy=json_set(policy,'$.workspace_cleanup_confirmed',json('true')) WHERE id=?1 AND grant_id=?2 AND result IS NULL AND json_extract(policy,'$.workspace.path') IS NULL",params![experiment,owner.id])?;
        if changed == 0 {
            return Err(Error::missing(
                "unregistered interrupted experiment not found in owner grant",
            ));
        }
        self.store.cleanup_sources(owner, 100)
    }

    /// Owner-only cleanup after an interrupted evaluation. The caller must
    /// establish that the evaluator and every process it started have stopped.
    /// This does not supply an evaluation result or release unknown model usage.
    pub fn settle_experiment_workspace(
        &self,
        owner: &Grant,
        experiment: &str,
        executor_stopped: bool,
    ) -> Result<()> {
        if !executor_stopped || self.store.grant(&owner.id)? != *owner {
            return Err(Error::denied(
                "workspace cleanup requires the stored owner grant and confirmation that the executor stopped",
            ));
        }
        let tx = self.store.write_transaction()?;
        let running: Option<bool> = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id=json_extract(e.policy,'$.workspace.run_id') AND status IN ('running','waiting')) FROM experiments e WHERE id=?1 AND grant_id=?2 AND json_extract(policy,'$.workspace.path') IS NOT NULL",
            params![experiment,owner.id], |row| row.get(0),
        ).optional()?;
        if running.ok_or_else(|| Error::missing("experiment workspace not found in owner grant"))? {
            return Err(Error::conflict(
                "stop the owning run before confirming evaluator workspace cleanup",
            ));
        }
        tx.execute("UPDATE experiments SET policy=json_set(policy,'$.workspace.executor_stopped',json('true')) WHERE id=?1", [experiment])?;
        tx.commit()?;
        self.store.cleanup_experiment_files(owner, experiment)?;
        self.store.cleanup_sources(owner, 100)
    }
}
