# Local bridge and recovery

Ribosome uses JSON-RPC 2.0 requests and responses, framed as one UTF-8 JSON value per newline. Stdout is protocol-only; diagnostics use stderr. The protocol is `ribosome/1`, the build is `0.1.0`, and the Pi adapter pin is `0.85.1`. Handshake mismatch fails before dispatch.

Both peers continue reading while a request is pending. Frames are limited to 1 MiB and active/outbound requests to 32 per peer. Artifact reads return at most 65,536 bytes per request; local artifacts are capped at 16 MiB. UTF-8 offsets are byte offsets. Requests have string IDs and never share their identity with an effect operation by implication.

Worker-capacity waiting accepts cancellation and is bounded by the root deadline. Handshake waiting is bounded by both its five-second timeout and the root deadline, and accepts cancellation before dispatching the agent.

Schema validation occurs on incoming arguments and outgoing results in both languages. Missing optional fields are omitted; `null` is rejected unless a schema explicitly permits it. Record schema version, protocol version, and motif/implementation version are separate. Large counters and millisecond timestamps use decimal strings. Unknown measurements omit `value`; they are not encoded as zero.

The supervisor binds the pipe to a run and its stored grant. Worker tool arguments have no scope or grant override. Unknown methods and extra fields are rejected. The trusted Pi adapter reads `session.grant` and supplies that authority as context; the model cannot call this method or replace its grant. Accounting, checkpoint and activity methods also belong to the adapter. There is no generic database endpoint. The record tool presents the selected operator's body schemas, generated from the same contracts that Rust validates.

After checkpointing, the adapter publishes complete visible messages through acknowledged `session.events` batches. These become `agent_message` evidence attributed to the originating role. Assistant statements and tool-result text do not replace host receipts. Provider reasoning is excluded from this view; non-text media remains in the checkpoint. Sequence identities make repeated publication after restart idempotent. Activity takes the most restricted split visible to its run and conservatively carries the IDs of earlier retrieved records and events. Retired, expired or deleted source memory makes that activity and derived records unavailable to retrieval. A worker's evidence tool excludes its own active message stream to avoid investigating its own output. Worker-submitted records and host receipts retain the most restricted split visible to that run; omitting source references cannot declassify output. Source-linked records also reject labels below the restriction of their sources. Opaque evaluation references released by the laboratory remain distinct from access to protected observations.

## Effects

1. The TypeScript tool wrapper derives a stable operation ID from the run and Pi tool-call ID.
2. Rust commits the intent before calling the host adapter.
3. The adapter checks capability and relevant versions and executes the operation.
4. Rust records a succeeded, failed, denied, stale or unknown receipt and an executor-observed event.
5. Pi receives the receipt as a tool observation and decides its next step.

A repeated operation ID with different arguments or a different grant is rejected. A repeated identical completed operation returns its existing receipt. Timeout or pipe failure is not authorization to retry it.

Settled receipts expose `evidence_ref`, the exact executor event ID accepted by record provenance. `operation_id` identifies the effect for lookup; it is not a source event ID. Event IDs use a bounded hash so a maximum-length operation ID can still produce valid evidence. Pending intents have no evidence reference.

Checked application verifies actual checker input versions against the live recipient, even when the agent omits them from its intervention. Only the selected artifact is applied; other checked branch inputs must already match live state.

For an edit with an intent but no recorded outcome, the local adapter compares the current artifact to the intended content. A matching result is recorded as reconciled, with that limited observation stated in the receipt. Otherwise the outcome is unknown and continuation stops. Non-edit operations without a known receipt remain unknown. Exactly-once external effects are not promised.

The worker checkpoints through Rust before and after tool transitions. On restoration, the adapter repairs incomplete tool-result sequences using host receipt lookup. It does not replay unacknowledged non-effect tools as if they had run. It preserves their missing-result status and lets the agent request fresh evidence. Unknown effects must be resolved by the owning host before continuation.

An acknowledged checkpoint restores Pi context, not workspace state. The pinned profile/operator/provider/model must match. Restored context cannot widen the original grant. A host restart marks orphaned running rows interrupted; a worker crash is recorded by its supervisor. At-least-once work leases have stable IDs, bounded redelivery, renewal, subject convergence and root-budget limits. A delivery-exhausted row is failed before scanning for the next item. The current lease owner may record a late completion unless another owner has reclaimed it.

Usage, checkpoints, visible activity and receipt lookup can settle after cancellation or a deadline. This does not permit additional model calls or effects. Unknown model usage retains its reserved budget.

## Evaluation and persistence

SQLite uses WAL, a busy timeout, explicit operational tables and FTS5. Schema 1 migrates transactionally to schema 2; unknown newer schemas are rejected without rewriting their version. Indexes are derived from scoped records; retiring/deleting/expiring memory invalidates affected retrieval entries.

Experiments persist their frozen cases, starting memory and policy before invoking a host-owned evaluator. Every experiment budget field must fit the run's grant, and execution requires sandbox or apply mode. Agent-authored `model_version` must match the configured run model. Interrupted experiments retain partial evidence and require owner investigation before protected reevaluation. Arm/repetition/case working directories are separate. `EvaluationTask.memory_start` contains snapshots of the declared, accessible Memory records; source event IDs are not starting-memory IDs. An evaluator initializes each new `memory_namespace` from those snapshots. The namespace is an opaque ID, not a path; the evaluator receives the workspace separately, as its current directory for command evaluators. Memory may persist between cases only when `retain_learning_memory` is enabled; it never crosses arms or repetitions. Directories are discarded after the study by default. The initial snapshot is limited to 256 KiB.

A host can register a development-only policy with `allow_generated_development_cases: true` and an empty `case_ids` list. The experimenter then supplies `Experiment.development_cases`, with unique IDs, inputs, source mechanism, synthetic development provenance and proposed `candidate_checks`. Its `case_ids` must match those cases in order. Rust resolves all source references within scope and rejects collisions with registered cases. The host's evaluator, required checks, repetitions and evaluation limit remain fixed. Candidate checks remain proposals. These studies cannot create admission; a separate host-owned acceptance study is required. All templates use the same evaluator path.

No model-provider credentials are passed to registered tools or evaluators. A separately authorized semantic evaluator can implement the `Evaluator` trait or the JSON command interface with its own host-supplied isolation. The supplied reference evaluator executes installed measurement procedures and checks their outputs; it does not interpret arbitrary generated code.

The CLI allows only the selected provider's environment into the worker. Azure uses Pi's `azure-openai-responses` transport with its API key, base URL/resource name, API version and deployment-name map. `request.model` remains the underlying catalogue model ID. Azure credentials and endpoint settings are not included in agent prompts or Rust checkpoints. Missing required Azure configuration fails before model permits are reserved.

The Pi full stream API requires a tool call for each turn, including `finish` for terminal results. Reasoning-capable OpenAI/Azure models receive medium reasoning effort within the existing 4,096 output-token reservation. Non-reasoning models omit reasoning options. Anthropic uses its native `any` tool choice without enabling extended thinking. Provider reasoning stays outside visible evidence.

See [JSON-RPC 2.0](https://www.jsonrpc.org/specification) for the envelope specification. Framing, checkpoints, grant binding, cancellation and recovery are Ribosome conventions, not guarantees supplied by JSON-RPC.

## External harness protocol

`ribosome host` serves the separate `ribosome-host/1` handshake and canonical `x-host-methods` allowlist. It binds each attachment to the host's immutable grant and one source execution. `attachment.events` acknowledges only committed batches; subscription routing commits its work item and cursor together. A second SQLite connection permits ingestion/status during slow serialized effects. Write transactions reserve the writer before reading state to update, avoiding WAL read-to-write upgrade races.

`attachment.feedback` delivers bounded, version-checked finding/proposal references; `attachment.ack` records delivery outcomes. `attachment.steer` records an unknown control intent before the consuming host calls its agent. Acceptance is distinct from later observed effects. Coordinated application additionally requires a held, unexpired generation bound to that repair run; `attachment.release` reconciles outcomes before releasing it. These administrative methods are unavailable to maintenance workers.

See the [integration guide](attachments.md) for wire examples and [operations](operations.md) for startup, detach, restart, schema migration, backup and index rebuild.
