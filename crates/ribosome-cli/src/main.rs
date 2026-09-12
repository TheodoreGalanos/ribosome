use ribosome_core::attachments::{AttachmentHost, AttachmentPolicy};
use ribosome_core::experiments::{AdmissionPolicy, CommandEvaluator, EvaluationCase};
use ribosome_core::{
    contracts::*,
    effects::Runtime,
    error::{Error, Result},
    host::{LocalHost, RegisteredTool},
    store::Store,
    supervisor::{Supervisor, WorkerConfig},
    validation::{decode, id, now_ms, validate},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    workspace: PathBuf,
    state_dir: PathBuf,
    node: PathBuf,
    worker: PathBuf,
    grant: Grant,
    request: AgentRunRequest,
    tools: BTreeMap<String, RegisteredTool>,
    #[serde(default)]
    evaluators: BTreeMap<String, CommandEvaluator>,
    #[serde(default)]
    cases: Vec<EvaluationCase>,
    #[serde(default)]
    policies: Vec<AdmissionPolicy>,
    #[serde(default)]
    attachment: AttachmentPolicy,
}

#[tokio::main]
async fn main() {
    if let Err(error) = command().await {
        eprintln!("{}", serde_json::to_string(&error).unwrap());
        std::process::exit(1);
    }
}

async fn command() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") => println!(
            "Ribosome 0.1.0\n\n  ribosome init DIRECTORY\n  ribosome run CONFIG.json\n  ribosome host CONFIG.json\n  ribosome inspect DATABASE RUN_ID\n  ribosome ingest DATABASE EVENTS.json\n  ribosome records CONFIG.json RECORDS.json\n  ribosome search CONFIG.json QUERY.json\n  ribosome validate CONTRACT VALUE.json\n\nrun explicitly starts a supervised Pi worker. Ctrl-C cancels it.\nhost explicitly serves ribosome-host/1 over stdio until EOF or Ctrl-C.\nRepeat an interrupted run's unchanged config to reconcile and resume.\nConfiguration and evidence are JSON. Diagnostics go to stderr."
        ),
        Some("init") => {
            let directory = PathBuf::from(argument(&args, 1)?);
            std::fs::create_dir_all(&directory)?;
            let path = directory.join("ribosome.json");
            if path.exists() {
                return Err(Error::conflict("ribosome.json already exists"));
            }
            let cwd = std::env::current_dir()?;
            let workspace = directory.canonicalize()?;
            let node = node_executable()?;
            let configuration = json!({"workspace":workspace,"state_dir":workspace.join(".ribosome"),"node":node,"worker":cwd.join("packages/agents/dist/worker.js"),"grant":{"id":id(),"scope":{"client":"local","project":"example"},"mode":"observe","paths":[],"tools":[],"profiles":["caretaker","curator","experimenter"],"budget":{"max_calls":12,"max_tokens":"1000000","max_cost_microusd":"1000000","max_actions":20,"max_work_items":4,"max_depth":2,"deadline_ms":(now_ms()+3600000).to_string()},"context":"local-project","visible_splits":["development"],"allow_export":false},"request":{"run_id":id(),"profile":"caretaker","operator":"proofreading@1","prompt":"Inspect the available evidence and report what requires attention.","provider":"openai","model":"your-model-id"},"tools":{}});
            std::fs::write(&path, serde_json::to_string_pretty(&configuration)?)?;
            println!("{}", path.display());
        }
        Some("validate") => {
            validate(argument(&args, 1)?, &read_json(argument(&args, 2)?)?)?;
            println!("{{\"valid\":true}}");
        }
        Some("ingest") => {
            let store = Store::open(argument(&args, 1)?)?;
            let events = read_json(argument(&args, 2)?)?;
            let array = events
                .as_array()
                .ok_or_else(|| Error::invalid("events file must be an array"))?;
            let mut inserted = 0;
            for event in array {
                let event: Event = decode("Event", event.clone())?;
                inserted += usize::from(store.ingest(&event)?);
            }
            println!(
                "{}",
                json!({"inserted":inserted,"duplicates":array.len()-inserted})
            );
        }
        Some("inspect") => {
            let store = Store::open(argument(&args, 1)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&store.inspect_run(argument(&args, 2)?)?)?
            );
        }
        Some("records") => {
            let config: RunConfig = serde_json::from_value(read_json(argument(&args, 1)?)?)?;
            std::fs::create_dir_all(&config.state_dir)?;
            let store = Store::open(config.state_dir.join("ribosome.db"))?;
            store.register_grant(&config.grant)?;
            let input = read_json(argument(&args, 2)?)?;
            let submissions = input
                .as_array()
                .ok_or_else(|| Error::invalid("records input must be an array"))?;
            let mut saved = Vec::new();
            for submission in submissions {
                saved.push(store.submit(
                    &config.grant,
                    &decode("RecordSubmission", submission.clone())?,
                    false,
                )?);
            }
            println!("{}", serde_json::to_string_pretty(&saved)?);
        }
        Some("search") => {
            let config: RunConfig = serde_json::from_value(read_json(argument(&args, 1)?)?)?;
            let store = Store::open(config.state_dir.join("ribosome.db"))?;
            let query: SearchRequest = decode("SearchRequest", read_json(argument(&args, 2)?)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&store.search(&config.grant, &query)?)?
            );
        }
        Some("run") | Some("host") => {
            let config: RunConfig = serde_json::from_value(read_json(argument(&args, 1)?)?)?;
            if config.request.checkpoint.is_some() {
                return Err(Error::invalid(
                    "checkpoints are restored from Rust storage; omit request.checkpoint",
                ));
            }
            std::fs::create_dir_all(&config.state_dir)?;
            let host = LocalHost::new(&config.workspace, config.tools)?;
            let store = Store::open(config.state_dir.join("ribosome.db"))?;
            store.register_grant(&config.grant)?;
            let mut runtime = Runtime::new(store, Box::new(host), &config.state_dir)?;
            for (name, evaluator) in config.evaluators {
                runtime
                    .laboratory
                    .register_evaluator(name, Box::new(evaluator))?;
            }
            for case in config.cases {
                runtime.laboratory.register_case(case)?;
            }
            for policy in config.policies {
                runtime.laboratory.register_policy(policy)?;
            }
            let supervisor = Supervisor::new(runtime, 1)?;
            let mut environment = BTreeMap::new();
            let keys: &[&str] = match config.request.provider.as_str() {
                "openai" => &["OPENAI_API_KEY"],
                "anthropic" => &["ANTHROPIC_API_KEY"],
                "azure-openai-responses" => &[
                    "AZURE_OPENAI_API_KEY",
                    "AZURE_OPENAI_BASE_URL",
                    "AZURE_OPENAI_RESOURCE_NAME",
                    "AZURE_OPENAI_API_VERSION",
                    "AZURE_OPENAI_DEPLOYMENT_NAME_MAP",
                ],
                _ => return Err(Error::invalid("unsupported provider")),
            };
            for key in keys {
                if let Ok(value) = std::env::var(key) {
                    environment.insert((*key).into(), value);
                }
            }
            let worker = WorkerConfig {
                node: config.node,
                worker: config.worker,
                environment,
            };
            let (cancel, receiver) = tokio::sync::watch::channel(false);
            let signal = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    let _ = cancel.send(true);
                }
            });
            if args[0] == "host" {
                let host = AttachmentHost::new(
                    Store::open(config.state_dir.join("ribosome.db"))?,
                    supervisor,
                    worker,
                    &config.grant.id,
                    config.request,
                    config.attachment,
                )?;
                let result = host
                    .serve(
                        tokio::io::BufReader::new(tokio::io::stdin()),
                        tokio::io::stdout(),
                        receiver,
                    )
                    .await;
                signal.abort();
                return result;
            }
            let provider = config.request.provider.clone();
            let model = config.request.model.clone();
            let result = supervisor
                .run(&worker, &config.grant.id, config.request, receiver.clone())
                .await?;
            let followups = if matches!(
                result.disposition,
                Disposition::Completed | Disposition::Abstained
            ) {
                supervisor
                    .drain_work(&worker, &config.grant.id, &provider, &model, receiver)
                    .await?
            } else {
                vec![]
            };
            signal.abort();
            let mut output = serde_json::to_value(&result)?;
            output["follow_up_runs"] = serde_json::to_value(followups)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
            if !matches!(
                result.disposition,
                Disposition::Completed | Disposition::Abstained
            ) {
                std::process::exit(2);
            }
        }
        _ => return Err(Error::invalid("unknown command; use ribosome --help")),
    }
    Ok(())
}

fn argument(args: &[String], index: usize) -> Result<&str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| Error::invalid("missing argument; use ribosome --help"))
}
fn read_json(path: impl AsRef<Path>) -> Result<Value> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::invalid("JSON input exceeds 16 MiB"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn node_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("RIBOSOME_NODE") {
        return Ok(PathBuf::from(path).canonicalize()?);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join("node");
            if candidate.is_file() {
                return Ok(candidate.canonicalize()?);
            }
        }
    }
    Err(Error::invalid(
        "node was not found on PATH; set RIBOSOME_NODE to its executable",
    ))
}
