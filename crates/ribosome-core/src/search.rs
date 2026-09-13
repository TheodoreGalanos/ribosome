use crate::{
    contracts::*,
    error::{Error, Result},
    records::expired,
    store::Store,
    validation::validate,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    query: SearchRequest,
    grant: String,
    prepared_run: Option<String>,
    records_generation: i64,
    sources_generation: i64,
    id: String,
    rank: f64,
}

impl Store {
    pub fn search(&self, grant: &Grant, request: &SearchRequest) -> Result<RecordPage> {
        self.expire_memories(grant)?;
        validate("SearchRequest", &serde_json::to_value(request)?)?;
        let mut identity = request.clone();
        identity.after = None;
        identity.offset = 0;
        // The requested page size is not part of the match identity.
        identity.limit = 1;
        let ranked = request.order == Some(SearchRequestOrder::Relevance)
            && !request.query.trim().is_empty();
        let (mut records_generation, sources_generation): (i64, i64) = self.db.query_row(
            "SELECT coalesce((SELECT generation FROM record_index_generation WHERE client=?1 AND project=?2),0),coalesce((SELECT generation FROM source_policy WHERE client=?1 AND project=?2),0)",
            params![grant.scope.client,grant.scope.project], |r| Ok((r.get(0)?,r.get(1)?)))?;
        // BM25 uses corpus-wide document frequencies. Any index write can
        // change its scores, even when the changed record is not deliverable.
        if ranked {
            records_generation = self.db.query_row(
                "SELECT coalesce(sum(generation),0) FROM record_index_generation",
                [],
                |r| r.get(0),
            )?;
        }
        let cursor: Option<Cursor> = request
            .after
            .as_ref()
            .map(|cursor| {
                serde_json::from_str(cursor).map_err(|_| Error::invalid("invalid search cursor"))
            })
            .transpose()?;
        if let Some(cursor) = &cursor {
            if request.offset != 0
                || cursor.query != identity
                || cursor.grant != grant.id
                || cursor.prepared_run != grant.prepared_run
            {
                return Err(Error::invalid(
                    "search cursor belongs to a different query or grant",
                ));
            }
            if cursor.records_generation != records_generation
                || cursor.sources_generation != sources_generation
            {
                return Err(Error::conflict(
                    "search index or source availability changed; restart pagination",
                ));
            }
        }
        let separator = if request.query_mode == Some(SearchRequestQueryMode::AnyTerms) {
            " OR "
        } else {
            " AND "
        };
        let query = request
            .query
            .split_whitespace()
            .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(separator);
        let kind = request
            .kind
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?
            .and_then(|v| v.as_str().map(str::to_owned));
        // Eligibility and scope constrain the candidates before presentation.
        // IDs break lexical-score ties and serve as stable cursor keys.
        let source = if query.is_empty() {
            "SELECT r.id,r.body,0.0 AS score FROM records r WHERE r.client=?1 AND r.project=?2 AND (?3 IS NULL OR r.kind=?3) AND (?5 IS NULL OR json_extract(r.body,'$.body.function')=?5)"
        } else {
            "SELECT r.id,r.body,bm25(record_search) AS score FROM record_search JOIN records r ON r.id=record_search.id WHERE r.client=?1 AND r.project=?2 AND (?3 IS NULL OR r.kind=?3) AND record_search MATCH ?4 AND (?5 IS NULL OR json_extract(r.body,'$.body.function')=?5)"
        };
        let sql = if ranked {
            format!(
                "SELECT id,body,score FROM ({source}) WHERE (?6 IS NULL OR score>?7 OR (score=?7 AND id>?6)) ORDER BY score,id"
            )
        } else {
            format!("SELECT id,body,score FROM ({source}) WHERE (?6 IS NULL OR id>?6) ORDER BY id")
        };
        let mut statement = self.db.prepare(&sql)?;
        // Bind numeric parameters explicitly because an empty query leaves ?4
        // unused and ID ordering leaves ?7 unused.
        statement.raw_bind_parameter(1, &grant.scope.client)?;
        statement.raw_bind_parameter(2, &grant.scope.project)?;
        statement.raw_bind_parameter(3, kind)?;
        statement.raw_bind_parameter(4, &query)?;
        statement.raw_bind_parameter(5, &request.function)?;
        statement.raw_bind_parameter(6, cursor.as_ref().map(|c| &c.id))?;
        if ranked {
            statement.raw_bind_parameter(7, cursor.as_ref().map_or(0.0, |c| c.rank))?;
        }
        let mut rows = statement.raw_query();
        let mut records = Vec::new();
        let mut skipped = 0;
        let mut bytes = 0;
        let mut last = None;
        let mut complete = true;
        while let Some(row) = rows.next()? {
            let body: String = row.get(1)?;
            let record: RecordEnvelope = serde_json::from_str(&body)?;
            if record.retired
                || expired(&record)?
                || !grant.visible_splits.contains(&record.provenance.split)
                || !self.source_available(grant, "record", &record.id)?
            {
                continue;
            }
            if request.eligible == Some(true) && !self.implementation_eligible(grant, &record)? {
                continue;
            }
            if request.inventory == SearchRequestInventory::Usable
                && (record.kind != RecordKind::Implementation
                    || !self.is_admitted(grant, &record)?)
            {
                continue;
            }
            if skipped < request.offset {
                skipped += 1;
                continue;
            }
            if records.len() >= request.limit as usize
                || bytes + body.len() > crate::validation::MAX_FRAME / 2
            {
                complete = false;
                break;
            }
            last = Some((record.id.clone(), row.get::<_, f64>(2)?));
            bytes += body.len();
            records.push(record);
        }
        let next = if complete {
            None
        } else {
            last.map(|(id, rank)| {
                serde_json::to_string(&Cursor {
                    query: identity,
                    grant: grant.id.clone(),
                    prepared_run: grant.prepared_run.clone(),
                    records_generation,
                    sources_generation,
                    id,
                    rank,
                })
            })
            .transpose()?
        };
        Ok(RecordPage {
            next_offset: request.offset + records.len() as u32,
            records,
            next,
            complete: Some(complete),
        })
    }

    fn implementation_eligible(&self, grant: &Grant, record: &RecordEnvelope) -> Result<bool> {
        if record.kind != RecordKind::Implementation {
            return Ok(false);
        }
        let implementation: Implementation =
            serde_json::from_value(serde_json::Value::Object(record.body.clone()))?;
        Ok(implementation
            .required_capabilities
            .iter()
            .all(|tool| grant.tools.contains(tool))
            && (grant.mode != Mode::Observe || implementation.possible_effects.is_empty()))
    }
}
