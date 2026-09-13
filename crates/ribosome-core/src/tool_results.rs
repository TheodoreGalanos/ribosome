use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    host::hash,
    store::Store,
    validation::{derived_split, id, validate},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

pub(crate) const RESULT_PREFIX: &str = "ribosome-result:";
const INLINE_BYTES: usize = 32 * 1024;

impl Runtime {
    pub(crate) fn observe_tool(&self, run: &str, request: &ToolCall) -> Result<ToolObservation> {
        validate("ToolCall", &serde_json::to_value(request)?)?;
        let grant = self.store.require_active(run)?;
        let method = serde_json::to_value(&request.method)?
            .as_str()
            .unwrap()
            .to_owned();
        let args = Value::Object(request.arguments.clone());
        let args_hash = hash(serde_json::to_string(&args)?.as_bytes());
        let saved:Option<(String,String)>=self.store.db.query_row("SELECT result_method,result_args_hash FROM artifact_snapshots WHERE result_run_id=?1 AND result_call_id=?2",params![run,request.call_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((previous_method, previous_args)) = saved {
            if previous_method != method || previous_args != args_hash {
                return Err(Error::conflict(
                    "tool call identity reused with different arguments",
                ));
            }
            return self.retained_tool(run, &request.call_id);
        }
        // Effects still use the existing dispatcher and its durable operation IDs.
        let result = self.tool_call(run, &method, args)?;
        let content = serde_json::to_string(&result)?;
        let sources = self
            .store
            .tool_output_sources(&method, &request.arguments, &result)?;
        let snapshot = id();
        let version = hash(content.as_bytes());
        let path = format!("{RESULT_PREFIX}{snapshot}");
        let tx = self.store.write_transaction()?;
        tx.execute("INSERT INTO artifact_snapshots(id,client,project,grant_id,path,split,version,current_version,required_freshness,body,result_run_id,result_call_id,result_method,result_args_hash,result_content,result_sources) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,'historical','{}',?8,?9,?10,?11,?12,?13)",params![snapshot,grant.scope.client,grant.scope.project,grant.id,path,serde_json::to_value(derived_split(&grant.visible_splits))?.as_str(),version,run,request.call_id,method,args_hash,content,serde_json::to_string(&sources)?])?;
        for source in sources
            .iter()
            .filter(|source| source.kind == ContextSourceKind::Record)
        {
            let current: String = tx.query_row(
                "SELECT version FROM records WHERE id=?1",
                [&source.id],
                |row| row.get(0),
            )?;
            if current != source.version {
                return Err(Error::conflict(
                    "record changed before its observation was retained; request fresh evidence",
                ));
            }
        }
        let mut references = sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        references.extend(self.store.run_context_sources(run)?);
        references.sort();
        references.dedup();
        crate::sources::source_edges(&tx, "artifact", &snapshot, &references, &[])?;
        for source in &sources {
            tx.execute("UPDATE source_edges SET source_version=?3 WHERE subject_kind='artifact' AND subject_id=?1 AND source_id=?2",params![snapshot,source.id,source.version])?;
        }
        tx.commit()?;
        // The call may itself retire a source. Its actual result can settle,
        // while the next context authorization will withhold that lineage.
        self.store.tool_observation(run, &request.call_id, false)
    }

    pub(crate) fn retained_tool(&self, run: &str, call: &str) -> Result<ToolObservation> {
        let grant = self.store.require_active(run)?;
        self.refresh_artifact_sources(&grant)?;
        self.store.tool_observation(run, call, true)
    }
}

impl Store {
    fn tool_output_sources(
        &self,
        method: &str,
        arguments: &serde_json::Map<String, Value>,
        result: &Value,
    ) -> Result<Vec<ContextSource>> {
        let mut sources = Vec::new();
        let mut record = |value: &Value| -> Result<()> {
            sources.push(ContextSource {
                kind: ContextSourceKind::Record,
                id: value["id"]
                    .as_str()
                    .ok_or_else(|| Error::internal("record tool omitted its identity"))?
                    .into(),
                version: value["version"]
                    .as_str()
                    .ok_or_else(|| Error::internal("record tool omitted its version"))?
                    .into(),
            });
            Ok(())
        };
        match method {
            "experiment.run" => {
                // The released aggregate depends on the experiment, without
                // granting access to its protected per-case observations.
                let reference = arguments["id"]
                    .as_str()
                    .ok_or_else(|| Error::internal("experiment request omitted its identity"))?;
                let version: String = self.db.query_row(
                    "SELECT version FROM records WHERE id=?1",
                    [reference],
                    |row| row.get(0),
                )?;
                record(&json!({"id":reference,"version":version}))?;
            }
            "work.status" => {
                if let Some(source) = result.get("source") {
                    sources.push(serde_json::from_value(source.clone())?);
                }
            }
            "record.read" | "record.submit" | "inventory.admission_request" => record(result)?,
            "artifact.validity" => {
                for property in result["properties"]
                    .as_array()
                    .ok_or_else(|| Error::internal("validity view omitted properties"))?
                {
                    record(&property["obligation"])?;
                }
                sources.push(ContextSource {
                    kind: ContextSourceKind::Artifact,
                    id: result["snapshot_id"]
                        .as_str()
                        .ok_or_else(|| Error::internal("validity view omitted version snapshot"))?
                        .into(),
                    version: result["artifact"]["version"]
                        .as_str()
                        .ok_or_else(|| Error::internal("validity view omitted artifact version"))?
                        .into(),
                });
                for property in result["properties"].as_array().unwrap() {
                    for reference in property["evidence_refs"].as_array().ok_or_else(|| {
                        Error::internal("property assessment omitted evidence references")
                    })? {
                        let reference = reference.as_str().ok_or_else(|| {
                            Error::internal("property evidence reference is not an identity")
                        })?;
                        let sequence: String = self.db.query_row(
                            "SELECT sequence FROM events WHERE id=?1",
                            [reference],
                            |row| row.get(0),
                        )?;
                        sources.push(ContextSource {
                            kind: ContextSourceKind::Event,
                            id: reference.into(),
                            version: sequence,
                        });
                    }
                }
            }
            "search.query" => {
                for value in result["records"]
                    .as_array()
                    .ok_or_else(|| Error::internal("search omitted records"))?
                {
                    record(value)?;
                }
            }
            "inventory.archive" => {
                for cell in result["cells"]
                    .as_array()
                    .ok_or_else(|| Error::internal("archive omitted cells"))?
                {
                    record(&cell["implementation"])?;
                }
            }
            "evidence.read" => {
                for event in result["events"]
                    .as_array()
                    .ok_or_else(|| Error::internal("evidence omitted events"))?
                {
                    sources.push(ContextSource {
                        kind: ContextSourceKind::Event,
                        id: event["id"]
                            .as_str()
                            .ok_or_else(|| Error::internal("event omitted its identity"))?
                            .into(),
                        version: event["sequence"]
                            .as_str()
                            .ok_or_else(|| Error::internal("event omitted its sequence"))?
                            .into(),
                    });
                }
            }
            "message.send" | "message.inbox" => {
                let values = if method == "message.send" {
                    vec![result]
                } else {
                    result["messages"]
                        .as_array()
                        .ok_or_else(|| Error::internal("inbox omitted messages"))?
                        .iter()
                        .collect()
                };
                for message in values {
                    sources.push(ContextSource {
                        kind: ContextSourceKind::Event,
                        id: message["id"]
                            .as_str()
                            .ok_or_else(|| Error::internal("message omitted identity"))?
                            .into(),
                        version: message["sequence"]
                            .as_str()
                            .ok_or_else(|| Error::internal("message omitted sequence"))?
                            .into(),
                    });
                }
            }
            "artifact.read" => sources.push(ContextSource {
                kind: ContextSourceKind::Artifact,
                id: result["snapshot_id"]
                    .as_str()
                    .ok_or_else(|| Error::internal("artifact omitted its retained identity"))?
                    .into(),
                version: result["artifact"]["version"]
                    .as_str()
                    .ok_or_else(|| Error::internal("artifact omitted its version"))?
                    .into(),
            }),
            "training.export" => sources.push(ContextSource {
                kind: ContextSourceKind::Artifact,
                id: result["artifact"]["path"]
                    .as_str()
                    .and_then(|p| p.strip_prefix(crate::export_files::EXPORT_PREFIX))
                    .ok_or_else(|| Error::internal("export omitted its artifact identity"))?
                    .into(),
                version: result["artifact"]["version"]
                    .as_str()
                    .ok_or_else(|| Error::internal("export omitted its version"))?
                    .into(),
            }),
            "action.execute" | "action.lookup" if result["content_available"] != false => {
                for reference in [
                    result["evidence_ref"].as_str(),
                    result["settlement"]["evidence_ref"].as_str(),
                ]
                .into_iter()
                .flatten()
                {
                    let sequence: String = self.db.query_row(
                        "SELECT sequence FROM events WHERE id=?1",
                        [reference],
                        |r| r.get(0),
                    )?;
                    sources.push(ContextSource {
                        kind: ContextSourceKind::Event,
                        id: reference.into(),
                        version: sequence,
                    });
                }
            }
            _ => {}
        }
        Ok(sources)
    }

    fn tool_observation(&self, run: &str, call: &str, authorize: bool) -> Result<ToolObservation> {
        let row:Option<(String,String,String,Option<String>,String,String)>=self.db.query_row("SELECT id,path,version,result_content,result_sources,result_method FROM artifact_snapshots WHERE result_run_id=?1 AND result_call_id=?2",params![run,call],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let (snapshot, path, version, content, sources, method) =
            row.ok_or_else(|| Error::missing("no retained result for this tool call"))?;
        if authorize {
            self.require_source(&self.run_grant(run)?, "artifact", &snapshot)?;
        }
        let content = content.ok_or_else(|| Error::denied("retained tool result was deleted"))?;
        let mut sources: Vec<ContextSource> = serde_json::from_str(&sources)?;
        sources.push(ContextSource {
            kind: ContextSourceKind::Artifact,
            id: snapshot,
            version: version.clone(),
        });
        let total_bytes = content.len().to_string();
        let artifact = ArtifactRef { path, version };
        let cursor = if method == "evidence.read" {
            serde_json::from_str::<Value>(&content)?["cursor"]
                .as_str()
                .map(str::to_owned)
        } else {
            None
        };
        let text = if content.len() > INLINE_BYTES {
            let excerpt = utf8_window(&content, 0, 4096)?;
            serde_json::to_string(
                &json!({"retained_result":artifact,"excerpt":excerpt,"offset":0,"next_offset":excerpt.len(),"total_bytes":total_bytes,"complete":false,"note":"Historical tool result. Read further bytes with artifact_read; re-read original artifacts before relying on current state."}),
            )?
        } else {
            content
        };
        Ok(ToolObservation {
            content: text,
            artifact,
            sources,
            total_bytes,
            cursor,
        })
    }

    pub(crate) fn read_result_artifact(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
    ) -> Result<ArtifactChunk> {
        if request.branch_id.is_some() || request.required_freshness == Some(Freshness::Current) {
            return Err(Error::conflict(
                "retained tool results are historical observations; inspect original artifacts for current state",
            ));
        }
        let snapshot = request
            .path
            .strip_prefix(RESULT_PREFIX)
            .ok_or_else(|| Error::invalid("invalid tool-result artifact reference"))?;
        if request
            .snapshot_id
            .as_deref()
            .is_some_and(|id| id != snapshot)
        {
            return Err(Error::denied(
                "snapshot does not match tool-result artifact",
            ));
        }
        self.require_source(grant, "artifact", snapshot)?;
        let row:Option<(Option<String>,String)>=self.db.query_row("SELECT result_content,version FROM artifact_snapshots WHERE id=?1 AND result_run_id IS NOT NULL",[snapshot],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (content, version) =
            row.ok_or_else(|| Error::denied("reference is not a retained tool result"))?;
        let content = content.ok_or_else(|| Error::denied("retained tool result was deleted"))?;
        // Keep the serialized chunk inline even when its JSON text needs
        // escaping. Reading a result must not create another excerpt to chase.
        let text = utf8_window(
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

pub(crate) fn utf8_window(content: &str, offset: usize, length: usize) -> Result<&str> {
    if offset > content.len() || !content.is_char_boundary(offset) {
        return Err(Error::invalid(
            "artifact offset must be within the retained UTF-8 content",
        ));
    }
    let mut end = (offset + length).min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    if end == offset && offset < content.len() {
        return Err(Error::invalid(
            "artifact length is smaller than the next UTF-8 character",
        ));
    }
    Ok(&content[offset..end])
}
