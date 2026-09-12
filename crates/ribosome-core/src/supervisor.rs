use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    rpc::read_frame,
    validation::{MAX_FRAME, MAX_PENDING, counter, decode, id, now_ms},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::Command,
    sync::{Mutex, Semaphore, mpsc, watch},
    task::JoinSet,
};

#[derive(Clone)]
pub struct WorkerConfig {
    pub node: PathBuf,
    pub worker: PathBuf,
    /// Explicit provider environment supplied by the host. Never logged.
    pub environment: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{host::LocalHost, store::Store};

    #[tokio::test]
    async fn cancellation_while_waiting_for_worker_capacity_is_recorded_without_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().join("state.db")).unwrap();
        let grant: Grant = decode("Grant", json!({"id":"capacity-grant","scope":{"client":"test","project":"capacity"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":1,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":1,"max_work_items":1,"max_depth":1,"deadline_ms":(now_ms()+60000).to_string()},"context":"capacity","visible_splits":["development"],"allow_export":false})).unwrap();
        store.register_grant(&grant).unwrap();
        let host = LocalHost::new(directory.path(), BTreeMap::new()).unwrap();
        let supervisor = Supervisor::new(
            Runtime::new(store, Box::new(host), directory.path().join("state")).unwrap(),
            1,
        )
        .unwrap();
        let _occupied = supervisor.slots.acquire().await.unwrap();
        let config = WorkerConfig {
            node: directory.path().join("must-not-launch-node"),
            worker: directory.path().join("must-not-launch-worker"),
            environment: BTreeMap::new(),
        };
        let request = AgentRunRequest {
            run_id: "waiting-run".into(),
            profile: Profile::Caretaker,
            operator: "proofreading@1".into(),
            prompt: "Wait for capacity".into(),
            provider: "openai".into(),
            model: "test-model".into(),
            checkpoint: None,
        };
        let (send, receive) = watch::channel(false);
        let run = supervisor.run(&config, &grant.id, request, receive);
        let trigger = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            send.send(true).unwrap();
        });
        let result = tokio::time::timeout(Duration::from_millis(250), run)
            .await
            .expect("capacity cancellation must not wait for the grant deadline")
            .unwrap();
        trigger.await.unwrap();
        assert_eq!(result.disposition, Disposition::Cancelled);
        let inspection = supervisor
            .runtime
            .lock()
            .await
            .store
            .inspect_run("waiting-run")
            .unwrap();
        assert_eq!(inspection["status"], "cancelled");
        assert_eq!(inspection["model_usage"]["calls"], 0);
        assert_eq!(inspection["effects"].as_array().unwrap().len(), 0);
    }
}

#[derive(Clone)]
pub struct Supervisor {
    runtime: Arc<Mutex<Runtime>>,
    slots: Arc<Semaphore>,
}

impl Supervisor {
    pub fn new(runtime: Runtime, concurrency: usize) -> Result<Self> {
        if !(1..=16).contains(&concurrency) {
            return Err(Error::invalid("worker concurrency must be 1..16"));
        }
        Ok(Self {
            runtime: Arc::new(Mutex::new(runtime)),
            slots: Arc::new(Semaphore::new(concurrency)),
        })
    }

    pub fn runtime(&self) -> Arc<Mutex<Runtime>> {
        self.runtime.clone()
    }

    pub async fn drain_work(
        &self,
        config: &WorkerConfig,
        grant_id: &str,
        provider: &str,
        model: &str,
        cancel: watch::Receiver<bool>,
    ) -> Result<Vec<AgentResult>> {
        let mut results = Vec::new();
        let owner = id();
        loop {
            if *cancel.borrow() {
                break;
            }
            let work = {
                self.runtime
                    .lock()
                    .await
                    .store
                    .claim_work(grant_id, &owner, 300000)?
            };
            let Some(work) = work else { break };
            let request = AgentRunRequest {
                run_id: work.id.clone(),
                profile: work.profile,
                operator: work.operator,
                prompt: format!(
                    "Follow-up for {}: {}. Evidence references: {}",
                    work.subject,
                    work.reason,
                    work.evidence_refs.join(", ")
                ),
                provider: provider.into(),
                model: model.into(),
                checkpoint: None,
            };
            let run = self.run(config, grant_id, request, cancel.clone());
            tokio::pin!(run);
            let mut renew = tokio::time::interval(Duration::from_secs(60));
            let result = loop {
                tokio::select! {
                    result=&mut run=>break result?,
                    _=renew.tick()=>{self.runtime.lock().await.store.renew_work(&work.id,&owner,300000)?;}
                }
            };
            let status = match result.disposition {
                Disposition::Completed | Disposition::Abstained => WorkItemStatus::Completed,
                Disposition::Cancelled => WorkItemStatus::Cancelled,
                Disposition::Exhausted => WorkItemStatus::Exhausted,
                Disposition::Interrupted => WorkItemStatus::Interrupted,
                Disposition::Failed => WorkItemStatus::Failed,
            };
            self.runtime
                .lock()
                .await
                .store
                .finish_work(&work.id, &owner, status)?;
            results.push(result);
        }
        Ok(results)
    }

    pub async fn run(
        &self,
        config: &WorkerConfig,
        grant_id: &str,
        mut request: AgentRunRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<AgentResult> {
        if !config.node.is_absolute() || !config.worker.is_absolute() {
            return Err(Error::invalid(
                "worker executable and entrypoint must be absolute paths",
            ));
        }
        let deadline = {
            let runtime = self.runtime.lock().await;
            counter(&runtime.store.grant(grant_id)?.budget.deadline_ms)?
        };
        {
            let runtime = self.runtime.lock().await;
            runtime
                .store
                .begin_run(&request.run_id, grant_id, &request)?;
            let receipts = runtime.reconcile_run(&request.run_id)?;
            if receipts.iter().any(|r| r.status == EffectStatus::Unknown) {
                let result = AgentResult {
                    disposition: Disposition::Interrupted,
                    summary:
                        "Unknown host effects require owner reconciliation before continuation."
                            .into(),
                };
                runtime.store.finish_run(&request.run_id, &result)?;
                return Ok(result);
            }
            request.checkpoint = runtime.store.load_checkpoint(&request.run_id)?;
        }
        let run_id = request.run_id.clone();
        let stop_flag = self.runtime.lock().await.cancellation(&run_id);
        stop_flag.store(false, std::sync::atomic::Ordering::SeqCst);
        if *cancel.borrow() {
            let result = AgentResult {
                disposition: Disposition::Cancelled,
                summary: "Cancelled before worker dispatch.".into(),
            };
            self.runtime
                .lock()
                .await
                .store
                .finish_run(&run_id, &result)?;
            return Ok(result);
        }
        let capacity = tokio::select! {
            biased;
            _ = cancel.changed() => Err(AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled while waiting for worker capacity.".into() }),
            permit = tokio::time::timeout(Duration::from_millis(deadline.saturating_sub(now_ms())), self.slots.acquire()) => match permit {
                Ok(Ok(permit)) => Ok(permit),
                Ok(Err(_)) => return Err(Error::internal("supervisor closed")),
                Err(_) => Err(AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached while waiting for worker capacity.".into() }),
            },
        };
        let _slot = match capacity {
            Ok(permit) => permit,
            Err(result) => {
                stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                self.runtime
                    .lock()
                    .await
                    .store
                    .finish_run(&run_id, &result)?;
                return Ok(result);
            }
        };
        let outcome = self
            .worker_session(config, request, deadline, &mut cancel, stop_flag.clone())
            .await;
        stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        let result = match outcome {
            Ok(result) => result,
            Err(error) => AgentResult {
                disposition: Disposition::Interrupted,
                summary: format!("Worker did not complete: {}", error.message),
            },
        };
        self.runtime
            .lock()
            .await
            .store
            .finish_run(&run_id, &result)?;
        Ok(result)
    }

    async fn worker_session(
        &self,
        config: &WorkerConfig,
        request: AgentRunRequest,
        deadline: u64,
        cancel: &mut watch::Receiver<bool>,
        stop_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<AgentResult> {
        let mut child = Command::new(&config.node)
            .arg(&config.worker)
            .env_clear()
            .envs(&config.environment)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (outbound, mut writes) = mpsc::channel::<Value>(MAX_PENDING);
        let writer = tokio::spawn(async move {
            while let Some(value) = writes.recv().await {
                let mut bytes = serde_json::to_vec(&value)?;
                bytes.push(b'\n');
                if bytes.len() > MAX_FRAME {
                    return Err(Error::invalid("outbound frame too large"));
                }
                stdin.write_all(&bytes).await?;
                stdin.flush().await?;
            }
            Ok::<_, Error>(())
        });
        let (frames, mut inbound) = mpsc::channel(MAX_PENDING);
        let reader = tokio::spawn(async move {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_frame(&mut stdout).await {
                    Ok(Some(frame)) => {
                        if frames.send(Ok(frame)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = frames
                            .send(Err(Error::internal("worker pipe closed")))
                            .await;
                        break;
                    }
                    Err(error) => {
                        let _ = frames.send(Err(error)).await;
                        break;
                    }
                }
            }
        });
        let hello_id = id();
        let session = id();
        let hello = json!({"protocol":"ribosome/1","build":"0.1.0","pi":"0.85.1","session":session,"capabilities":["agent.run","agent.cancel","agent.steer"]});
        outbound
            .send(json!({"jsonrpc":"2.0","id":hello_id,"method":"bridge.hello","params":hello}))
            .await
            .map_err(|_| Error::internal("writer closed"))?;
        let result=async {
            let frame = tokio::select! {
                response = tokio::time::timeout(Duration::from_millis(5000.min(deadline.saturating_sub(now_ms()))), inbound.recv()) => {
                    match response {
                        Ok(frame) => frame.ok_or_else(|| Error::internal("worker closed during handshake"))??,
                        Err(_) if now_ms() >= deadline => return Ok(AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached during worker handshake.".into() }),
                        Err(_) => return Err(Error::invalid("worker handshake timed out")),
                    }
                }
                _ = cancel.changed() => {
                    stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    return Ok(AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled during worker handshake.".into() });
                }
            };
            if frame["id"]!=hello_id{return Err(Error::invalid("unexpected handshake response"));}
            let accepted:Handshake=decode("Handshake",frame["result"].clone())?;
            if accepted.session!=session || !accepted.capabilities.iter().any(|c|c=="agent.run"){return Err(Error::invalid("worker handshake incompatible"));}
            let request_id=id();
            outbound.send(json!({"jsonrpc":"2.0","id":request_id,"method":"agent.run","params":request})).await.map_err(|_|Error::internal("writer closed"))?;
            let mut tools=JoinSet::new();let mut active_ids=HashSet::new();let mut stopping:Option<Disposition>=None;
            let mut stop_at=deadline;
            loop {
                tokio::select! {
                    frame=inbound.recv()=>{
                        let frame=frame.ok_or_else(||Error::internal("worker pipe closed"))??;
                        if let Some(method)=frame["method"].as_str(){
                            let rpc_id=frame["id"].as_str().unwrap().to_owned();
                            if active_ids.len()>=MAX_PENDING || !active_ids.insert(rpc_id.clone()){return Err(Error::exhausted("worker request capacity exceeded"));}
                            let runtime=self.runtime.clone();let run_id=request.run_id.clone();let method=method.to_owned();let params=frame["params"].clone();
                            tools.spawn_blocking(move||{
                                let result=runtime.blocking_lock().tool_call(&run_id,&method,params);
                                (rpc_id,result)
                            });
                        }else if frame["id"]==request_id {
                            if let Some(error)=frame.get("error"){return Err(Error::internal(error["message"].as_str().unwrap_or("agent request failed")));}
                            let mut result:AgentResult=decode("AgentResult",frame["result"].clone())?;
                            if let Some(disposition)=stopping {result.disposition=disposition;}
                            // A final agent response does not cancel host effects already
                            // dispatched. Settle their receipts before returning.
                            while let Some(task)=tools.join_next().await {
                                let (_, outcome)=task.map_err(|_|Error::internal("tool task failed"))?;
                                if outcome.is_err() && result.disposition==Disposition::Completed {
                                    result.disposition=Disposition::Failed;result.summary="Worker completed before outstanding tool requests settled; at least one failed.".into();
                                }
                            }
                            return Ok(result);
                        }
                    }
                    tool=tools.join_next(),if !tools.is_empty()=>{
                        let (rpc_id,result)=tool.unwrap().map_err(|_|Error::internal("tool task failed"))?;active_ids.remove(&rpc_id);
                        let response=match result{Ok(result)=>json!({"jsonrpc":"2.0","id":rpc_id,"result":result}),Err(error)=>json!({"jsonrpc":"2.0","id":rpc_id,"error":error})};
                        outbound.send(response).await.map_err(|_|Error::internal("writer closed"))?;
                    }
                    changed=cancel.changed(),if stopping.is_none()=>{
                        if changed.is_err() || *cancel.borrow(){stop_flag.store(true,std::sync::atomic::Ordering::SeqCst);stopping=Some(Disposition::Cancelled);stop_at=now_ms()+1000;outbound.send(json!({"jsonrpc":"2.0","id":id(),"method":"agent.cancel","params":{}})).await.map_err(|_|Error::internal("writer closed"))?;}
                    }
                    _=tokio::time::sleep(Duration::from_millis(stop_at.saturating_sub(now_ms())))=>{
                        if let Some(disposition)=stopping{return Ok(AgentResult{disposition,summary:"Run stopped at its cancellation or deadline boundary; host receipts remain authoritative.".into()});}
                        else{stop_flag.store(true,std::sync::atomic::Ordering::SeqCst);stopping=Some(Disposition::Exhausted);stop_at=now_ms()+1000;outbound.send(json!({"jsonrpc":"2.0","id":id(),"method":"agent.cancel","params":{}})).await.map_err(|_|Error::internal("writer closed"))?;}
                    }
                }
            }
        }.await;
        drop(outbound);
        writer.abort();
        reader.abort();
        let _ = child.kill().await;
        let _ = child.wait().await;
        result
    }
}
