use super::*;
use crate::{
    rpc::read_frame,
    supervisor::{Supervisor, WorkerConfig},
    validation::{MAX_FRAME, decode, encode, id},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, watch},
    task::JoinSet,
};

/// Explicitly owned local host. Its Store is another connection to the same
/// SQLite file, allowing ingestion while the supervisor serializes effects.
pub struct AttachmentHost {
    store: Store,
    supervisor: Supervisor,
    worker: WorkerConfig,
    grant: Grant,
    template: AgentRunRequest,
    policy: AttachmentPolicy,
    owner: String,
    active: BTreeMap<String, watch::Sender<bool>>,
}

impl AttachmentHost {
    pub fn new(
        store: Store,
        supervisor: Supervisor,
        worker: WorkerConfig,
        grant_id: &str,
        template: AgentRunRequest,
        policy: AttachmentPolicy,
    ) -> Result<Self> {
        policy.validate()?;
        validate("AgentRunRequest", &serde_json::to_value(&template)?)?;
        let runtime = supervisor.runtime();
        let runtime = runtime.try_lock().map_err(|_| {
            Error::conflict("construct attachment host before starting maintenance")
        })?;
        if store.db.path() != runtime.store.db.path() || store.db.path().is_none_or(str::is_empty) {
            return Err(Error::invalid(
                "attachment host and supervisor must use the same persistent database",
            ));
        }
        drop(runtime);
        let grant = store.grant(grant_id)?;
        store.recover_attachment_deliveries(grant_id)?;
        store.settle_attachment_work(grant_id, &policy)?;
        Ok(Self {
            store,
            supervisor,
            worker,
            grant,
            template,
            policy,
            owner: id(),
            active: BTreeMap::new(),
        })
    }

    fn bound_attachment(&self, id: &str) -> Result<Attachment> {
        let a = self.store.attachment(id)?;
        if a.grant_id != self.grant.id {
            return Err(Error::denied("attachment belongs to another host grant"));
        }
        Ok(a)
    }

    fn handle(&mut self, method: &str, params: Value) -> Result<Value> {
        let entry = &crate::validation::schema()["x-host-methods"][method];
        let input = entry[0]
            .as_str()
            .ok_or_else(|| Error::denied("method is not available on the host protocol"))?;
        let output = entry[1]
            .as_str()
            .ok_or_else(|| Error::invalid("missing host output contract"))?;
        validate(input, &params)?;
        if let Some(id) = params.get("attachment_id").and_then(Value::as_str) {
            self.bound_attachment(id)?;
        }
        let result = match method {
            "host.hello" => encode(
                output,
                HostHello {
                    protocol: "ribosome-host/1".into(),
                    scope: self.grant.scope.clone(),
                    capabilities: self.policy.capabilities(),
                },
            ),
            "attachment.open" => encode(
                output,
                self.store.open_attachment(
                    &self.grant,
                    &decode(input, params)?,
                    &self.policy,
                    &self.template,
                )?,
            ),
            "attachment.events" => encode(
                output,
                self.store
                    .append_attachment_events(&decode(input, params)?, &self.policy)?,
            ),
            "attachment.status" => {
                let p: AttachmentRequest = decode(input, params)?;
                encode(output, self.store.attachment_status(&p.attachment_id)?)
            }
            "attachment.feedback" => {
                let p: AttachmentRequest = decode(input, params)?;
                let runtime = self.supervisor.runtime();
                match runtime.try_lock() {
                    Ok(r) => encode(output, r.attachment_feedback(&p.attachment_id)?),
                    Err(_) => encode(output, AttachmentFeedbackPage { items: vec![] }),
                }
            }
            "attachment.ack" => {
                self.store.acknowledge_feedback(&decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "attachment.steer" => {
                let runtime = self.supervisor.runtime();
                let runtime = runtime.try_lock().map_err(|_| {
                    Error::conflict("runtime busy; steering has not been dispatched")
                })?;
                runtime.begin_attachment_steering(&decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            "attachment.record" => {
                let p: AttachmentRecordRequest = decode(input, params)?;
                encode(
                    output,
                    self.store
                        .attachment_record(&p.attachment_id, &p.record_id)?,
                )
            }
            "attachment.complete" => {
                let p: AttachmentRequest = decode(input, params)?;
                self.store.complete_attachment_source(&p.attachment_id)?;
                Ok(json!({"ok":true}))
            }
            "attachment.detach" => {
                let p: AttachmentRequest = decode(input, params)?;
                self.store.detach_attachment(&p.attachment_id)?;
                self.cancel_attachment(&p.attachment_id)?;
                Ok(json!({"ok":true}))
            }
            "attachment.interrupt" => {
                let p: AttachmentInterruption = decode(input, params)?;
                self.store
                    .interrupt_attachment(&p.attachment_id, &p.reason)?;
                self.cancel_attachment(&p.attachment_id)?;
                Ok(json!({"ok":true}))
            }
            "attachment.repair" => {
                let p: AttachmentRequest = decode(input, params)?;
                encode(
                    output,
                    self.store
                        .begin_attachment_repair(&p.attachment_id, &self.template)?,
                )
            }
            "attachment.release" => {
                let runtime = self.supervisor.runtime();
                let runtime = runtime
                    .try_lock()
                    .map_err(|_| Error::conflict("runtime busy; writer handoff remains held"))?;
                runtime.release_attachment_repair(&decode(input, params)?)?;
                Ok(json!({"ok":true}))
            }
            _ => Err(Error::denied("method unavailable")),
        }?;
        validate(output, &result)?;
        if serde_json::to_vec(&result)?.len() > MAX_FRAME - 1024 {
            return Err(Error::exhausted("host response exceeds frame capacity"));
        }
        Ok(result)
    }

    fn cancel_attachment(&self, id: &str) -> Result<()> {
        for (run, send) in &self.active {
            if self.store.run_attachment(run)?.is_some_and(|a| a.id == id) {
                let _ = send.send(true);
            }
        }
        Ok(())
    }

    fn tick(&mut self, tasks: &mut JoinSet<(String, Result<AgentResult>)>) -> Result<()> {
        self.store
            .settle_attachment_work(&self.grant.id, &self.policy)?;
        for id in self.store.attachment_ids(&self.grant.id)? {
            let a = self.store.attachment(&id)?;
            if a.state != AttachmentState::Active {
                self.cancel_attachment(&id)?;
                continue;
            }
            if let Err(e) = self.store.route_attachment(&id) {
                self.store.interrupt_attachment(&id, &e.message)?;
                self.cancel_attachment(&id)?;
            }
        }
        if !self.active.is_empty() {
            return Ok(());
        }
        if now_ms() >= counter(&self.grant.budget.deadline_ms)? {
            return Ok(());
        }
        if let Some(work) =
            self.store
                .claim_work_matching(&self.grant.id, &self.owner, 300000, true)?
        {
            let mut request = self.template.clone();
            request.run_id = work.id.clone();
            request.profile = work.profile;
            request.operator = work.operator;
            request.prompt = format!(
                "{}\nFollow-up for {}: {}. Evidence references: {}",
                self.template.prompt,
                work.subject,
                work.reason,
                work.evidence_refs.join(", ")
            );
            request.checkpoint = None;
            let supervisor = self.supervisor.clone();
            let worker = self.worker.clone();
            let grant_id = self.grant.id.clone();
            let (send, receive) = watch::channel(false);
            self.active.insert(work.id.clone(), send);
            tasks.spawn(async move {
                (
                    work.id,
                    supervisor.run(&worker, &grant_id, request, receive).await,
                )
            });
        }
        Ok(())
    }

    fn completed(&mut self, run_id: &str, result: Result<AgentResult>) -> Result<()> {
        self.active.remove(run_id);
        let result = result.unwrap_or_else(|e| AgentResult {
            disposition: Disposition::Failed,
            summary: e.message,
        });
        self.store.finish_run(run_id, &result)?;
        let body: String =
            self.store
                .db
                .query_row("SELECT body FROM work WHERE id=?1", [run_id], |r| r.get(0))?;
        self.store.publish_attachment_result(
            &serde_json::from_str(&body)?,
            &result,
            &self.policy,
        )?;
        if result.disposition == Disposition::Interrupted
            && let Some(a) = self.store.run_attachment(run_id)?
        {
            self.store.interrupt_attachment(&a.id, &result.summary)?;
        }
        self.store
            .settle_attachment_work(&self.grant.id, &self.policy)?;
        // Failures before begin_run still settle the claimed work lease.
        let status = match result.disposition {
            Disposition::Completed | Disposition::Abstained => WorkItemStatus::Completed,
            Disposition::Cancelled => WorkItemStatus::Cancelled,
            Disposition::Exhausted => WorkItemStatus::Exhausted,
            Disposition::Interrupted => WorkItemStatus::Interrupted,
            Disposition::Failed => WorkItemStatus::Failed,
        };
        self.store.finish_work(run_id, &self.owner, status)?;
        Ok(())
    }

    pub async fn serve<R, W>(
        mut self,
        mut input: R,
        mut output: W,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<()>
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (incoming, mut frames) = mpsc::channel(32);
        let reader = tokio::spawn(async move {
            loop {
                let frame = read_frame(&mut input).await;
                let end = !matches!(&frame, Ok(Some(_)));
                if incoming.send(frame).await.is_err() || end {
                    break;
                }
            }
        });
        let (outgoing, mut replies) = mpsc::channel::<Value>(32);
        let mut writer = tokio::spawn(async move {
            while let Some(reply) = replies.recv().await {
                let mut bytes = serde_json::to_vec(&reply)?;
                bytes.push(b'\n');
                output.write_all(&bytes).await?;
                output.flush().await?;
            }
            Ok::<_, Error>(())
        });
        let mut tasks = JoinSet::new();
        let mut hello = false;
        let mut poll = tokio::time::interval(Duration::from_millis(50));
        let mut renew = tokio::time::interval(Duration::from_secs(60));
        let outcome:Result<()>=async {
            loop {
                tokio::select! {
                    frame=frames.recv()=>{
                        let Some(frame)=frame else{break};let Some(frame)=frame? else{break};
                        let method=frame["method"].as_str().ok_or_else(||Error::invalid("host expects requests"))?;
                        let result=if !hello && method!="host.hello" {Err(Error::denied("host handshake required"))} else {self.handle(method,frame["params"].clone())};
                        if method=="host.hello" && result.is_ok(){hello=true;}
                        let reply=match result{Ok(result)=>json!({"jsonrpc":"2.0","id":frame["id"],"result":result}),Err(error)=>json!({"jsonrpc":"2.0","id":frame["id"],"error":error})};
                        outgoing.send(reply).await.map_err(|_|Error::internal("host output closed"))?;
                    },
                    _=poll.tick(),if hello=>{self.tick(&mut tasks)?;},
                    _=renew.tick()=>{for run in self.active.keys(){self.store.renew_work(run,&self.owner,300000)?;}},
                    Some(result)=tasks.join_next()=>{let (run,result)=result.map_err(|_|Error::internal("attachment worker task failed"))?;self.completed(&run,result)?;},
                    changed=cancel.changed()=>{if changed.is_err() || *cancel.borrow(){break;}},
                    result=&mut writer=>{return result.map_err(|_|Error::internal("host writer task failed"))?;},
                }
            }
            Ok(())
        }.await;
        reader.abort();
        for send in self.active.values() {
            let _ = send.send(true);
        }
        // Supervisor cancellation settles dispatched operations before exit.
        let mut cleanup_error = None;
        while let Some(result) = tasks.join_next().await {
            let settled = match result {
                Ok((run, result)) => self.completed(&run, result),
                Err(_) => Err(Error::internal(
                    "attachment worker task failed during shutdown",
                )),
            };
            if let Err(error) = settled {
                cleanup_error.get_or_insert(error);
            }
        }
        for id in self.store.attachment_ids(&self.grant.id)? {
            self.store.interrupt_attachment(
                &id,
                "Host disconnected; source execution status is unknown.",
            )?;
        }
        drop(outgoing);
        if !writer.is_finished() {
            match tokio::time::timeout(Duration::from_secs(2), &mut writer).await {
                Ok(result) => {
                    result.map_err(|_| Error::internal("host writer task failed"))??;
                }
                Err(_) => writer.abort(),
            }
        }
        outcome?;
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        Ok(())
    }
}
