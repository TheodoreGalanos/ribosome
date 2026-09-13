use crate::{
    contracts::*,
    error::{Error, Result},
    motif_records::{validate_frontier, within_frontier},
    store::Store,
    validation::{MAX_FRAME, validate},
};
use rusqlite::{OptionalExtension, params};
use std::collections::HashSet;

impl Store {
    pub(crate) fn require_corpus_event(&self, grant: &Grant, event: &str) -> Result<()> {
        if let Some(reference) = &grant.discovery_corpus
            && !self
                .discovery_corpus(grant, reference)?
                .source_windows
                .iter()
                .any(|window| window.event_refs.iter().any(|id| id == event))
        {
            return Err(Error::denied(
                "event is outside the host-assigned corpus windows",
            ));
        }
        Ok(())
    }
    /// Owner-only registration. There is deliberately no agent RPC for this.
    pub fn register_discovery_corpus(&self, grant: &Grant, corpus: &DiscoveryCorpus) -> Result<()> {
        validate("DiscoveryCorpus", &serde_json::to_value(corpus)?)?;
        let body = serde_json::to_string(corpus)?;
        if body.len() > MAX_FRAME / 4 {
            return Err(Error::invalid("discovery corpus exceeds 256 KiB"));
        }
        let tx = self.write_transaction()?;
        let old: Option<String> = tx.query_row("SELECT body FROM discovery_corpora WHERE client=?1 AND project=?2 AND id=?3 AND version=?4", params![grant.scope.client,grant.scope.project,corpus.id,corpus.version], |r| r.get(0)).optional()?;
        if let Some(old) = old {
            return if serde_json::from_str::<DiscoveryCorpus>(&old)? == *corpus {
                Ok(())
            } else {
                Err(Error::conflict(
                    "discovery corpus identity is immutable; register a new version",
                ))
            };
        }
        // Registration uses the owner's grant, before the corpus itself can
        // restrict a worker's delivery. It does not expand client/split access.
        let mut owner = grant.clone();
        owner.discovery_corpus = None;
        let mut events = HashSet::new();
        for window in &corpus.source_windows {
            validate_frontier(&window.frontier)?;
            for reference in &window.event_refs {
                let event = self.motif_event(&owner, reference)?;
                if event.run_id != window.execution {
                    return Err(Error::invalid(
                        "corpus window event belongs to another execution",
                    ));
                }
                within_frontier(&event, &window.frontier)?;
                events.insert(reference);
            }
        }
        if events.len() > 1000 {
            return Err(Error::invalid(
                "discovery corpus exceeds 1000 distinct events",
            ));
        }
        for reference in &corpus.definition_refs {
            self.motif_definition(&owner, reference)?;
        }
        for artifact in &corpus.artifacts {
            self.require_source(&owner, "artifact", &artifact.snapshot_id)?;
            let stored: (String, String, bool) = self.db.query_row(
                "SELECT path,version,version_only FROM artifact_snapshots WHERE id=?1",
                [&artifact.snapshot_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            if stored.0 != artifact.artifact.path
                || stored.1 != artifact.artifact.version
                || stored.2
            {
                return Err(Error::invalid(
                    "corpus artifact must identify a retained content snapshot at its declared version",
                ));
            }
        }
        let dependencies = self.dependencies(&grant.scope)?;
        for dependency in &corpus.dependencies {
            if !dependencies.contains(dependency)
                || !corpus
                    .artifacts
                    .iter()
                    .any(|a| a.artifact == dependency.source)
                || !corpus
                    .artifacts
                    .iter()
                    .any(|a| a.artifact == dependency.dependent)
            {
                return Err(Error::invalid(
                    "corpus dependency must match the stored edge and pinned artifacts",
                ));
            }
            for reference in &dependency.evidence_refs {
                self.require_reference(&owner, reference)?;
                if !events.contains(reference) {
                    return Err(Error::invalid(
                        "corpus dependency evidence must be selected in a source window",
                    ));
                }
            }
        }
        tx.execute(
            "INSERT INTO discovery_corpora(client,project,id,version,body) VALUES(?1,?2,?3,?4,?5)",
            params![
                grant.scope.client,
                grant.scope.project,
                corpus.id,
                corpus.version,
                body
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn discovery_corpus(
        &self,
        grant: &Grant,
        reference: &VersionRef,
    ) -> Result<DiscoveryCorpus> {
        let body: Option<String> = self.db.query_row("SELECT body FROM discovery_corpora WHERE client=?1 AND project=?2 AND id=?3 AND version=?4", params![grant.scope.client,grant.scope.project,reference.id,reference.version], |r| r.get(0)).optional()?;
        Ok(serde_json::from_str(&body.ok_or_else(|| {
            Error::missing("discovery corpus is not registered in this scope")
        })?)?)
    }

    pub(crate) fn discovery_grant(
        &self,
        grant: &Grant,
        requested: Option<&VersionRef>,
    ) -> Result<Grant> {
        if grant
            .discovery_corpus
            .as_ref()
            .zip(requested)
            .is_some_and(|(root, request)| root != request)
        {
            return Err(Error::denied(
                "run cannot replace the root grant's discovery corpus",
            ));
        }
        let mut effective = grant.clone();
        effective.discovery_corpus = requested
            .cloned()
            .or_else(|| grant.discovery_corpus.clone());
        if let Some(reference) = &effective.discovery_corpus {
            let corpus = self.discovery_corpus(grant, reference)?;
            effective.mode = Mode::Observe;
            effective.tools.clear();
            effective.writable_paths = Some(vec![]);
            effective
                .paths
                .retain(|path| corpus.artifacts.iter().any(|a| &a.artifact.path == path));
        }
        Ok(effective)
    }

    fn run_uses_discovery_corpus(&self, grant: &Grant, run: &str) -> Result<bool> {
        let reference = grant
            .discovery_corpus
            .as_ref()
            .ok_or_else(|| Error::internal("missing discovery authority"))?;
        self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs r JOIN grants g ON g.id=r.grant_id
             WHERE r.grant_id=?1 AND (r.id=?2 OR (NOT EXISTS(SELECT 1 FROM runs WHERE id=?2) AND r.id IN (SELECT json_extract(w.body,'$.parent_id') FROM work w WHERE w.id=?2 AND w.grant_id=?1)))
             AND coalesce(json_extract(r.request,'$.discovery_corpus.id'),json_extract(g.body,'$.discovery_corpus.id'))=?3
             AND coalesce(json_extract(r.request,'$.discovery_corpus.version'),json_extract(g.body,'$.discovery_corpus.version'))=?4)",
            params![grant.id,run,reference.id,reference.version], |r| r.get(0),
        ).map_err(Into::into)
    }

    pub(crate) fn require_discovery_sources(
        &self,
        grant: &Grant,
        sources: &[(String, String)],
    ) -> Result<()> {
        let Some(reference) = &grant.discovery_corpus else {
            return Ok(());
        };
        let corpus = self.discovery_corpus(grant, reference)?;
        for (kind, id) in sources {
            self.require_discovery_source_in(grant, &corpus, kind, id)?;
        }
        Ok(())
    }

    fn require_discovery_source_in(
        &self,
        grant: &Grant,
        corpus: &DiscoveryCorpus,
        kind: &str,
        id: &str,
    ) -> Result<()> {
        let allowed = match kind {
            "event" => {
                let selected = corpus
                    .source_windows
                    .iter()
                    .any(|w| w.event_refs.iter().any(|e| e == id));
                let run: Option<String> = self
                    .db
                    .query_row("SELECT run_id FROM events WHERE id=?1", [id], |r| r.get(0))
                    .optional()?;
                selected
                    || run
                        .as_deref()
                        .map(|run| self.run_uses_discovery_corpus(grant, run))
                        .transpose()?
                        .unwrap_or(false)
            }
            "record" => {
                let selected = corpus.definition_refs.iter().find(|r| r.id == id);
                let pinned = if let Some(selected) = selected {
                    let version: Option<String> = self.db.query_row("SELECT json_extract(body,'$.body.version') FROM records WHERE id=?1 AND kind='definition'", [id], |r| r.get(0)).optional()?.flatten();
                    version.as_ref() == Some(&selected.version)
                } else {
                    false
                };
                let mut statement = self.db.prepare("SELECT DISTINCT a.result_run_id FROM artifact_snapshots a,json_each(a.result_sources) s WHERE a.result_method='record.submit' AND json_extract(s.value,'$.kind')='record' AND json_extract(s.value,'$.id')=?1")?;
                let mut produced = false;
                for run in statement.query_map([id], |r| r.get::<_, String>(0))? {
                    if self.run_uses_discovery_corpus(grant, &run?)? {
                        produced = true;
                        break;
                    }
                }
                pinned || produced
            }
            "artifact" => {
                let row: Option<(String,String,Option<String>,bool)> = self.db.query_row("SELECT path,version,result_run_id,export_file IS NOT NULL FROM artifact_snapshots WHERE id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
                if let Some((path, version, run, export)) = row {
                    if corpus.artifacts.iter().any(|a| {
                        a.snapshot_id == id
                            && a.artifact.path == path
                            && a.artifact.version == version
                    }) {
                        true
                    } else if let Some(run) = run {
                        self.run_uses_discovery_corpus(grant, &run)?
                    } else {
                        !export
                            && corpus
                                .artifacts
                                .iter()
                                .any(|a| a.artifact.path == path && a.artifact.version == version)
                    }
                } else {
                    false
                }
            }
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(Error::denied(
                "source is outside the host-assigned discovery corpus",
            ))
        }
    }

    pub(crate) fn require_discovery_artifact(
        &self,
        grant: &Grant,
        artifact: &ArtifactRef,
    ) -> Result<()> {
        if let Some(reference) = &grant.discovery_corpus
            && !self
                .discovery_corpus(grant, reference)?
                .artifacts
                .iter()
                .any(|a| &a.artifact == artifact)
        {
            return Err(Error::denied(
                "artifact version is outside the host-assigned discovery corpus",
            ));
        }
        Ok(())
    }
}
