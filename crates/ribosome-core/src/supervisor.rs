use crate::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    rpc::read_frame,
    timing::TimingPhase,
    validation::{MAX_FRAME, MAX_PENDING, counter, decode, id, now_ms},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::Command,
    sync::{Mutex, Semaphore, mpsc, watch},
    task::JoinSet,
};

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod concurrency_tests;

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
            parent_allocation_id: None,
            invocation: None,
            discovery_corpus: None,
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
    executor: Arc<Mutex<Runtime>>,
    slots: Arc<Semaphore>,
    run_cancellations: RunCancellations,
}

type RunCancellations =
    Arc<std::sync::Mutex<BTreeMap<String, std::sync::Weak<watch::Sender<bool>>>>>;

struct RunRegistration {
    id: String,
    cancellations: RunCancellations,
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        if let Ok(mut runs) = self.cancellations.lock() {
            runs.remove(&self.id);
        }
    }
}

impl Supervisor {
    pub fn new(runtime: Runtime, concurrency: usize) -> Result<Self> {
        if !(1..=16).contains(&concurrency) {
            return Err(Error::invalid("worker concurrency must be 1..16"));
        }
        let executor = runtime.service_connection()?;
        Ok(Self {
            runtime: Arc::new(Mutex::new(runtime)),
            executor: Arc::new(Mutex::new(executor)),
            slots: Arc::new(Semaphore::new(concurrency)),
            run_cancellations: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
        })
    }

    /// Short state operations and inspection. Host callers must use
    /// `executor()` for effects, experiment execution and reconciliation.
    pub fn runtime(&self) -> Arc<Mutex<Runtime>> {
        self.runtime.clone()
    }

    /// Host effects and reconciliation share this workspace owner. Do not
    /// acquire it while holding the state runtime returned by `runtime()`.
    pub fn executor(&self) -> Arc<Mutex<Runtime>> {
        self.executor.clone()
    }

    fn tool_runtime(&self, method: &str, params: &Value) -> Arc<Mutex<Runtime>> {
        let method = if method == "tool.call" {
            params["method"].as_str().unwrap_or(method)
        } else {
            method
        };
        if matches!(
            method,
            "action.execute" | "action.lookup" | "experiment.run" | "inventory.admission_request"
        ) {
            self.executor.clone()
        } else {
            self.runtime.clone()
        }
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
            let result = self
                .run_claimed_work(
                    config,
                    grant_id,
                    (provider, model),
                    work,
                    &owner,
                    cancel.clone(),
                )
                .await?;
            results.push(result);
        }
        Ok(results)
    }

    async fn run_claimed_work(
        &self,
        config: &WorkerConfig,
        grant_id: &str,
        provider_model: (&str, &str),
        work: WorkItem,
        owner: &str,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<AgentResult> {
        let (provider, model) = provider_model;
        let completed = {
            let runtime = self.runtime.lock().await;
            let terminal: bool = runtime.store.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1 AND status NOT IN ('running','waiting','interrupted') AND result IS NOT NULL)",
                [&work.id], |r| r.get(0),
            )?;
            if terminal {
                Some(runtime.store.run_result_for_delivery(&work.id)?)
            } else {
                None
            }
        };
        if let Some(result) = completed {
            // The host may have died after persisting the run result but before
            // acknowledging its work lease. Reuse that result without a worker.
            return self.finish_claimed_work(&work, owner, result).await;
        }
        let request = AgentRunRequest {
            run_id: work.id.clone(),
            profile: work.profile.clone(),
            operator: work.operator.clone(),
            prompt: format!(
                "Follow-up for {}: {}. Evidence references: {}",
                work.subject,
                work.reason,
                work.evidence_refs.join(", ")
            ),
            provider: provider.into(),
            model: model.into(),
            checkpoint: None,
            parent_allocation_id: None,
            invocation: None,
            discovery_corpus: {
                let runtime = self.runtime.lock().await;
                // Attached/source executions need not be Ribosome runs.
                let parent_is_run: bool = runtime.store.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1)",
                    [&work.parent_id],
                    |row| row.get(0),
                )?;
                if parent_is_run {
                    runtime.store.run_grant(&work.parent_id)?.discovery_corpus
                } else {
                    None
                }
            },
        };
        let (stop_child, child_cancel) =
            watch::channel(*cancel.borrow() || cancel.has_changed().is_err());
        let run = self.run(config, grant_id, request, child_cancel);
        tokio::pin!(run);
        let mut renew = tokio::time::interval(Duration::from_secs(60));
        let mut lease_error = None;
        let mut stop_sent = *stop_child.borrow();
        let result = loop {
            tokio::select! {
                result=&mut run=>break result?,
                changed=cancel.changed(), if !stop_sent=>{
                    if changed.is_err() || *cancel.borrow() { let _=stop_child.send(true); stop_sent=true; }
                }
                _=renew.tick(), if lease_error.is_none()=>{
                    if let Err(error)=self.runtime.lock().await.store.renew_work(&work.id,owner,300000) {
                        lease_error=Some(error);let _=stop_child.send(true);stop_sent=true;
                    }
                }
            }
        };
        let result = self.finish_claimed_work(&work, owner, result).await?;
        if let Some(error) = lease_error {
            return Err(error);
        }
        Ok(result)
    }

    async fn finish_claimed_work(
        &self,
        work: &WorkItem,
        owner: &str,
        result: AgentResult,
    ) -> Result<AgentResult> {
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
            .finish_work(&work.id, owner, status)?;
        Ok(result)
    }

    async fn run_waited_work(
        &self,
        config: &WorkerConfig,
        grant_id: &str,
        request: &AgentRunRequest,
        deadline: u64,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<Option<AgentResult>> {
        let owner = id();
        loop {
            let stop = if *cancel.borrow() || cancel.has_changed().is_err() {
                Some(Disposition::Cancelled)
            } else if now_ms() >= deadline {
                Some(Disposition::Exhausted)
            } else {
                None
            };
            if let Some(disposition) = stop {
                return Ok(Some(AgentResult { disposition, summary: "Parent stopped while waiting for child work; issued executions retain their owners.".into() }));
            }
            let work = {
                let runtime = self.runtime.lock().await;
                let wait = runtime
                    .store
                    .waiting_work(&request.run_id)?
                    .ok_or_else(|| Error::internal("missing parent wait"))?;
                if !runtime
                    .store
                    .work_wait_status(&request.run_id, &wait)?
                    .wait_required
                {
                    runtime.store.resume_waiting(&request.run_id)?;
                    return Ok(None);
                }
                runtime
                    .store
                    .claim_waited_work(grant_id, &owner, &wait.work_ids)?
            };
            if let Some(work) = work {
                self.run_claimed_work(
                    config,
                    grant_id,
                    (&request.provider, &request.model),
                    work,
                    &owner,
                    cancel.clone(),
                )
                .await?;
            } else {
                tokio::select! {
                    _ = cancel.changed() => {},
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {},
                }
            }
        }
    }

    pub async fn run(
        &self,
        config: &WorkerConfig,
        grant_id: &str,
        request: AgentRunRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<AgentResult> {
        // The caller owns only a wait. The supervised task retains worker and
        // execution handles until issued effects have settled, even if that
        // wait is dropped. Closing this channel requests cancellation.
        let (send, receive) = watch::channel(*cancel.borrow());
        let send = Arc::new(send);
        {
            let mut runs = self
                .run_cancellations
                .lock()
                .map_err(|_| Error::internal("run cancellation registry failed"))?;
            if runs.contains_key(&request.run_id) {
                return Err(Error::conflict("run already has a supervisor owner"));
            }
            // A weak reference lets another scheduler signal this run without
            // keeping its caller's cancellation channel alive after abandonment.
            runs.insert(request.run_id.clone(), Arc::downgrade(&send));
        }
        let registration = RunRegistration {
            id: request.run_id.clone(),
            cancellations: self.run_cancellations.clone(),
        };
        let supervisor = self.clone();
        let config = config.clone();
        let grant_id = grant_id.to_owned();
        let mut owned = tokio::spawn(async move {
            let _registration = registration;
            supervisor
                .run_owned(&config, &grant_id, request, receive)
                .await
        });
        loop {
            tokio::select! {
                result = &mut owned => return result.map_err(|_| Error::internal("supervised task failed"))?,
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        let _ = send.send(true);
                        return owned.await.map_err(|_| Error::internal("supervised task failed"))?;
                    }
                }
            }
        }
    }

    fn run_owned<'a>(
        &'a self,
        config: &'a WorkerConfig,
        grant_id: &'a str,
        mut request: AgentRunRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentResult>> + Send + 'a>> {
        Box::pin(async move {
            if !config.node.is_absolute() || !config.worker.is_absolute() {
                return Err(Error::invalid(
                    "worker executable and entrypoint must be absolute paths",
                ));
            }
            let mut deadline = {
                let runtime = self.runtime.lock().await;
                counter(&runtime.store.grant(grant_id)?.budget.deadline_ms)?
            };
            let run_id = request.run_id.clone();
            let (stop_flag, has_effects) = {
                let runtime = self.runtime.lock().await;
                runtime.store.begin_run(&run_id, grant_id, &request)?;
                let has_effects: bool = runtime.store.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM effects WHERE run_id=?1)",
                    [&run_id],
                    |row| row.get(0),
                )?;
                (runtime.cancellation(&run_id), has_effects)
            };
            deadline = deadline.min(counter(
                &self
                    .runtime
                    .lock()
                    .await
                    .store
                    .run_budget_status(&run_id)?
                    .remaining
                    .deadline_ms,
            )?);
            stop_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            if *cancel.borrow() || cancel.has_changed().is_err() {
                stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                return self
                    .finish(
                        &run_id,
                        AgentResult {
                            disposition: Disposition::Cancelled,
                            summary: "Cancelled before worker dispatch.".into(),
                        },
                    )
                    .await;
            }
            if has_effects {
                // Acquire workspace ownership only after releasing the state lock.
                // Fresh runs do not queue behind unrelated reconciliation/effects.
                let executor = tokio::select! {
                    biased;
                    _ = cancel.changed() => {
                        stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        return self.finish(&run_id, AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled before effect reconciliation.".into() }).await;
                    }
                    owner = tokio::time::timeout(Duration::from_millis(deadline.saturating_sub(now_ms())), self.executor.lock()) => match owner {
                        Ok(owner) => owner,
                        Err(_) => {
                            stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                            return self.finish(&run_id, AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached before effect reconciliation.".into() }).await;
                        }
                    }
                };
                let receipts = executor.reconcile_run(&run_id)?;
                drop(executor);
                if receipts.iter().any(ActionReceipt::requires_reconciliation) {
                    return self.finish(&run_id, AgentResult {
                    disposition: Disposition::Interrupted,
                    summary: "Unknown host effects require owner reconciliation before continuation.".into(),
                }).await;
                }
            }
            loop {
                if self
                    .runtime
                    .lock()
                    .await
                    .store
                    .waiting_work(&run_id)?
                    .is_some()
                {
                    let waiting_started = Instant::now();
                    let result = match self
                        .run_waited_work(config, grant_id, &request, deadline, &mut cancel)
                        .await
                    {
                        Ok(result) => result,
                        Err(error) => Some(AgentResult {
                            disposition: if error.code == Error::exhausted("").code {
                                Disposition::Exhausted
                            } else {
                                Disposition::Interrupted
                            },
                            summary: format!("Child scheduling stopped: {}", error.message),
                        }),
                    };
                    let waiting_elapsed = waiting_started.elapsed();
                    self.runtime
                        .lock()
                        .await
                        .store
                        .observe_timings(&run_id, &[(TimingPhase::ChildWait, waiting_elapsed)]);
                    if let Some(result) = result {
                        stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        return self.finish(&run_id, result).await;
                    }
                }
                request.checkpoint = self.runtime.lock().await.store.load_checkpoint(&run_id)?;
                let queue_started = Instant::now();
                let capacity = tokio::select! {
                    biased;
                    _ = cancel.changed() => Err(AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled while waiting for worker capacity.".into() }),
                    permit = tokio::time::timeout(Duration::from_millis(deadline.saturating_sub(now_ms())), self.slots.acquire()) => match permit {
                        Ok(Ok(permit)) => Ok(permit),
                        Ok(Err(_)) => return Err(Error::internal("supervisor closed")),
                        Err(_) => Err(AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached while waiting for worker capacity.".into() }),
                    },
                };
                let queue_elapsed = queue_started.elapsed();
                self.runtime
                    .lock()
                    .await
                    .store
                    .observe_timings(&run_id, &[(TimingPhase::WorkerQueue, queue_elapsed)]);
                let slot = match capacity {
                    Ok(permit) => permit,
                    Err(result) => {
                        stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        return self.finish(&run_id, result).await;
                    }
                };
                let outcome = self
                    .worker_session(
                        config,
                        request.clone(),
                        deadline,
                        &mut cancel,
                        stop_flag.clone(),
                    )
                    .await;
                drop(slot);
                let result = match outcome {
                    Ok(None) => continue,
                    Ok(Some(result)) => result,
                    Err(error) => AgentResult {
                        disposition: Disposition::Interrupted,
                        summary: format!("Worker did not complete: {}", error.message),
                    },
                };
                stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                return self.finish(&run_id, result).await;
            }
        })
    }

    async fn finish(&self, run_id: &str, result: AgentResult) -> Result<AgentResult> {
        let runtime = self.runtime.lock().await;
        if matches!(
            result.disposition,
            Disposition::Cancelled | Disposition::Exhausted
        ) {
            let children = runtime
                .store
                .stop_waiting_children(run_id, &result.disposition)?;
            let owners = self
                .run_cancellations
                .lock()
                .map_err(|_| Error::internal("run cancellation registry failed"))?;
            for child in children {
                if let Some(send) = owners.get(&child).and_then(std::sync::Weak::upgrade) {
                    let _ = send.send(true);
                }
            }
        }
        runtime.store.finish_run(run_id, &result)?;
        runtime.store.run_result_for_delivery(run_id)
    }

    async fn worker_session(
        &self,
        config: &WorkerConfig,
        request: AgentRunRequest,
        deadline: u64,
        cancel: &mut watch::Receiver<bool>,
        stop_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Option<AgentResult>> {
        let startup_started = Instant::now();
        let mut startup_observed = false;
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
        let hello = json!({"protocol":"ribosome/1","build":"0.1.0","pi":"0.85.1","session":session,"capabilities":["agent.run","agent.cancel","agent.steer","context.v2","context.sources/4","context.compaction/1","context.results/1","budget.allocations/1","context.parking/1"]});
        outbound
            .send(json!({"jsonrpc":"2.0","id":hello_id,"method":"bridge.hello","params":hello}))
            .await
            .map_err(|_| Error::internal("writer closed"))?;
        let mut tools = JoinSet::new();
        let (stop_tools, stopping_tools) = watch::channel(false);
        let mut stopping: Option<Disposition> = None;
        let mut result=async {
            let frame = tokio::select! {
                response = tokio::time::timeout(Duration::from_millis(5000.min(deadline.saturating_sub(now_ms()))), inbound.recv()) => {
                    match response {
                        Ok(frame) => frame.ok_or_else(|| Error::internal("worker closed during handshake"))??,
                        Err(_) if now_ms() >= deadline => return Ok(Some(AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached during worker handshake.".into() })),
                        Err(_) => return Err(Error::invalid("worker handshake timed out")),
                    }
                }
                _ = cancel.changed() => {
                    stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    return Ok(Some(AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled during worker handshake.".into() }));
                }
            };
            if frame["id"]!=hello_id{return Err(Error::invalid("unexpected handshake response"));}
            let accepted:Handshake=decode("Handshake",frame["result"].clone())?;
            if accepted.session!=session || !accepted.capabilities.iter().any(|c|c=="agent.run"){return Err(Error::invalid("worker handshake incompatible"));}
            if request.checkpoint.as_ref().is_some_and(|c| c.format=="pi-0.85.1/2") && !accepted.capabilities.iter().any(|c|c=="context.v2") { return Err(Error::invalid("worker lacks context.v2 continuation support")); }
            if !accepted.capabilities.iter().any(|c|c=="context.sources/4") { return Err(Error::invalid("worker lacks context.sources/4 source lineage authorization")); }
            if !accepted.capabilities.iter().any(|c|c=="context.compaction/1") { return Err(Error::invalid("worker lacks context.compaction/1 semantic continuation")); }
            if !accepted.capabilities.iter().any(|c|c=="context.results/1") { return Err(Error::invalid("worker lacks context.results/1 retained tool observations")); }
            if !accepted.capabilities.iter().any(|c| c=="budget.allocations/1") { return Err(Error::invalid("worker lacks budget.allocations/1 provider accounting")); }
            if !accepted.capabilities.iter().any(|c| c=="context.parking/1") { return Err(Error::invalid("worker lacks context.parking/1 parent continuation")); }
            let startup_elapsed = startup_started.elapsed();
            self.runtime.lock().await.store.observe_timings(&request.run_id, &[(TimingPhase::WorkerStartup, startup_elapsed)]);
            startup_observed = true;
            let request_id=id();
            outbound.send(json!({"jsonrpc":"2.0","id":request_id,"method":"agent.run","params":request})).await.map_err(|_|Error::internal("writer closed"))?;
            let mut active_ids=HashSet::new();
            let mut stop_at=deadline;
            loop {
                tokio::select! {
                    frame=inbound.recv()=>{
                        let frame=frame.ok_or_else(||Error::internal("worker pipe closed"))??;
                        if let Some(method)=frame["method"].as_str(){
                            let rpc_id=frame["id"].as_str().unwrap().to_owned();
                            if method == "session.park" {
                                let parked = if !active_ids.is_empty() || stop_flag.load(std::sync::atomic::Ordering::SeqCst) {
                                    Err(Error::conflict("parking requires all issued requests to settle"))
                                } else {
                                    match decode::<SessionParkRequest>("SessionParkRequest", frame["params"].clone()) {
                                        Ok(requested) => self.runtime.lock().await.store.park_run(&request.run_id, &requested),
                                        Err(error) => Err(error),
                                    }
                                };
                                match parked {
                                    Ok(()) => return Ok(None),
                                    Err(error) => {
                                        outbound.send(json!({"jsonrpc":"2.0","id":rpc_id,"error":error})).await.map_err(|_| Error::internal("writer closed"))?;
                                        continue;
                                    }
                                }
                            }
                            if active_ids.len()>=MAX_PENDING || !active_ids.insert(rpc_id.clone()){return Err(Error::exhausted("worker request capacity exceeded"));}
                            let params=frame["params"].clone();let runtime=self.tool_runtime(method, &params);let run_id=request.run_id.clone();let method=method.to_owned();
                            let execution=Arc::ptr_eq(&runtime, &self.executor);
                            let mut stopped=stopping_tools.clone();
                            let queued = Instant::now();
                            tools.spawn(async move {
                                let runtime = tokio::select! {
                                    biased;
                                    _ = stopped.changed(), if execution => return (rpc_id, Err(Error::denied("run stopped before tool dispatch"))),
                                    runtime = runtime.lock_owned() => runtime,
                                };
                                let queue_wait = queued.elapsed();
                                let blocking_started = Instant::now();
                                let result = tokio::task::spawn_blocking(move || {
                                    let blocking_wait = blocking_started.elapsed();
                                    let started = Instant::now();
                                    let write_before = runtime.store.write_wait_us.get();
                                    let result = runtime.tool_call(&run_id, &method, params);
                                    let service = started.elapsed();
                                    let write_wait = Duration::from_micros(runtime.store.write_wait_us.get().saturating_sub(write_before));
                                    runtime.store.observe_timings(&run_id, &[
                                        (if execution { TimingPhase::ExecutorQueue } else { TimingPhase::StateQueue }, queue_wait),
                                        (TimingPhase::BlockingQueue, blocking_wait),
                                        (if execution { TimingPhase::ExecutorService } else { TimingPhase::StateService }, service),
                                        (TimingPhase::SqliteWriteBegin, write_wait),
                                    ]);
                                    result
                                }).await.unwrap_or_else(|_| Err(Error::internal("tool executor failed")));
                                (rpc_id,result)
                            });
                        }else if frame["id"]==request_id {
                            if let Some(error)=frame.get("error"){return Err(Error::internal(error["message"].as_str().unwrap_or("agent request failed")));}
                            let mut result:AgentResult=decode("AgentResult",frame["result"].clone())?;
                            if let Some(disposition)=stopping.clone() {result.disposition=disposition;}
                            return Ok(Some(result));
                        }
                    }
                    tool=tools.join_next(),if !tools.is_empty()=>{
                        let (rpc_id,result)=tool.unwrap().map_err(|_|Error::internal("tool task failed"))?;active_ids.remove(&rpc_id);
                        let response=match result{Ok(result)=>json!({"jsonrpc":"2.0","id":rpc_id,"result":result}),Err(error)=>json!({"jsonrpc":"2.0","id":rpc_id,"error":error})};
                        outbound.send(response).await.map_err(|_|Error::internal("writer closed"))?;
                    }
                    changed=cancel.changed(),if stopping.is_none()=>{
                        if changed.is_err() || *cancel.borrow(){stop_flag.store(true,std::sync::atomic::Ordering::SeqCst);let _=stop_tools.send(true);stopping=Some(Disposition::Cancelled);stop_at=now_ms()+1000;outbound.send(json!({"jsonrpc":"2.0","id":id(),"method":"agent.cancel","params":{}})).await.map_err(|_|Error::internal("writer closed"))?;}
                    }
                    _=tokio::time::sleep(Duration::from_millis(stop_at.saturating_sub(now_ms())))=>{
                        if let Some(disposition)=stopping.clone(){return Ok(Some(AgentResult{disposition,summary:"Run stopped at its cancellation or deadline boundary; host receipts remain authoritative.".into()}));}
                        else{stop_flag.store(true,std::sync::atomic::Ordering::SeqCst);let _=stop_tools.send(true);stopping=Some(Disposition::Exhausted);stop_at=now_ms()+1000;outbound.send(json!({"jsonrpc":"2.0","id":id(),"method":"agent.cancel","params":{}})).await.map_err(|_|Error::internal("writer closed"))?;}
                    }
                }
            }
        }.await;
        if !startup_observed {
            let startup_elapsed = startup_started.elapsed();
            self.runtime.lock().await.store.observe_timings(
                &request.run_id,
                &[(TimingPhase::WorkerStartup, startup_elapsed)],
            );
        }
        if result.is_err()
            && let Some(disposition) = stopping
        {
            result = Ok(Some(AgentResult {
                disposition,
                summary:
                    "Worker stopped after cancellation or deadline; issued host work is settling."
                        .into(),
            }));
        }
        if !matches!(&result, Ok(None))
            && !matches!(&result, Ok(Some(result)) if matches!(result.disposition, Disposition::Completed | Disposition::Abstained))
        {
            stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = stop_tools.send(true);
        }
        drop(outbound);
        writer.abort();
        reader.abort();
        let _ = child.kill().await;
        let _ = child.wait().await;
        // Keep every issued handle through worker exit and cancellation. An
        // already running blocking task cannot be stopped by dropping its RPC.
        while !tools.is_empty() {
            tokio::select! {
                task = tools.join_next() => {
                    let failed = !matches!(task, Some(Ok((_, Ok(_)))));
                    if failed && let Ok(Some(result)) = &mut result && result.disposition == Disposition::Completed {
                        result.disposition = Disposition::Failed;
                        result.summary = "Worker completed before outstanding tool requests settled; at least one failed.".into();
                    }
                }
                _ = cancel.changed(), if !stop_flag.load(std::sync::atomic::Ordering::SeqCst) => {
                    stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = stop_tools.send(true);
                    result = Ok(Some(AgentResult { disposition: Disposition::Cancelled, summary: "Cancelled while settling issued tool requests.".into() }));
                }
                _ = tokio::time::sleep(Duration::from_millis(deadline.saturating_sub(now_ms()))), if !stop_flag.load(std::sync::atomic::Ordering::SeqCst) => {
                    stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = stop_tools.send(true);
                    result = Ok(Some(AgentResult { disposition: Disposition::Exhausted, summary: "Root deadline reached while settling issued tool requests.".into() }));
                }
            }
        }
        result
    }
}
