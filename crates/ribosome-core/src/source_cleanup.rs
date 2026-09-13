use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, now_ms},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

const BATCH_WORK: usize = 100;

impl Store {
    /// Each selected job advances by at most 100 node/edge operations. A failed
    /// item rolls back; earlier items and the traversal frontier stay committed.
    pub fn cleanup_sources(&self, grant: &Grant, limit: u32) -> Result<()> {
        if !(1..=100).contains(&limit) {
            return Err(Error::invalid("cleanup limit must be 1..100"));
        }
        let jobs=self.db.prepare("SELECT source_id FROM source_cleanup WHERE client=?1 AND project=?2 AND status<>'complete' ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END,rowid LIMIT ?3")?.query_map(params![grant.scope.client,grant.scope.project,limit],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for job in jobs {
            if let Err(error) = self.advance_cleanup(grant, &job) {
                self.db.execute(
                    "UPDATE source_cleanup SET status='failed',error=?2 WHERE source_id=?1",
                    params![job, error.message],
                )?;
            }
        }
        Ok(())
    }

    fn advance_cleanup(&self, grant: &Grant, job: &str) -> Result<()> {
        let mut remaining = BATCH_WORK;
        while remaining > 0 {
            let tx = self.write_transaction()?;
            let (kind,delete):(String,bool)=tx.query_row("SELECT source_kind,delete_content FROM source_cleanup WHERE source_id=?1 AND client=?2 AND project=?3",params![job,grant.scope.client,grant.scope.project],|r|Ok((r.get(0)?,r.get(1)?)))?;
            tx.execute(
                "INSERT OR IGNORE INTO source_cleanup_items(job_id,kind,id) VALUES(?1,?2,?1)",
                params![job, kind],
            )?;
            let item:Option<(i64,String,String,i64,bool)>=tx.query_row("SELECT sequence,kind,id,after_edge,payload_done FROM source_cleanup_items WHERE job_id=?1 AND complete=0 ORDER BY sequence LIMIT 1",[job],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let Some((sequence, kind, source, after, payload_done)) = item else {
                tx.execute(
                    "UPDATE source_cleanup SET status='complete',error=NULL WHERE source_id=?1",
                    [job],
                )?;
                tx.commit()?;
                return Ok(());
            };
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE ?1='record' AND id=?2 AND client=?3 AND project=?4 UNION ALL SELECT 1 FROM events WHERE ?1='event' AND id=?2 AND client=?3 AND project=?4 UNION ALL SELECT 1 FROM artifact_snapshots WHERE ?1='artifact' AND id=?2 AND client=?3 AND project=?4)",params![kind,source,grant.scope.client,grant.scope.project],|r|r.get(0))?;
            remaining -= 1;
            if !owned {
                tx.execute(
                    "UPDATE source_cleanup_items SET complete=1 WHERE sequence=?1",
                    [sequence],
                )?;
                tx.commit()?;
                continue;
            }
            if !payload_done {
                if delete && source == job {
                    self.cleanup_legacy_exports(grant)?;
                    self.cleanup_legacy_effect_content(grant)?;
                    tx.execute("INSERT OR IGNORE INTO source_cleanup_items(job_id,kind,id) SELECT ?1,'event',id FROM events WHERE client=?2 AND project=?3 AND split='development' AND producer='ribosome-host' AND json_extract(body,'$.payload.content_available') IS NULL", params![job,grant.scope.client,grant.scope.project])?;
                }
                self.cleanup_source_payload(grant, &kind, &source, delete)?;
            }
            let children=self.db.prepare("SELECT rowid,subject_kind,subject_id FROM source_edges WHERE source_kind=?1 AND source_id=?2 AND rowid>?3 ORDER BY rowid LIMIT ?4")?.query_map(params![kind,source,after,remaining as i64],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
            let next = children.last().map_or(after, |(row, _, _)| *row);
            remaining -= children.len();
            for (_, child_kind, child) in children {
                tx.execute(
                    "INSERT OR IGNORE INTO source_cleanup_items(job_id,kind,id) VALUES(?1,?2,?3)",
                    params![job, child_kind, child],
                )?;
            }
            let more:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_edges WHERE source_kind=?1 AND source_id=?2 AND rowid>?3)",params![kind,source,next],|r|r.get(0))?;
            tx.execute("UPDATE source_cleanup_items SET after_edge=?2,payload_done=1,complete=?3 WHERE sequence=?1",params![sequence,next,!more])?;
            tx.execute("UPDATE source_cleanup SET status=CASE WHEN EXISTS(SELECT 1 FROM source_cleanup_items WHERE job_id=?1 AND complete=0) THEN 'pending' ELSE 'complete' END,error=NULL WHERE source_id=?1",[job])?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Called only inside the transaction that advances this node's cursor.
    fn cleanup_source_payload(
        &self,
        grant: &Grant,
        kind: &str,
        source: &str,
        delete: bool,
    ) -> Result<()> {
        self.db.execute(
            "DELETE FROM archive WHERE implementation_id=?1 AND client=?2 AND project=?3",
            params![source, grant.scope.client, grant.scope.project],
        )?;
        if delete {
            self.redact_context_source(grant, kind, source)?;
        }
        if kind == "artifact" {
            self.remove_export_file(source)?;
        }
        match kind {
            "record" => {
                let body: String =
                    self.db
                        .query_row("SELECT body FROM records WHERE id=?1", [source], |r| {
                            r.get(0)
                        })?;
                let mut record: RecordEnvelope = serde_json::from_str(&body)?;
                if record.kind == RecordKind::Admission
                    || (record.kind == RecordKind::Evaluation
                        && record.provenance.split != Split::Development)
                {
                    return Ok(());
                }
                if delete && record.kind == RecordKind::Experiment {
                    self.cleanup_experiment_files(grant, source)?;
                    // Preserve protected study evidence under its retention
                    // policy; development inputs are ordinary copied content.
                    self.db.execute("UPDATE experiments SET policy=json_object('policy',json_extract(policy,'$.policy'),'source_deleted',json('true'),'workspace_cleanup_confirmed',coalesce(json_extract(policy,'$.workspace_cleanup_confirmed'),0)) WHERE id=?1 AND json_type(policy,'$.cases')='array' AND json_array_length(policy,'$.cases')>0 AND NOT EXISTS(SELECT 1 FROM json_each(policy,'$.cases') WHERE json_extract(value,'$.split') IS NOT 'development')", [source])?;
                }
                if !record.retired || (delete && !record.body.is_empty()) {
                    record.version = (counter(&record.version)? + 1).to_string();
                    record.retired = true;
                    record.updated_ms = now_ms().to_string();
                    if delete {
                        record.body.clear();
                        record.provenance.source_refs.clear();
                    }
                    self.db.execute(
                        "UPDATE records SET body=?2,version=?3 WHERE id=?1",
                        params![source, serde_json::to_string(&record)?, record.version],
                    )?;
                }
                self.db
                    .execute("DELETE FROM record_search WHERE id=?1", [source])?;
                self.cleanup_tombstone(grant, kind, source, delete)?;
            }
            "event" if delete => {
                let body: String =
                    self.db
                        .query_row("SELECT body FROM events WHERE id=?1", [source], |r| {
                            r.get(0)
                        })?;
                let mut event: Event = serde_json::from_str(&body)?;
                if event.provenance.split == Split::Development {
                    self.cleanup_effect_content(source)?;
                    event.payload = json!({"redacted":true,"reason":"source deleted"})
                        .as_object()
                        .unwrap()
                        .clone();
                    self.db.execute(
                        "UPDATE events SET body=?2 WHERE id=?1",
                        params![source, serde_json::to_string(&event)?],
                    )?;
                    self.cleanup_tombstone(grant, kind, source, true)?;
                    self.db.execute("UPDATE messages SET body='{}',acknowledged=1 WHERE id=?1 AND source_format=1 AND grant_id IN (SELECT id FROM grants WHERE json_extract(body,'$.scope.client')=?2 AND json_extract(body,'$.scope.project')=?3)",params![source,grant.scope.client,grant.scope.project])?;
                    self.db.execute("UPDATE attachment_feedback SET body=json_set(body,'$.summary','','$.detail','Source deleted; feedback withdrawn.','$.state','expired') WHERE json_extract(body,'$.run_id') IN (SELECT id FROM runs WHERE result_source=?1 AND grant_id IN (SELECT id FROM grants WHERE json_extract(body,'$.scope.client')=?2 AND json_extract(body,'$.scope.project')=?3))",params![source,grant.scope.client,grant.scope.project])?;
                    self.db.execute("UPDATE runs SET result=NULL WHERE result_source=?1 AND grant_id IN (SELECT id FROM grants WHERE json_extract(body,'$.scope.client')=?2 AND json_extract(body,'$.scope.project')=?3)",params![source,grant.scope.client,grant.scope.project])?;
                }
            }
            "artifact" if delete => {
                self.db.execute(
                    "UPDATE artifact_snapshots SET body='{}',result_content=NULL WHERE id=?1 AND split='development'",
                    [source],
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn cleanup_tombstone(
        &self,
        grant: &Grant,
        kind: &str,
        source: &str,
        delete: bool,
    ) -> Result<()> {
        let previous: Option<bool> = self
            .db
            .query_row(
                "SELECT deleted FROM source_tombstones WHERE kind=?1 AND id=?2",
                params![kind, source],
                |r| r.get(0),
            )
            .optional()?;
        self.db.execute("INSERT INTO source_tombstones(kind,id,client,project,deleted) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(kind,id) DO UPDATE SET deleted=max(deleted,excluded.deleted)",params![kind,source,grant.scope.client,grant.scope.project,delete])?;
        if previous.is_none() || (delete && previous == Some(false)) {
            self.db.execute("INSERT INTO source_policy(client,project,generation) VALUES(?1,?2,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1",params![grant.scope.client,grant.scope.project])?;
        }
        Ok(())
    }

    pub fn source_cleanup_status(&self, grant: &Grant) -> Result<Value> {
        self.source_cleanup_status_page(grant, "0", 100)
    }

    pub fn source_cleanup_status_page(
        &self,
        grant: &Grant,
        after: &str,
        limit: u32,
    ) -> Result<Value> {
        if !(1..=100).contains(&limit) {
            return Err(Error::invalid("cleanup status limit must be 1..100"));
        }
        let after = i64::try_from(counter(after)?)
            .map_err(|_| Error::invalid("cleanup cursor exceeds SQLite integer capacity"))?;
        let tx = self.db.unchecked_transaction()?;
        let mut rows=tx.prepare("SELECT rowid,source_id,source_kind,delete_content,status,error,(SELECT count(*) FROM source_cleanup_items WHERE job_id=c.source_id AND complete=1),(SELECT count(*) FROM source_cleanup_items WHERE job_id=c.source_id AND complete=0) FROM source_cleanup c WHERE client=?1 AND project=?2 AND rowid>?3 ORDER BY rowid LIMIT ?4")?.query_map(params![grant.scope.client,grant.scope.project,after,limit+1],|r|Ok((r.get::<_,i64>(0)?,json!({"source_id":r.get::<_,String>(1)?,"source_kind":r.get::<_,String>(2)?,"delete_content":r.get::<_,bool>(3)?,"status":r.get::<_,String>(4)?,"error":r.get::<_,Option<String>>(5)?,"processed":r.get::<_,i64>(6)?.to_string(),"queued":r.get::<_,i64>(7)?.to_string()}))))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let complete = rows.len() <= limit as usize;
        rows.truncate(limit as usize);
        let next = rows.last().map_or(after, |(cursor, _)| *cursor).to_string();
        let generation: i64 = tx
            .query_row(
                "SELECT generation FROM source_policy WHERE client=?1 AND project=?2",
                params![grant.scope.client, grant.scope.project],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(
            json!({"generation":generation.to_string(),"jobs":rows.into_iter().map(|(_,entry)|entry).collect::<Vec<_>>(),"next":next,"complete":complete}),
        )
    }
}
