# Operate the local attachment host

The consuming application owns `ribosome host CONFIG.json`. There is no listener, background daemon, auto-discovered agent or implicit startup on import. The host acquires the local workspace lock before opening/migrating state. Use one owner per workspace. Supervisor has separate state and executor connections to the same SQLite WAL database; attachment ingestion/status retains its own connection. A command or evaluator holds the executor while independent worker evidence reads, checkpoints and provider permits use the state connection.

Register host tools, evaluators, cases and policies before constructing `Supervisor`. Host adapters and evaluators implement `Send + Sync`; the configured laboratory is shared and rejects registration while shared. Supervision requires a persistent database. Creating its service connection does not run migrations or orphan recovery. `Runtime::new` performs host-startup recovery after workspace ownership is acquired; do not use it to open an additional service connection.

For Rust embedding, use `Supervisor::runtime()` for short state work and `Supervisor::executor()` for effects, experiments and reconciliation. Never acquire the executor while holding the state runtime or a SQLite write transaction. An executor may commit short prepare/capture/finalize transactions; it does not retain a write transaction during a command or evaluator call. All workspace effects share that executor and recheck authority/versions after acquiring it. Reads of changing artifact bytes still require the existing version/snapshot checks.

Dropping the future returned by `Supervisor::run` requests cancellation of the owned run. The run keeps its worker slot and issued execution handles until they settle. Queued requests can be cancelled without waiting for another command. Worker pipe loss and cancellation stop new dispatch, signal running host work and drain its handles before the run becomes terminal. A custom evaluator that ignores cancellation retains ownership until it returns; a timeout on the caller is not proof that it stopped. Host shutdown must await owned work before dropping the Tokio runtime. The one-second independent-request target is a local regression bound, not a production latency guarantee.

## Inspect execution timing

`ribosome inspect DATABASE RUN_ID` returns `timings` beside `budget` and `model_usage`. Each measured span has `samples`, `total_us` and `max_us`; duration strings use microseconds. `worker_queue` measures waiting for a worker slot, `worker_startup` measures process launch through handshake, and `child_wait` measures a parked parent's wait. Startup samples include failed handshakes that reach cleanup. A process-launch failure may have no sample.

For worker tool requests, `state_queue` and `executor_queue` measure waiting for the appropriate runtime; `blocking_queue` measures scheduling onto the blocking pool. `state_service` and `executor_service` measure the call itself. `sqlite_write_begin` measures time spent acquiring immediate write transactions during that call, including failed attempts and transaction-start overhead. Its sample count is instrumented tool calls, not SQL transactions. These spans overlap: do not add them to derive wall time, and do not add a parent's child wait to child execution time.

`provider_round_trip` uses persisted wall-clock milliseconds from permit dispatch to the first usage report. It includes bridge/reporting delay; it is not pure provider inference time. `observed_calls` includes incomplete usage reports. `unobserved_dispatched_calls` counts dispatches without reports; `missing_dispatch_time_calls` identifies legacy calls without a measured start. `clock_reversals` flags negative wall-clock differences, which contribute zero duration. Budget inspection separately retains unknown usage and its reservation. Repeated usage settlement does not move the timing boundary.

Timing persistence is best effort. A failed sample does not replace a tool result or cause an effect retry. Missing spans mean unobserved time, not zero work. Aggregates survive restart; direct library operations outside Supervisor do not acquire worker spans. Permit dispatch and usage timestamps are recorded by the accounting APIs. Schema 18 leaves old timestamps absent.

## State and limits

`state_dir/ribosome.db` stores grants, events, subscription cursors, work, runs, checkpoints, records, effect receipts, attachment bindings and feedback. `state_dir/branches` and `state_dir/exports` hold filesystem artifacts. SQLite owns Ribosome's durable state; the TypeScript client keeps only pending transport operations, source subscriptions and delivery callbacks.

Schemas 1–20 migrate transactionally to schema 21, preserving existing rows and indexing their declared source relationships. Before an existing database is upgraded, `Store::open` creates a consistent `VACUUM INTO` snapshot beside it, named `DATABASE.before-schema-VERSION-ID.db`. If the backup fails, migration does not start. Failed migration rolls back its new tables/columns; unknown newer schemas are rejected. Existing v0.1 `run` configurations remain valid. Do not open a migrated database with an older binary. The workspace lock is acquired by CLI execution, not by every possible direct `Store::open` caller; library hosts must acquire ownership themselves.

Host defaults are an eight-event or 200 ms batch, a 1,000-event unprocessed backlog, five-minute feedback expiry, 100 producer identities per attachment and 16 attachments per grant. Event batches permit 100 events and 256 KiB; protocol frames permit 1 MiB and transport queues 32 entries. `attachment` configuration can bound `event_kinds`, `batch_size`, `max_delay_ms`, `max_backlog` and `feedback_ttl_ms`. Declared capabilities do not widen the immutable grant. Model calls, tokens, costs, actions, work items and deadlines share that grant across its attachments.

Creating an attachment records source status as `unknown` until source activity establishes it. Status reports source coverage, subscription cursor, observed source status, actionable errors, writer ownership and feedback state counts. Usage reports calls, observed cost and unknown/incomplete calls; it is grant-wide. No external-source usage is inferred. A local file lock does not coordinate arbitrary external tools; use the documented writer handoff for shared repair.

Registered-tool identity includes its configuration, executable contents and absolute regular-file arguments. Use optional `code_files` (at most 32 absolute paths) for additional checker code or configuration dependencies. Each fingerprinted file must be a regular file no larger than 256 MiB; hashing streams the bytes. Relative imports, inherited environment and remote state are not discovered automatically. Declare relevant checker dependencies and task input paths; this is a cooperative host guarantee, not a hermetic execution claim. Changing checker code makes earlier certificates ineligible for transfer, even if the command still exits successfully. Inputs in `reads` and targets in `validates` remain separate.

## Assign discovery evidence

Register a `DiscoveryCorpus` through `Store::register_discovery_corpus`, then set `discovery_corpus` on the curator run request or root grant. The CLI `run` configuration also accepts `corpora`, registered before the run starts. Referenced events, definitions and artifact snapshots must already exist in that database. Use a separate owner/source run to capture them. The curator reads assignment metadata with `evidence.corpus` and retrieves the selected evidence through normal tools.

The assignment selects event windows, artifact versions, definitions and dependencies. Keep future outcomes out of online prompts as well as the selected evidence. Assigned runs are observation-only; their curator children inherit the assignment and root budget. Artifact reads use retained historical snapshots, and source withdrawal still takes precedence. Record submission remains available for discoveries and hypotheses. Effects or experiments need separate host-authorized work.

New discovery definitions include their functional contract and occurrences include grounding. The curator selects evidence and interprets it; Rust can supply omitted run identity and event sequence frontiers. A bounded contrast child uses `contrast-motif@1` through the existing work queue. Its saved investigation is an interpretation to review, not an admission or independent proof of utility.

Prepare the R4 development corpus with `node tests/evaluations/r4-discovery.mjs --prepare .ribosome/r4-discovery-qualification`. Preparation executes actual donor file/tool operations across twelve authored episodes and makes no model calls. The source events identify that limitation; the assessor's family/variant manifest is not supplied to the curator. Run an authorized live trial with `node --env-file=.env tests/evaluations/r4-discovery.mjs --run .ribosome/r4-discovery-qualification`. It has one 100-call, US$1.50 aggregate allowance. `--retry-after-fix` uses a fresh corpus assignment under the same grant, deadline and remaining allowance after inspection of a completed attempt. `--complete` instead retains the existing candidate and contrast records and asks a curator to finish the investigation. Reports retain prior attempts and distinguish saved records from semantic assessment. Raw configurations and provider output stay in the ignored trial directory.

## Wait for child maintenance

A maintenance agent can request child work, call `work.wait` with its direct child IDs, and inspect completion through `work.status`. Pi finishes the current tool exchange before handing a coherent checkpoint to Rust. The supervisor records the wait, stops and reaps the parent worker, and releases its slot. The existing Rust queue then runs the children, including at worker capacity one. Once they settle, the same parent run resumes from its saved context. There is no additional TypeScript work queue.

Run inspection reports `waiting_on`. A wait remains durable through interruption and host restart; resume the original run with its original configuration and allocation. Its unfinished children run before the parent's next provider call. If a child run completed before its work-item status was saved, restart reuses the saved result and does not launch that child again. A queued child held by a parent wait is excluded from unrelated queue drains. An already-claimed child retains its execution owner.

Cancellation or deadline exhaustion stops queued children and closes their allowances. Cancellation also signals an already-running child owned by another scheduler on this supervisor. That owner retains the worker, command and workspace fence until settlement; a stopped parent does not establish that the child has stopped. Usage for already-dispatched provider calls can still settle. Coordinated repair cannot park while it holds a writer handoff.

Schema 17 adds durable parent waits and task-source references. Each new child's task inherits its requesting context and evidence references. Withdrawal prevents that task from entering a provider request; `work.status` also withholds a completion whose source is unavailable. Legacy work rows retain their bytes but have no verified task lineage and cannot execute automatically. The owner must review their evidence and issue a fresh authorized request. Deploy Rust and Pi workers together: both require `context.parking/1`.

## Inspect and recover budget usage

Run inspection includes `budget` with its allocation ID, parent, causal ID, purpose, subtree usage and effective remaining capacity. `model_usage` separately reports undispatched reservations, dispatched unknown calls and released calls. Released reservations do not consume remaining budget or appear as active calls. Attachment usage remains grant-wide and does not infer external-source costs.

Allocate child ceilings through the host-only `Store::allocate` API and pin their identity in `AgentRunRequest.parent_allocation_id`. Use the original allocation and provider call IDs when recovering. Do not replace a dispatched call with a fresh permit merely because its response was lost. Only `model.release` on a provably undispatched permit may restore capacity. Outstanding liabilities are retained when a run or study finishes. The permit ledger is accounting enforcement, not a guarantee about provider invoices; in-flight requests may still incur charges after cancellation.

To include an external source agent in a measured treatment, its trusted host transport must participate in accounting. Create its allocation beneath the same root, bind its run to that allocation, and call `Store::permit` with a stable call ID before dispatch. Call `Store::dispatch_permit` before contacting the provider, then `Store::usage` with the adapter's observation. Maintenance and evaluator calls consume the remaining root capacity. Attaching an event observer alone does not instrument the source provider or make its costs known.

A registered evaluator uses its case-scoped `EvaluationAccount` for permits, dispatch, usage and release. Only an adapter that meters every provider call (or makes none) can report `provider_usage_is_metered() == true`. The built-in `CommandEvaluator` cannot account for arbitrary subprocess model calls and leaves `usage_complete` false. The whole-agent adapter uses the shared accounting path for Pi subjects and maintenance stages. External contrast agents and provider-backed semantic judges must also meter their calls before a study can claim complete usage.

Schema 16 preserves all legacy provider observations without inventing call IDs or role attribution. Known usage remains settled; incomplete legacy usage remains a root liability that cannot be released as undispatched. The current Pi worker requires `budget.allocations/1`, so deploy matching Rust and TypeScript builds. See [the protocol](protocol.md#shared-resource-accounting) for evaluator instrumentation and matrix completion semantics.

## Finish, stop and reconnect

Call `finish()` after the source execution ends to flush remaining observations and await final feedback. It leaves the attachment connected for another source turn or an explicit repair. `detach()` unsubscribes the connector, cancels pending maintenance and requests cancellation of active maintenance; dispatched effects settle through Supervisor. The source agent is not aborted. `client.close()` also closes an owned host's stdin and waits for shutdown. A normal finished detach records `completed`; an early detach records `detached`. Both are terminal attachment identities.

EOF, Ctrl-C, a failed output pipe or a worker failure stops/cancels work and retains its outcome. On host restart, nonterminal connections are interrupted until reopened with the same attachment ID, execution ID, connector identity, capabilities and original grant. Use persisted producer sequence positions to retry exact event identities. Do not silently declare missing source observations complete. A source host needs its own history to recover events that Ribosome never acknowledged.

Pending ordinary advice survives restart; previously delivered advice can be redelivered with its stable ID. Delivery and steering recheck the producing run's source availability, including sources inherited by its summary. Withdrawn advice is withheld, and deletion cleanup redacts the stored feedback copy. Legacy feedback without verified run-result lineage is not delivered. Unknown steering outcomes stay unknown. Completed maintenance whose feedback publication was interrupted is recovered from its saved result. Checkpoints restore the maintenance session, not the external agent or workspace.

An expired grant denies further events, model calls and effects. Status and receipt inspection remain available. A new budget requires explicit owner-issued work and a new grant identity. Do not edit the stored grant or reconnect with a fresh identity to evade an exhausted allowance.

An effect in `observed` phase has a captured adapter outcome awaiting finalization. Receipt lookup or run recovery retries its receipt/event/dependency transaction without executing the action again. A `dispatching` effect without an outcome remains uncertain. Matching current file bytes produce an unknown receipt with `current_postcondition_observed`, not proof of execution or checks. New live writes remain denied while an issued effect is unsettled or unknown. Use the owner settlement interface below after establishing that the executor has stopped. Receipts retain check certificates and explicitly validated properties; inspect `validations`, `restored_validity` and `restored_properties` separately from execution status. A completed application whose delayed finalizer finds changed inputs, code, policy or intervention is applied but unverified and requires fresh authorized checks. Legacy success labels do not gain execution evidence or property certificates during migration.

During a writer handoff, timeout/disconnect never releases the external writer automatically. Keep it stopped; inspect and reconcile receipts using the original generation. Unknown outcomes require owner investigation. Aborting an agent is not proof that a tool or descendant process stopped.

## Settle an executor with an unknown outcome

Keep external writers stopped while the outcome is unresolved. Stop maintenance and establish that the original executor and any processes it started have stopped. The local host lock excludes another cooperative Ribosome host; it does not establish that an orphan process or arbitrary external writer has stopped.

Inspect the original effect and current workspace without starting a model:

```sh
ribosome effect /absolute/config.json OPERATION_ID
```

Construct a settlement from the returned `receipt_version` and complete `workspace_versions`:

```json
{
  "operation_id": "OPERATION_ID",
  "expected_receipt_version": "RETURNED_RECEIPT_VERSION",
  "executor_stopped": true,
  "workspace_versions": [{ "path": "report.txt", "version": "RETURNED_ARTIFACT_VERSION" }],
  "reason": "Describe how the owner established executor stoppage and inspected its outputs.",
  "source_refs": []
}
```

Include every path returned by inspection. Then run `ribosome settle /absolute/config.json /absolute/settlement.json`. The command acquires local host ownership and does not start a model. For an already connected harness, use `AttachmentClient.inspectEffect(operationId)` and `settleEffect(request)` over the owner protocol. Stop the maintenance run first; a running owner or busy runtime cannot release the fence. If a snapshot is stale, inspect again and review the resulting state before submitting a new decision. Retrying the same successful decision returns its historical result.

Settlement preserves the original unknown outcome and records the owner decision separately in `effects.settlement` and an `effect_settled` event. It invalidates possibly affected live artifacts and consumers; it does not establish that the old action or its checks ran. Reinspect the current artifacts and obtain fresh authorized checks. Resume an interrupted run with its unchanged configuration while its grant remains valid. For a coordinated repair, settle every unknown effect before `reconcileRepair` can release the handoff. Settlement remains available after expiry through the CLI, but cannot extend the grant or authorize more work. Scope checks and original receipt identity still apply.

## Configure property validation

A registered checker retains its `reads` and `validates` declarations. To certify a semantic property, the host also supplies `validated_properties`, for example:

```json
{
  "validated_properties": [
    {
      "obligation": { "id": "report-reconciliation", "version": "1" },
      "path": "report.txt"
    }
  ]
}
```

The referenced Obligation must exist in the grant's scope, be available to the run, and declare the target in `affected_outputs`. The target must also be in the checker's `validates` and `reads`. Bind only criteria the actual checker establishes. An agent cannot add coverage through an action payload. Revising the Obligation or checker requires fresh authorization and a new check; a cached receipt remains a historical observation. Checks do not rewrite the Obligation's authored state.

Use `artifact.validity` (or `Runtime::artifact_validity`) for current per-property assessments. A successful effect can leave some properties unproven or stale. Aggregate artifact invalidation clears only when all declared scoped properties have current supporting evidence; consumers need their own checks. `property_validations` retains the supporting effect identity, obligation/artifact versions and generation. `artifact_invalidation_generations` keeps the latest known invalidation even after the pending stale flag clears. These are current support records, not a record of unobserved external filesystem changes. Version-only validity snapshots contain no artifact text. Schema 14 creates no property proof for older receipts.

## Backup and restore

For a complete operational backup, stop the attachment host **and every external writer**. Let dispatched effects settle. Copy the entire `state_dir`, the workspace files it describes, and the non-secret host configuration into a new backup directory. Include `ribosome.db-wal` and `ribosome.db-shm` if present; do not copy just a live `ribosome.db`. Use an access-controlled backup location with the same privacy requirements as source observations and checkpoints. Provider keys stay in the environment.

SQLite's backup mechanism or `VACUUM INTO` provides a consistent database-only snapshot, including committed WAL content. For example, with a local SQLite CLI:

```sh
sqlite3 /absolute/state/ribosome.db "VACUUM INTO '/absolute/new-backup/ribosome.db';"
```

The destination database must not exist. A database-only snapshot does not capture branch files or freeze the external world. The tests restore this snapshot for inspection and verify scope, retirement and event positions; they do not claim a database copy restores external processes.

Restore into a separate inspection directory first. Verify `PRAGMA integrity_check`, compare receipts and artifact versions with the current workspace, and identify any steering/effects that could have happened after the snapshot. Do not attach a live application or automatically replay control from a restored snapshot. An older backup can also contain records retired or deleted after it was taken; reconcile those withdrawals before using it for retrieval.

For operational resumption, preserve the original configured workspace/state paths: saved branch paths are absolute. Restoring to different paths is an inspection workflow until the owner supplies a supported migration; there is no automatic branch-path rewrite. Resume only after the owner has reconciled external outcomes and ensured every writer remains controlled.

## Rebuild search and handle failed persistence

With all owners stopped, make a backup and rebuild the derived FTS5 index from current visible records:

```sh
sqlite3 /absolute/state/ribosome.db < scripts/rebuild-index.sql
sqlite3 /absolute/state/ribosome.db 'PRAGMA integrity_check;'
```

The [SQL script](../scripts/rebuild-index.sql) rebuilds transactionally, excludes retired and expired records, and keeps authoritative records unchanged. Scope and lineage checks still apply during retrieval. It does not recover damaged authoritative records.

A full disk or failed database write must produce an error, not an ingestion acknowledgement. A batch and its sequence frontier roll back together; routed work and cursor advancement also commit together. Stop the affected attachment, retain any unacknowledged source events in the source host, restore writable capacity, inspect integrity and reconnect explicitly. The fault tests inject rejected writes and migration failure; they do not fill this machine's disk.

Retirement/deletion removes retrieval access and derived support. It does not promise secure erasure of old WAL pages, external logs, backups or files copied outside managed storage. Apply the owner's retention policy to those copies. Automatic archival, database vacuum scheduling, backup scheduling and a remote service are outside this delivery.

Schema 3 commits a source tombstone and cleanup job with logical retirement. Record reads, search and source-reference checks deny unavailable ancestors even if physical derivative cleanup fails. `Store::source_cleanup_status(grant)` reports the first 100 scoped jobs, errors and discovered-node progress. Its `complete` flag describes pagination, not whether every job is finished. Continue with `Store::source_cleanup_status_page(grant, next, limit)` until the page is complete. `Store::cleanup_sources(grant, limit)` advances up to 100 jobs per call, with at most 100 traversal node/edge operations per selected job. A node can redact multiple copied context rows; this is not a byte or latency bound. Library hosts can invoke it after restart. Schema 7 stores each job's worklist and edge cursor in `source_cleanup_items`. Redaction and cursor advancement commit together; a failed item rolls back while earlier progress survives. New source edges reopen affected jobs, including for observations received after cleanup. Previously completed jobs are rechecked on migration. Protected evaluation evidence and admission records retain their existing policy. Development evaluation output and saved development study inputs are removed with deleted source lineage. Schema 4 adds the Rust-owned context ledger; schema 5 adds retained artifact read windows and their freshness requirements; schema 6 adds semantic summaries and their model-permit bindings. Schema 8 stores full tool results alongside artifact snapshots, using immutable run/call identities. Large results and oversized groups of inline results reach Pi as bounded excerpts or readable references. Retiring a source denies the result artifacts through their ancestry before cleanup clears the copied payloads. Current Pi continuations use bounded descriptors and reauthorize record/event/artifact ancestry before provider calls. Deletion jobs also redact copied context messages, semantic summaries and their later interpretations; failure remains observable and retrieval stays denied. Source edges retain ancestry after payload deletion. An authorized owner can escalate an already-retired record to deletion with `record.retire`, `delete: true` and its current `expected_version`; scope and protected-record restrictions still apply. Schema 9 adds source-aware message delivery and run-summary inspection/delivery. Deletion also clears legacy development checkpoint text, message bodies and completion summaries without inventing lineage. Protected and other-client copies remain retained. Schema 10 registers new export files as artifact snapshots with their full reviewed source closure. Schema 11 also preserves mutable-record ancestry in older record/event/result observations where it can be established. Ambiguous older copies remain retained but unavailable; migration does not assign current dependencies to an unproven historical observation. Source format 4 rebuilds unsupported context segments. Legacy copies with missing ancestry still need their scoped physical-retention policy; this migration is not erasure. Runtime binds their managed directory and retries unfinished publications and cleanup on startup. `Store::cleanup_sources` requires that Runtime binding to remove an export file; without it the job reports a failure and remains retryable. The managed `ribosome-export:` read path checks current source authorization and content integrity. The returned physical path remains an owner-held delivery copy until cleanup removes it. Host adapters must keep raw state directories outside granted task artifacts; raw filesystem access is not a source-authorized read. Interrupted legacy studies without workspace metadata leave cleanup failed until the owner confirms external cleanup. Backup and WAL retention require the owner's separate policy.


Pi compaction uses the same model configuration and root allowance as the maintenance run. Every summary call consumes a permit; incomplete calls retain their reservation. Rust stores raw context in `context_items`, typed observations in `context_sources`, and summary coverage, predecessors and text in `context_summaries`. Committed summaries also have scoped `pi-compactor` events and source edges. The worker fetches the current summary instead of trusting a stale checkpoint reference. Summary text is a model interpretation; operation receipts remain the authority for effects. A provider failure or an exhausted compaction budget stops continuation explicitly. See [the protocol](protocol.md) for limits and crash/withdrawal ordering.

## Remove legacy exports and evaluator workspaces

New exports use their recorded source closure for cleanup. Older exports have no such manifest. Opening state preserves those files. On a later source deletion, cleanup removes recognizable legacy JSONL exports in the same client/project rather than assigning them an unverified historical lineage. This can remove an independent legacy export in that scope. Regenerate needed products from current authorized records. Other scopes, symlinks, and unrecognized files are left untouched. Inspection is bounded at 16 MiB per unregistered file; a larger file leaves cleanup failed with an owner-inspection message.

Evaluator workspaces are temporary. Before an evaluator writes, Rust saves the workspace path and owning run in the existing study configuration. Normal completion removes the temporary directory. Startup removes any remaining workspace for a completed study. An interrupted study can have a surviving external executor, so startup preserves its files and source cleanup reports unresolved ownership.

After the host establishes that the evaluator and every process it started have stopped, the Rust owner can call `runtime.settle_experiment_workspace(&grant, &experiment_id, true)`. The run must no longer be running or waiting. The method records that confirmation, removes the managed workspace and retries source cleanup. It does not invent evaluation results or release unknown provider usage. There is no model-facing cleanup tool. Legacy workspaces without registered ownership need owner inspection; their paths are not guessed from directory names.

Action receipt content inherits the source context captured before host dispatch. Withdrawal masks copied action text, output, side-effect descriptions and validation details in public lookup and run inspection; operation identity, status and outcome basis remain available for recovery. `content_available: false` identifies that projection. Deletion also redacts finalized receipt and observation copies. An unfinished effect retains the inputs needed for reconciliation and leaves cleanup failed until it can finish safely. This never replays or reverses a workspace edit. Legacy receipt content with unverified lineage is withheld; deleting a development source also conservatively removes legacy receipt copies from development-only grants in the same client/project and legacy development host-event payloads. This can remove an independent legacy copy. Protected evidence keeps its existing retention policy.

After a context rebuild, `continuation.read` can recover prior independent obligations, operation references and direct child-work references beyond the first page embedded in the clean segment. Records that are withdrawn or declared satisfied are omitted from obligation pages. Inspect referenced records, receipts and child status for actual outcomes. Reuse the evidence cursor only with the corresponding evidence selection; it does not establish global coverage.

For an interrupted legacy study without a recorded workspace path, cleanup reports that it cannot locate the copied files. The host owner must establish that its evaluators and descendants stopped, locate and remove their copied inputs, then call `Runtime::confirm_legacy_experiment_cleanup(&grant, experiment_id)`. This owner-only method records that attestation and retries cleanup; it does not discover paths, delete arbitrary directories, independently verify the external cleanup, invent an evaluation result, or settle unknown model usage. Use `settle_experiment_workspace` for a registered managed workspace. Completion of legacy cleanup therefore depends on the owner attestation.

The R1 live continuation qualification is separate from fixture tests. Prepare a new case with `node tests/evaluations/r1-continuation.mjs --prepare .ribosome/r1-continuation-qualification`. After authorizing its paid-provider allowance, run `node --env-file-if-exists=.env tests/evaluations/r1-continuation.mjs --run .ribosome/r1-continuation-qualification`. It uses the existing provider configuration, a shared $2 model-accounting ceiling, at most 100 calls and one automatic restart. Preparation makes no provider calls. The directory must be new; an already-started trial requires inspection before another trial. If the initial request failed because of connectivity or a rejected tool schema, fix that cause and use `--retry-rejected` with the same directory. This creates a new run under the existing grant, deadline and database; earlier reservations and diagnostics remain retained. It refuses to retry after a completed provider call or an effect. Its `qualification.json` records the current continuation result, prior failed attempts and aggregate usage. `run_usage_complete` applies to the current run; aggregate `usage.incomplete` and `unknown_reserved_cost_microusd` separately retain earlier unknown calls. Raw database/configuration files remain private under the ignored `.ribosome` directory. The qualification does not establish R2–R7 completion or production task performance.

Qualification host stdout/stderr tails are retained in `attempt-N-diagnostics.json` inside the private case directory. Do not publish those files: provider errors can include deployment settings. If evidence collection fails, the shareable `qualification.json` records an incomplete result with unknown usage and points to private diagnostics; `report-error.txt` retains the collection error for local inspection. Recover usage from host state before deciding on another trial.


For a small R1 content diagnostic, prepare with `node tests/evaluations/r1-summary-diagnostic.mjs --prepare .ribosome/r1-summary-diagnostic`, then run the approved trial with `node --env-file-if-exists=.env tests/evaluations/r1-summary-diagnostic.mjs --run .ribosome/r1-summary-diagnostic`. It compares fresh answers from original evidence and a Pi-authored summary using the same independent checker. All stages share one six-call, US$0.25 Rust grant and perform no effects. Preparation is offline. After inspecting and fixing an interrupted harness run, `--resume` retains the grant, deadline and prior usage. `--repeat-summary` retains a passed original-evidence answer and repeats the summary stages within the same remaining allowance. Reports separate infrastructure observations from model-answer accuracy; they do not claim autonomous long-run qualification.

## Execute prepared instructions

Select an available `instructions` implementation with `instruction_contract`. Add this fragment to the ordinary host-owned run request:

```json
{
  "profile": "caretaker",
  "operator": "execute-motif@1",
  "invocation": {
    "implementation": { "id": "saved-implementation-id", "version": "1" },
    "bindings": { "input": "recipient.json", "output": "result.json", "checker": "recipient-check" },
    "recipient_refs": [],
    "purpose": "experimental"
  }
}
```

Use the implementation's actual slot names. Grant only recipient paths and required registered tools. Experimental runs require `mode: "sandbox"`; use `purpose: "production"` only for an implementation admitted in the grant's context. The host controls this selection. Pi cannot enable experimental authority with a tool argument. Inspect the saved run request, receipts and output artifacts; `completed` remains the model's reported disposition. Material changes require a new candidate and do not inherit admission.

`Transplant.invocation_run` can link the actual execution and its bindings. Nested invocation is explicitly unsupported; schedule a separate scoped run from the host. Prepared reuse cannot fetch raw donor history through an alternative RPC. Donor-source retirement still invalidates prepared derivatives.

A bounded live development demonstration is available:

```sh
node tests/evaluations/r5-invocation.mjs --prepare .ribosome/r5-trial
node --env-file=.env tests/evaluations/r5-invocation.mjs --run .ribosome/r5-trial
```

Preparation runs authored donor policies locally. The live trial gives the curator and three recipients one shared 36-call, US$0.35 allowance. It uses the production Pi worker and a checker that cannot repair output. Private configuration and results stay in the trial directory. A curator can consume the shared allowance before later recipients execute; the report retains those exhausted outcomes instead of treating them as passes. Inspect saved policy, actual receipts and recipient results separately from the model's completion summary.

For retrieval, preserve the current default search or explicitly select `query_mode: "any_terms"`, `order: "relevance"`, `eligible: true` and an authored `function` category. Follow `next` with `after` and `offset: 0` until `complete`; restart when an index/source change invalidates the cursor. Legacy offset callers remain supported.

## Running whole-agent studies

Register `agent_evaluators` in the same host configuration used by `run` and `host`. An entry supplies provider/model settings and version labels, the subject's paths and tools, an offline `judge`, optional `secondary_judges`, and optional per-arm `stages`. Every case supplies public `input.subject` with `prompt`, `files` and `bindings`; protected oracle input remains outside that object. Stage implementations must already exist in the owner's scope. Store the policy and complete matrix before execution.

Run an existing experiment without spending a model call on orchestration:

```sh
ribosome study CONFIG.json EXPERIMENT_ID
```

The configuration's request must use `experimenter` / `experiment@1` with a new run ID. Ctrl-C cancels execution. The command returns the persisted `ExperimentResult`; a rejected comparison is still a completed study operation. Inspect `complete`, `usage_complete`, the decision and the report separately. Do not infer success from the command's exit code or a worker's summary.

A bounded qualification runner prepares donor observations, asks a live curator for instructions, then compares unchanged, retry, critique, ordinary-care and prepared-care workflows:

```sh
node tests/evaluations/r6-laboratory.mjs --prepare .ribosome/r6-trial
node --env-file=.env tests/evaluations/r6-laboratory.mjs --run .ribosome/r6-trial
```

The declared reduced matrix has two cases per family, two families, five arms and one repetition. The curator and evaluation runs share a 280-call, US$1 ceiling; each case has a 16-call allowance. These ceilings are not a promise that every case can consume its maximum. Private configuration, raw execution outputs and reports remain in the ignored trial directory. Inspect actual saved material and protected observations before making a behavioral claim.

A previously exposed set can be used for an explicitly labelled development recheck after an implementation fix:

```sh
node --env-file=.env tests/evaluations/r6-laboratory.mjs --development-recheck .ribosome/r6-trial
```

This retains the original grant and its liabilities. It creates development evidence, does not reset protected exposure, and does not request admission. Use new independent recipient families for a new protected qualification. The stock agent evaluator does not retain learned memory across cases and does not run provider-backed semantic judges. Use the host `Evaluator` interface when a study needs those capabilities, with all provider calls funded through its `EvaluationAccount`.

## R7 qualification and handoff

`node scripts/qualify-reliable-learning.mjs` runs the focused R7 selection and writes command logs plus a mechanical report to `.ribosome/r7-mechanical/`. It includes migrations, exports, shared accounting, protected exposure, admission, invocation, compaction with an interrupted effect, bridge failures and integrated attachment recovery. It does not run the full suite or call a paid provider. To run selected checks, pass an output directory followed by IDs, for example:

```sh
node scripts/qualify-reliable-learning.mjs .ribosome/r7-mechanical-followup current-store-upgrade qualification-reporting
```

`npm run test:installed` packs TypeScript, installs Rust into a separate temporary consumer, and runs attachment plus R7 recovery and prepared-invocation tests there. Set `RIBOSOME_INSTALL_REPORT` to an absolute JSON path to retain its consumer location, such as `$PWD/.ribosome/r7-installed.json`. Installation needs cached dependencies or registry access. It does not publish either package or execute a live model.

The separate [learned-behavior example](../examples/learned-behavior/README.md) is copied into that consumer as `learning-demo.mjs`. Explicitly prepare and run it after configuring provider access. The model may conclude that evidence is insufficient; no authored replacement policy is inserted to make qualification pass. Starting an attachment does not automatically run this learning pipeline.

Generate the sanitized evidence bundle with:

```sh
node scripts/qualification-evidence.mjs .ribosome/r7-evidence
```

The exporter reads the R7 mechanical/installed reports, the installed learning trial and the retained R6 development recheck. It selects fixed fields, omits transcripts/provider settings and preserves missing reports, incomplete matrices and unknown usage. Source commit and dirty-working-tree status are separate: a commit alone does not identify uncommitted implementation changes. Exact private provider configuration remains with the private trial, not the shared bundle.

Schema 20 upgrades transactionally to 21, preserving consumed protected families. A partial upgrade rolls back and a reopen succeeds after the conflict is removed. Unknown newer schemas are rejected. R7 adds no production schema or protocol change. Existing `ribosome/1` and `ribosome-host/1` negotiation and defaults remain in effect.

Recovery does not renew writer authority. If an attached application happened before final bookkeeping failed, recovery retains the single application outcome, but a lost handoff can prevent the old checks from restoring current validity. Reconcile the stopped writer, obtain fresh authorized checks, and inspect validity. Unchecked downstream work remains stale. See [qualification results](validation.md#r7-integrated-qualification-and-handoff).
