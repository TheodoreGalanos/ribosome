use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    validation::{MAX_FRAME, decode, encode, method_contract, validate},
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

impl Runtime {
    /// The run comes from the supervised pipe binding, never the tool payload.
    pub fn tool_call(&self, run_id: &str, method: &str, params: Value) -> Result<Value> {
        let (input, output) = method_contract(method)?;
        validate(input, &params)?;
        let settling = matches!(
            method,
            "model.usage"
                | "model.release"
                | "model.permit.lookup"
                | "budget.status"
                | "session.compaction.commit"
                | "session.context.append"
                | "session.checkpoint"
                | "session.events"
                | "action.lookup"
        );
        let grant = if settling {
            self.store.run_grant(run_id)?
        } else {
            self.store.require_active(run_id)?
        };
        if self
            .cancellation(run_id)
            .load(std::sync::atomic::Ordering::SeqCst)
            && !settling
        {
            return Err(Error::denied("run cancelled"));
        }
        if grant.discovery_corpus.is_some()
            && matches!(
                method,
                "action.execute"
                    | "record.retire"
                    | "message.ack"
                    | "artifact.validity"
                    | "experiment.run"
                    | "inventory.admission_request"
                    | "training.export"
                    | "message.send"
                    | "message.inbox"
            )
        {
            return Err(Error::denied(
                "assigned discovery uses frozen evidence; this operation requires separate host-authorized work",
            ));
        }
        if matches!(
            method,
            "session.context"
                | "session.compaction.prepare"
                | "session.compaction.read"
                | "session.compaction.commit"
                | "session.summary"
                | "model.permit"
                | "model.dispatch"
                | "session.context.read"
                | "artifact.read"
                | "artifact.validity"
                | "record.read"
                | "record.submit"
                | "search.query"
                | "evidence.read"
                | "continuation.read"
                | "inventory.archive"
                | "message.inbox"
                | "message.send"
                | "training.export"
        ) {
            self.refresh_artifact_sources(&grant)?;
        }
        if grant.prepared_run.is_some()
            && matches!(
                method,
                "message.inbox"
                    | "message.send"
                    | "training.export"
                    | "evidence.corpus"
                    | "experiment.run"
            )
        {
            return Err(Error::denied(
                "prepared reuse requires separate host-scoped investigation for this operation",
            ));
        }
        let result = match method {
            "invocation.read" => encode(output, self.store.invocation_material(run_id, &grant)?),
            "continuation.read" => encode(
                output,
                self.store.continuation(run_id, &decode(input, params)?)?,
            ),
            "evidence.corpus" => {
                let reference = grant
                    .discovery_corpus
                    .as_ref()
                    .ok_or_else(|| Error::missing("run has no host-assigned discovery corpus"))?;
                encode(output, self.store.discovery_corpus(&grant, reference)?)
            }
            "tool.call" => encode(output, self.observe_tool(run_id, &decode(input, params)?)?),
            "tool.result" => {
                let p: IdRequest = decode(input, params)?;
                encode(output, self.retained_tool(run_id, &p.id)?)
            }
            "session.compaction.prepare" => encode(output, self.store.prepare_compaction(run_id)?),
            "session.compaction.read" => {
                let p: IdRequest = decode(input, params)?;
                encode(output, self.store.read_compaction(run_id, &p.id)?)
            }
            "session.compaction.commit" => encode(
                output,
                self.store
                    .commit_compaction(run_id, &decode(input, params)?)?,
            ),
            "session.summary" => {
                let p: IdRequest = decode(input, params)?;
                encode(output, self.store.read_summary(run_id, &p.id)?)
            }
            "session.context" => encode(output, self.store.authorize_context(run_id)?),
            "session.context.append" => encode(
                output,
                self.store.append_context(run_id, &decode(input, params)?)?,
            ),
            "session.context.read" => encode(
                output,
                self.store.read_context(run_id, &decode(input, params)?)?,
            ),
            "session.grant" => encode(output, &grant),
            "budget.status" => encode(output, self.store.run_budget_status(run_id)?),
            "model.dispatch" | "model.release" | "model.permit.lookup" => {
                let request: IdRequest = decode(input, params)?;
                match method {
                    "model.dispatch" => {
                        self.store.dispatch_permit(run_id, &request.id)?;
                        Ok(json!({"ok": true}))
                    }
                    "model.release" => {
                        self.store.release_permit(run_id, &request.id)?;
                        Ok(json!({"ok": true}))
                    }
                    _ => encode(output, self.store.lookup_permit(run_id, &request.id)?),
                }
            }
            "inventory.archive" => encode(output, self.store.archive(&grant)?),
            "evidence.read" => {
                let mut request: EvidenceRequest = decode(input, params)?;
                if let Some(attachment) = self.store.run_attachment(run_id)? {
                    if request
                        .run_id
                        .as_ref()
                        .is_some_and(|id| id != &attachment.execution_id)
                    {
                        return Err(Error::denied(
                            "evidence belongs to a different attached execution",
                        ));
                    }
                    request.run_id = Some(attachment.execution_id);
                }
                encode(
                    output,
                    self.store
                        .evidence_excluding_activity(&grant, &request, Some(run_id))?,
                )
            }
            "search.query" => encode(output, self.store.search(&grant, &decode(input, params)?)?),
            "record.read" => {
                let p: IdRequest = decode(input, params)?;
                encode(output, self.store.record(&grant, &p.id)?)
            }
            "artifact.read" => {
                let p: ArtifactRead = decode(input, params)?;
                if p.path.starts_with(crate::tool_results::RESULT_PREFIX) {
                    encode(output, self.store.read_result_artifact(&grant, &p)?)
                } else if p.path.starts_with(crate::export_files::EXPORT_PREFIX) {
                    encode(output, self.store.read_export_artifact(&grant, &p)?)
                } else {
                    encode(output, self.read_artifact_context(&grant, &p)?)
                }
            }
            "action.execute" => encode(output, self.execute(run_id, decode(input, params)?)?),
            "artifact.validity" => encode(
                output,
                self.artifact_validity(&grant, &decode(input, params)?)?,
            ),
            "action.lookup" => {
                let p: IdRequest = decode(input, params)?;
                encode(output, self.lookup(run_id, &p.id)?)
            }
            "record.submit" => {
                let mut submission: RecordSubmission = decode(input, params)?;
                self.store
                    .complete_motif_context(&grant, run_id, &mut submission)?;
                self.store
                    .inherit_context_sources(run_id, &mut submission)?;
                if crate::validation::split_level(&submission.provenance.split)
                    < crate::validation::split_level(&crate::validation::derived_split(
                        &grant.visible_splits,
                    ))
                {
                    return Err(Error::denied(
                        "record split must retain the run's evidence visibility",
                    ));
                }
                if submission.kind == RecordKind::Experiment {
                    let model: String = self.store.db.query_row(
                        "SELECT json_extract(request, '$.model') FROM runs WHERE id=?1",
                        [run_id],
                        |row| row.get(0),
                    )?;
                    if submission.body.get("model_version").and_then(Value::as_str)
                        != Some(model.as_str())
                    {
                        return Err(Error::invalid(format!(
                            "experiment model_version must match this run's configured model: {model}"
                        )));
                    }
                }
                encode(
                    output,
                    self.store
                        .submit_for_run(&grant, &submission, false, Some(run_id))?,
                )
            }
            "record.retire" => {
                self.store.retire(&grant, &decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "work.wait" => encode(
                output,
                self.store.wait_for_work(run_id, &decode(input, params)?)?,
            ),
            "work.status" => {
                let request: IdRequest = decode(input, params)?;
                encode(output, self.store.work_status(run_id, &request.id)?)
            }
            "work.request" => encode(
                output,
                self.store.request_work(run_id, &decode(input, params)?)?,
            ),
            "session.checkpoint" => {
                self.store.checkpoint(run_id, &decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "session.events" => {
                self.store
                    .append_agent_activity(run_id, &decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "model.permit" => encode(output, self.store.permit(run_id, &decode(input, params)?)?),
            "model.usage" => {
                self.store.usage(run_id, &decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "message.send" => encode(
                output,
                self.store.send_message(run_id, &decode(input, params)?)?,
            ),
            "message.inbox" => encode(output, self.store.inbox(run_id)?),
            "message.ack" => {
                let p: IdRequest = decode(input, params)?;
                self.store.acknowledge_message(run_id, &p.id)?;
                Ok(json!({"ok":true}))
            }
            "experiment.run" => {
                let p: ExperimentRequest = decode(input, params)?;
                encode(output, self.run_experiment(run_id, &p.id)?)
            }
            "inventory.admission_request" => {
                let p: AdmissionRequest = decode(input, params)?;
                encode(
                    output,
                    self.request_admission(run_id, &p.recommendation_id)?,
                )
            }
            "training.export" => encode(
                output,
                self.export_training(run_id, &decode(input, params)?)?,
            ),
            _ => Err(Error {
                code: -32601,
                message: "method not available to this worker".into(),
            }),
        }?;
        validate(output, &result)?;
        Ok(result)
    }
}

pub async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Value>> {
    let mut frame = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(Error::invalid("interrupted JSON frame"))
            };
        }
        let newline = buffer.iter().position(|b| *b == b'\n');
        let take = newline.map_or(buffer.len(), |i| i + 1);
        if frame.len() + take > MAX_FRAME {
            return Err(Error::invalid("frame too large"));
        }
        frame.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if newline.is_some() {
            break;
        }
    }
    let value: Value = serde_json::from_slice(&frame).map_err(|_| Error {
        code: -32700,
        message: "invalid UTF-8 JSON frame".into(),
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| Error::invalid("invalid JSON-RPC envelope"))?;
    if value["jsonrpc"] != "2.0" || !value["id"].is_string() {
        return Err(Error::invalid("JSON-RPC 2.0 and string id required"));
    }
    let allowed = if value.get("method").is_some() {
        &["jsonrpc", "id", "method", "params"][..]
    } else {
        &["jsonrpc", "id", "result", "error"][..]
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::invalid("unknown envelope field"));
    }
    if value.get("method").is_none()
        && value.get("result").is_some() == value.get("error").is_some()
    {
        return Err(Error::invalid(
            "response requires exactly one result or error",
        ));
    }
    Ok(Some(value))
}
