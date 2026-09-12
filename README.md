# Ribosome

Ribosome is a local library for agent maintenance, prepared procedure reuse, scoped memory, and controlled experiments. TypeScript uses Pi agent-core for investigation and decisions. Rust owns durable state, grants, effects, evaluation execution, and recovery.

The original v0.1 qualification passed 73 automated tests and 13 live cases using a locally configured model, including all three demonstrations and a separate installed host. The attachment delivery passes 99 automated tests and 15 installed-consumer tests, with two additional live model demonstrations. Infrastructure tests and live model evaluations have separate evidence; these runs do not establish general model reliability. See [validation status](docs/validation.md) and the [source specification](docs/specification.md).

Ribosome can now attach to an application-owned agent harness at startup or during execution. The generic client and Pi adapter provide durable observations, scoped findings, optional steering and cooperative checked repairs. See the [integration guide](docs/attachments.md), [operation and recovery](docs/operations.md), and [delivery plan](docs/runtime-integration-plan.md). Memory retrieval improvements and experiments comparing complete agent executions remain follow-on work.

## Build and check

The tested toolchain is Node **26.4.0**, Rust **1.96.0**, and Pi agent-core/Pi AI **0.85.1**. Node 22.19 or newer satisfies Pi's package requirement; this repository's CI uses the pinned version. No Python, aec-bench, broker, network listener, or external database is required.

```sh
npm ci --ignore-scripts
npm run check
```

The check regenerates contract expectations, builds both packages, runs cross-language and Pi integration tests, checks Rust formatting and Clippy, and runs Rust tests. The Node tests build the Rust executable themselves. `npm run format` formats Rust. `npm run test:installed` installs into a new temporary consumer and runs attachment conformance there without model calls. Build manifests and lockfiles pin the dependencies.

To install the built components into a separate host project, choose an existing absolute consumer directory:

```sh
cargo install --path crates/ribosome-cli --locked --root /path/to/consumer
npm pack --workspace @ribosome/agents --pack-destination /path/to/consumer
npm install --prefix /path/to/consumer /path/to/consumer/ribosome-agents-0.1.0.tgz
```

The executable is `consumer/bin/ribosome`. In a consumer's run configuration, set `node` to the Node executable and `worker` to the installed `@ribosome/agents/worker` export. Resolve that export from the consumer with `import.meta.resolve('@ribosome/agents/worker')` and convert its file URL to a path. The workspace, state directory and registered host tools belong to the consumer; they do not need to reference this checkout. `ribosome init` supplies a checkout-relative worker location that must be changed for an installed package.

## Run the local reference host

Set `RIBOSOME_MODEL` to a model ID in the pinned Pi catalogue and configure the selected provider credentials. The npm scripts load the root `.env`; shell variables take precedence. See [`.env.example`](.env.example) for a local configuration template. Run `npm run provider:check` after building to check setting presence without a model call.

```sh
npm run build
cargo build --workspace --locked
node --env-file-if-exists=.env examples/local-project/prepare.mjs .ribosome/reference A
```

Preparation executes the host's normalizer and checker, records their observations, then introduces a relevant edit after validation. It writes a config and evidence for two worker roles and a planner. It makes no model calls. If `RIBOSOME_MODEL` is absent during preparation, `request.model` contains `your-model-id`; replace it before running. `ribosome init` also uses this placeholder.

After exporting the provider settings into the shell and deciding a spending limit (the Rust executable does not load `.env`):

```sh
# OPENAI_API_KEY must already be configured in this shell.
target/debug/ribosome run .ribosome/reference/ribosome.json
```

The reference grant reserves at most **US$1** of model usage across its runs and follow-ups, with a ten-minute deadline. Edit the generated grant before its first use to change that limit. Grant IDs are immutable once stored. Prices and usage come from the pinned Pi provider adapter; unknown usage retains its reservation. These are local accounting limits, not a provider billing guarantee.

Only the selected provider's settings are passed to the maintenance worker: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or the selected provider settings in [`.env.example`](.env.example). Registered host commands and reference preparation tools receive no provider credentials. The library does not discover CLI sessions, OAuth files, or a second TypeScript session database.

To run the complete live demonstrations:

```sh
npm run demo -- A
npm run demo -- B
npm run demo -- C
# All three demonstrations, at most US$3 in aggregate:
npm run test:live -- --cases A,B,C
# Demonstrations plus nine semantic cases, at most US$12 in aggregate:
npm run test:live
# Additional generated-development study, at most US$1:
npm run test:live -- --cases generated-development
```

Use `RIBOSOME_PROVIDER=anthropic` with `RIBOSOME_MODEL` set to a model in the pinned Pi catalogue for Anthropic. The provider defaults to OpenAI; a model must be configured explicitly. The runner fails explicitly when credentials are absent. It has no scripted fallback. Reports retain actual host receipts and independently check the final report and preserved cost analysis.

Demonstration B requires live curation, protected evaluation and admission before reuse. C uses that same admission path, consolidates scoped memory, and restores a report after an actual source revision. The semantic cases add benign edits, incomplete evidence, known/novel motifs, malicious observations, false-positive memory, incompatible transfers, an unavailable required check, and a source change during repair. See [evaluation cases](tests/evaluations/README.md) for commands and the observable acceptance criteria.

## Stop, resume and inspect

Ctrl-C requests cancellation, stops further agent work, then terminates an unresponsive worker. The grant deadline follows the same path. A running host command remains bounded by its own timeout and the root deadline. Already dispatched effects must settle or be reconciled; cancellation is not a rollback.

```sh
target/debug/ribosome inspect .ribosome/reference/.ribosome/ribosome.db RUN_ID
target/debug/ribosome run .ribosome/reference/ribosome.json
```

Repeat an **interrupted** run's unchanged config to restore its Rust checkpoint and reconcile receipts. Completed, failed, cancelled and exhausted runs are terminal. Starting new work requires a new run ID. Expired grants cannot be renewed by changing the stored grant or restoring messages. The owner must inspect the previous result and explicitly grant new work.

Inspection reports dispositions, effects, checkpoint metadata, visible message counts, tool names and retrieval targets, recorded model usage and unknown usage counts. It does not claim that missing usage was zero.

## Library boundaries

| Component | Public entry points | Responsibility |
| --- | --- | --- |
| Rust store | `Store::open`, `register_grant`, `ingest`, `submit`, `search` | SQLite/FTS5, evidence, scoped/versioned records and indexes |
| Rust effects | `Runtime`, `HostAdapter`, `LocalHost` | Bound tools, version checks, receipts, branches and reconciliation |
| Rust attachment host | `AttachmentHost::new`, `serve` | Explicit host lifecycle, source binding, durable feedback and cooperative repair |
| TypeScript attachment | `AttachmentClient`, `attachPi`, `WriteCoordinator` | Existing harness hooks, feedback delivery and external writer coordination |
| Rust execution | `Supervisor::run`, `drain_work` | Bounded workers, cancellation, recovery and follow-up delivery |
| Rust event routing | `subscribe`, `poll_subscription` | Host-owned kind filters, size/time batches and durable cursors |
| Rust laboratory | `Laboratory`, `Evaluator`, `AdmissionPolicy` | Matched arms, isolated memories, protected observations and fixed admission |
| TypeScript agents | `createAgentExecution`, `RpcPeer`, `operators` | Pi integration, typed tools, caretaker/curator/experimenter capabilities |

Construct `Runtime` after acquiring the host adapter. Runtime construction marks orphaned running work as interrupted. Register the grant and start/dispatch work after construction. Importing a library starts no daemon. Hosts call subscription polling from their own event loop and may use the same tools at selected boundaries or from embedded planners.

`Supervisor::runtime()` returns an `Arc<tokio::sync::Mutex<Runtime>>`; acquire it with `.lock().await` in async host code. Capacity waits and worker handshakes accept cancellation and respect the root deadline.

The operator catalogue covers proofreading, excision repair, motif discovery, extraction, recombination, chaperoning, regulation, memory consolidation, experiments and regeneration. Each operator pins its instructions, allowed output contracts and completion conditions. Rust does not contain a semantic diagnosis tree.

Recombination reads prepared inventory and recipient artifacts; its tool set omits raw source-history retrieval. Dispatch a separately budgeted curator investigation when source-history work is needed. Regeneration saves obligation records for restored and unresolved properties, with fresh receipt evidence and owners.

`contracts/schema.json` is the canonical wire definition. `npm run generate` creates the Rust types, TypeScript types and packaged runtime schemas. [Protocol documentation](docs/protocol.md) describes framing and recovery.

## Host capabilities and limits

The local adapter accepts exact relative file paths and rejects symlinks and traversal. It edits UTF-8 files with version preconditions and atomic replacement. Branches are separate copies of granted files, not Git branches. Checks and effectful procedures are explicitly registered executables with fixed arguments. Arbitrary model-supplied shell commands, filesystem paths, database methods and credentials are not exposed.

`grant.paths` grants reads. Set `grant.writable_paths` to restrict edits to a subset (omitting it uses `paths` for both). The reference source is read-only. `grant.required_checks` fixes mandatory application checks: writes must occur in a branch, and application runs those checks even if the agent's intervention omits them. Registered tools declare `reads`, `writes`, and the artifacts they actually `validate` through the `validates` field.

Applying one artifact requires the checker's other inputs to match the live recipient, including inputs omitted from the agent's proposal. A branch cannot validate against changed source bytes and then apply only its report to an unchanged live source.

Local commands and the maintenance worker are **trusted processes**. Process separation, a copied directory and an allowlist are not an OS sandbox. A host executing hostile generated code must provide a suitably isolated adapter/evaluator. The local adapter rejects code implementations; prepared implementations are agent instructions or host-registered procedures. External publication and network effects require a different explicitly supplied host capability.

One local host owns a workspace lock. Its workers share serialized effect dispatch. Atomic replacement and rereading preconditions detect cooperative version conflicts; an unrelated external writer can still race a multi-file validation/application. Hosts requiring strict cross-process atomicity must implement it in their adapter.

An experimenter cannot write evaluation or admission records through its tools. A host registers cases, evaluator code and acceptance policy. Protected cases are not exposed to the agent; the agent receives the aggregate decision and opaque evaluation references. Admission requires the complete matched evidence and the same frozen policy. Missing results remain inconclusive. A persisted cell archive retains the strongest accepted measurement in each host-defined descriptor cell.

A development-only policy can allow agent-generated cases with source mechanism, synthetic provenance and proposed checks. It retains the host's evaluator and required checks and cannot admit a candidate. Starting memory is resolved and frozen by Rust, with separate evaluator namespaces for arms and repetitions. See the [laboratory contract](docs/protocol.md#evaluation-and-persistence).

Memory expiry, retirement and deletion invalidate retrieval entries and derived records, including evidence copied into visible Pi messages. Those messages carry role attribution and source lineage; they are not host effect receipts. Scope is enforced before records, events or artifacts enter context. Training exports preserve origin and provenance and reject evaluation/holdout lineage. Observed, reexecuted and synthetic material cannot be relabelled by the export tool.

The local dependency graph holds at most 1,000 edges and 256 KiB per client/project. Capacity failures are explicit. Updating an edge replaces its current artifact versions; it does not silently retain obsolete versions. Oversized evidence views return an error so the caller can request a smaller window.

The source repository is [TheodoreGalanos/ribosome](https://github.com/TheodoreGalanos/ribosome). The npm workspace and Rust crates remain unpublished; build or install them from this repository. Publishing the GitHub repository does not publish packages to npm or crates.io.
