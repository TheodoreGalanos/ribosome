# Ribosome v0.1
## Standalone specification and implementation plan

**Status:** Draft for implementation planning\
**Version:** 0.1-draft.2\
**Date:** 12 September 2026\
**Stack decision:** TypeScript + Pi agent-core for agentic capabilities; Rust for the infrastructure substrate.\
**Scope:** A standalone, agent-driven library for maintaining and improving agent behavior. This document specifies a proposed product; it does not describe an already implemented library.

### Revision summary

This revision replaces the Python-first implementation proposal with a two-language architecture: Pi-based TypeScript agents and a Rust infrastructure library. It specifies ownership, the local process boundary, shared contracts, recovery, and cross-language validation. The complete behavioral scope and all six implementation milestones remain in v0.1. This is a specification update, not an implementation or a report of tested integration.

---

## 1. Product definition

Ribosome gives agent systems a small set of tool-using capabilities for understanding execution, maintaining valid work, recovering from errors, reusing useful behavior, and testing improvements before they become inherited behavior.

It normally works **alongside existing agents**. It can also expose capabilities to a worker or planner directly, and process completed executions without a live host.

Its central cycle is:

```text
execution evidence
    -> agentic interpretation
    -> bounded investigation and action
    -> fresh evidence
    -> reusable behavioral material
    -> controlled experiments
    -> context-specific admission for future use
```

Ribosome is not a new general-purpose agent orchestrator, a benchmark platform, an operating-system sandbox, or a complete enterprise memory service. It supplies maintenance agents, their tools and contracts, and enough infrastructure to make their work useful and inspectable.

### Independence

Ribosome must install, run, and pass its reference demonstrations without aec-bench. Its core must not import aec-bench, expose benchmark-specific types, or require its task, harness, evaluation, or storage model.

AEC-Bench may donate ideas and small, independently useful code fragments. Before copying code, check licensing and attribution, identify transitive dependencies, adapt its vocabulary, and bring its relevant tests. Do not copy a large subsystem merely to reuse a small behavior. An integration with aec-bench is an optional adapter, not the foundation of Ribosome.

### First-version scope

The first specification includes the entire behavioral cycle: motif recognition and discovery, extraction, proofreading, repair, recombination, artifact care, regulation, scoped memory, controlled learning and evolution, quality-diversity inventory, and regeneration against desired properties.

Small footprint means a limited deployment surface and shared mechanisms—not replacing these capabilities with a handful of heuristics or postponing their agentic substance.

## 2. The governing boundary: agency and authority

**Agents decide what the evidence means and what to do. Infrastructure establishes what they can do and records what actually happened.**

Agents own semantic interpretation, motif discovery, diagnosis, investigation, planning, adaptation, repair, synthesis, memory consolidation, experiment design, and recommendations about reusable improvements.

The Rust infrastructure owns communication, event ingestion, work queues, scope filtering, persistence, retrieval execution, tool dispatch, state-version checks, resource accounting, and mechanically enforceable constraints. The TypeScript agents, using Pi agent-core, own the agentic responsibilities above. Deterministic tests remain useful measurements; an agent interprets their significance and chooses its next action. Language choice does not transfer diagnosis, adaptation, or semantic decisions into infrastructure.

The default caretaker is not a rule engine whose only intelligence is labeling a preselected fault. It must be able to inspect an unexpected situation, seek additional evidence, distinguish competing explanations, act through granted tools, inspect the result, and revise or abandon its approach.

Conversely, an agent cannot grant itself broader access, fabricate tool observations, silently alter acceptance criteria, or declare a candidate admitted merely because it considers the change promising.

### Autonomous action is a first-class path

An agent may directly create a repair branch, edit allowed material, execute a retrieved procedure, run checks, and continue a bounded repair loop when its host grant permits those actions. Requiring human approval for every local operation is not the default.

An intervention record is the structured representation of a planned change, not necessarily an approval form. The same record can be applied immediately inside an existing grant or submitted to the owner when additional authority is required.

### Initial autonomy modes

- **Observe:** Read granted evidence and write annotations, findings, and recommendations.
- **Sandbox:** Additionally investigate and repair in an isolated branch or disposable workspace, and run granted checks.
- **Apply:** Additionally apply qualifying results through a host-controlled boundary to designated live artifacts or continuations.

A host chooses the mode and limits. An agent can request a different grant but cannot change its own mode. Publication, external writes, and other consequential effects remain explicit capabilities.

## 3. Architecture and deployment

### 3.1 Two components, one product

Ribosome has a Rust infrastructure library and a TypeScript agent package. A thin Rust executable composes the local reference deployment; it is not a mandatory remote service. The TypeScript package uses Pi agent-core directly rather than implementing another general-purpose agent loop.

```text
Existing agents, workflows, artifacts, and external host tools
                            |
                Rust HostAdapter / evidence import
                            |
            Rust Ribosome infrastructure library
    events | communication | work | grants | persistence
    search | artifacts | effect receipts | experiment execution
                            |
           versioned local bidirectional bridge
                            |
              TypeScript Ribosome agent package
                Pi agent-core + Pi AI transport
        Caretaker         Curator          Experimenter
        operators | context construction | adaptation
                            |
                     model providers
```

Every reasoning-to-action cycle can cross this boundary repeatedly. Rust does not wait for a whole investigation to finish before making a useful tool available. An agent requests evidence, investigates, calls an authorized action, inspects its actual receipt, and chooses the next step.

### 3.2 Responsibility and state ownership

| Concern | TypeScript / Pi responsibility | Rust responsibility |
| --- | --- | --- |
| Agent execution | Pi tool-using loop, profiles, instructions, context selection, semantic control | Work dispatch, process supervision, grants, deadlines, cancellation, durable status |
| Communication | Interpret messages; decide whom to ask and what to request | Delivery, routing, inboxes, ordering metadata, cursors, acknowledgements and bounded retries |
| Evidence | Interpret events and artifacts; request more context | Ingest, validate, scope, version, retrieve and persist evidence |
| Search and retrieval | Formulate queries; compare candidates; assess fit; adapt or abstain | Execute queries, enforce scope and explicit filters, index, paginate and return evidence |
| Motifs and memory | Recognize, discover, extract, consolidate, generalize and identify contradictions | Store records and versions; update indexes; enforce retention, deletion and admission restrictions |
| Repairs and other effects | Diagnose, plan, edit through tools, inspect outcomes and revise | Authorize, check state, dispatch effects through adapters, record receipts and reconcile uncertainty |
| Experiments | Generate development scenarios, design comparisons, investigate results and recommend admission | Isolate runs, enforce feedback visibility, execute evaluators, attribute measurements and apply authorized policies |
| Quality-diversity | Propose useful descriptions; interpret specialization and trade-offs | Maintain cells and apply the study's fixed insertion policy to accepted evaluation evidence |

Pi's live in-memory context is a working representation, not a second durable authority. Ribosome-owned persistent records, search indexes, session checkpoints, work status and receipts are written through Rust. External hosts remain authoritative for their own world and artifact state; Rust records references and invokes the owning adapter rather than inventing a second world state.

The TypeScript agent package has no independent SQLite store, durable task queue, search index, admission database or retry scheduler. Small in-memory caches and SDK client plumbing are allowed; they must not become hidden competing infrastructure.

### 3.3 Pi is the agent library

In this specification, `pi-core` means upstream **Pi agent-core**. The upstream source consulted for this revision names the packages `@earendil-works/pi-agent-core` and `@earendil-works/pi-ai`. The former provides the stateful tool-using agent and events; the latter supplies model interaction. These are explicit dependencies of the agent package, not a requirement for the host's existing agents to use Pi. [P1][P2]

Use the agent-core library, not the interactive coding-agent CLI as Ribosome's runtime. Do not inherit a coding CLI's workspace tools, session persistence, configuration discovery or broad permissions by default. The upstream project explicitly does not provide a built-in permission boundary for process, filesystem, network or credential access. [P3]

The Pi integration must supply versioned profiles/operator instructions, context transformation, custom tool wrappers, event adaptation, cooperative cancellation, supported steering/follow-up behavior, and a tested session restore path. It should use Pi's existing extension points rather than fork or recreate its loop. Exact APIs and compatible package/runtime versions must be pinned and exercised in M1; a source manifest is not proof of a published or tested release. [P1][P2]

Model-provider transport stays in TypeScript through Pi AI. This is part of using the chosen agent library, not a second general communications service. Rust owns call permits, aggregate accounting and deadlines; the trusted TypeScript adapter reports observed model usage and honors cancellation. There is no requirement to reimplement provider SDKs or introduce a Rust model gateway in v0.1.

### 3.4 Agent profiles

**Caretaker:** Works with active or interrupted tasks. It investigates findings, performs proofreading, proposes or executes repairs, adapts strategies, retrieves prepared implementations, checks artifacts, and restores missing task properties. It may decide that no intervention is useful.

**Curator:** Works from evidence windows, checkpoints, and completed executions. It annotates occurrences, discovers definitions, extracts candidate implementations, identifies dependencies and assumptions, preserves useful fragments from failed runs, and consolidates scoped memories.

**Experimenter:** Turns candidate improvements into experiments. It creates development scenarios, runs controlled comparisons through an evaluator, performs ablation and interaction studies, inspects failures, proposes revisions, and prepares admission recommendations. It does not control protected evaluation criteria or admit its own candidate without the configured decision boundary.

These are three default TypeScript profiles, not three compulsory services or a permanent hierarchy. One worker can use different profiles in separate runs. Existing workers or planners can invoke the same capabilities without duplicating them.

### 3.5 Execution modes and local lifecycle

**Offline** processing consumes exported evidence and artifacts through Rust without requiring a live task host. It supports motif and curation work, experiments where execution is available, and explicitly labeled synthetic reconstruction where it is not.

**Alongside execution** processing uses Rust event ingestion and work delivery. It observes without making every worker turn wait for a maintenance model call. The application explicitly starts and stops the runtime; importing either library does not create an invisible daemon.

**At selected boundaries** processing handles handoff, publication, tool-effect, or checkpoint events. Mechanical checks can be immediate. Agentic assessment can run ahead of the boundary or await completion when host policy requires it. A shadow observer cannot block effects without a host-supported pre-effect hook.

**Embedded capability use** lets a planner or worker request an operator through the same client and grant model. A TypeScript host can embed the Pi-backed capability package while connecting to Rust. Embedding does not transfer persistence or effect authority into TypeScript.

The reference deployment is one explicitly started Rust host process and a bounded number of supervised Node.js agent workers. Each worker handles one active AgentRun at a time initially. A worker may remain warm for compatible work in the same trust scope; context is reset between unrelated runs. Rust owns dispatch and concurrency. No broker, network listener or distributed scheduler is required. The number of processes is a deployment choice, not the number of biological roles.

## 4. Implementation footprint and the language boundary

### 4.1 Initial stack

**Rust:** A small library with a thin executable, asynchronous process/I/O supervision, validated request handling, SQLite, metadata/full-text retrieval, and ordinary artifact files. Tokio and Serde are the initial choices for asynchronous infrastructure and serialization. SQLite FTS5 supplies the first text-search implementation; startup/build checks must verify that FTS5 is enabled. [R1][R2][R3]

**TypeScript:** A Node.js package using Pi agent-core and Pi AI, with the caretaker/curator/experimenter profiles, operator instructions, the Pi adapter, model-context handling, and a small typed client for Rust tools. The package supplies a working integration with at least one model provider. Scripted agent doubles do not satisfy this requirement.

**Not required:** Python, a replacement agent loop, a message broker, vector database, graph database, web service, native Node addon, foreign-function interface, or container platform for the trusted local demonstration. Host task material can be in another language, but that does not create a Ribosome runtime dependency. Stronger sandboxing remains a host responsibility where untrusted code is executed.

The Rust package remains usable as a library. The executable owns local composition, supervision and a CLI; it must not grow into a required standalone platform. Exact Rust, Node, Pi and dependency versions are recorded in build manifests and lockfiles before implementation claims are made.

### 4.2 A small local protocol

Use **JSON-RPC 2.0 over newline-delimited UTF-8 messages on child stdin/stdout** for the reference bridge. JSON-RPC provides request/response and error envelopes; newline framing, version negotiation, cancellation, event cursors and recovery semantics are Ribosome's own documented conventions. JSON-RPC itself does not provide durable delivery, sandboxing or exactly-once effects. [R4]

Both sides may issue requests while other requests are outstanding. A waiting `agent.run` request must not block the reader that handles the agent's tool calls. Stdout is protocol-only; diagnostics go to stderr. The bridge enforces message-size bounds, bounded queues, timeouts and explicit shutdown behavior. Large artifacts are referenced and read in bounded chunks rather than repeatedly sent through the pipe.

A startup handshake binds the protocol version, worker build, Pi adapter/version, capabilities and session. Unsupported combinations fail explicitly. Rust binds each worker/run to its grant; the model cannot expand scope by editing a `scope` or `grant` argument. Cancellation and deadlines are explicit bridge operations with recorded outcomes, not assumed features of the transport.

Illustrative method families are `agent.run`, `agent.cancel`, `evidence.read`, `search.query`, `artifact.read`, `action.execute`, `work.request`, `record.submit` and `session.checkpoint`. These are method groups to implement and validate in M1, not a commitment to a broad public RPC standard. No generic database, arbitrary Rust-method, or unrestricted filesystem endpoint is exposed.

Fire-and-forget notifications are for dispensable progress only. Checkpoints, completed evidence batches and requested state changes require acknowledgement. Stable effect-operation IDs are separate from per-request RPC IDs. Rust persists an operation's intent before dispatch and records its outcome; a missing response triggers lookup/reconciliation rather than blind execution again.

### 4.3 One contract definition, two typed implementations

Keep canonical wire schemas and examples in `contracts/`. Generate Rust and TypeScript transport types from one reviewed source, with runtime validation at both boundaries and cross-language conformance fixtures. This applies to exchanged records, not every private internal type.

Specify absent versus null values, enum behavior, timestamps, artifact versions, and error shapes. UUIDs and large counters that may exceed JavaScript's exact-integer range are strings on the wire. Missing measurements remain unknown. Protocol version and record schema version are separate from motif/implementation version.

Generated types do not establish authorization or semantic correctness. Rust validates grants, referenced state and mechanically checkable constraints at application time. The agent evaluates meaning and recommends or requests actions within those constraints.

Readable names plus explicit versions identify definitions and implementations. Rust allocates operational UUIDv7 identities with short display labels. Content hashes may support integrity or deduplication; they are not the default human-facing identity system.

### 4.4 Tool and receipt boundary

Pi tools are typed TypeScript adapters to Rust operations. A tool call supplies bounded arguments; Rust checks the grant and relevant state, invokes the host capability, persists the receipt, and returns a reference plus useful result content. The agent interprets that result and can immediately continue its investigation. No extra human approval is needed when the grant already permits the action.

Semantic tool preparation and result presentation may occur in TypeScript. Durable persistence, search execution, cross-agent delivery, workspace mutation, process launching and other infrastructure effects go through Rust. Do not import unrestricted coding-agent tools as a shortcut around this boundary.

Pi lifecycle events describe agent activity. An agent's tool-completion event is not independently authoritative evidence that a host-side effect occurred. Rust action receipts bind actual adapter results; TypeScript converts them into Pi tool results. Model-authored findings and explanations remain separately attributed records. A receipt proves the observed execution outcome, not that the artifact is semantically correct.

### 4.5 Sessions, restart and budgets

The Pi adapter holds active context in memory and serializes a versioned continuation payload. Rust persists that payload together with the evidence frontier, profile/operator/model versions, usage, completed receipt references and unresolved effects. The payload format is pinned to the tested Pi adapter; it is not an unversioned dump of library internals. Rust need not reinterpret provider-specific message details to persist them.

Checkpoint only at coherent boundaries with an explicitly representable continuation. On restart, Rust reconciles dispatched effects, then the TypeScript adapter restores context using the supported Pi API. Restoring messages alone does not restore the host workspace, guarantee identical model output, or justify rerunning completed tools. Unsafe or unsupported continuation returns a recoverable explicit status instead of inventing observations.

Use Pi's event hooks to adapt complete messages and turn summaries into bounded evidence batches. Streaming text is optional transient telemetry, not one synchronous database transaction per token. Backpressure must not silently discard required receipts or acknowledged checkpoint state. [P1]

Before each model call and privileged action, the worker obtains the applicable Rust budget permit. The Pi stream adapter reports actual usage, including provider failures where available. Rust prevents further dispatch when bounds or unknown usage require it and sends cancellation on deadline. Already in-flight provider work may still incur cost; a hard monetary ceiling requires an enforceable provider-side cap or a conservative reservation supported by the configured model. Do not claim that killing a worker reverses remote effects or guarantees zero additional spend.

The reference worker runs reviewed, pinned code with only the credentials it needs. It does not evaluate arbitrary agent-generated TypeScript inside its own process. Untrusted generated code runs through an appropriately isolated host executor. A language or process boundary alone is not an operating-system security boundary. [P3]

### 4.6 Suggested repository boundaries

```text
ribosome/
    Cargo.toml
    package.json
    contracts/                      # Canonical wire schemas and fixtures
    crates/
        ribosome-core/              # Rust modules, not separate services
            src/
                events/             # Ingestion and evidence views
                work/               # Delivery, leases and supervision
                store/              # SQLite, artifacts and checkpoints
                search/             # Queries and derived indexes
                effects/            # Grants, dispatch and receipts
                experiments/        # Isolation and evaluator execution
                inventory/          # Admission and diversity bookkeeping
                protocol/           # Validated local bridge
        ribosome-cli/               # Thin local composition and commands
    packages/
        agents/
            src/
                pi/                 # Pi integration, context and events
                profiles/           # Caretaker, curator, experimenter
                operators/          # Agentic capability instructions
                tools/              # Pi tool wrappers over Rust requests
                client/             # Typed bridge client, not another store
    tests/
        contracts/                  # Rust/TS conformance
        integration/                # Bridge, effects and crash recovery
        evaluations/                # Live model-backed operator cases
    examples/
        local-project/              # Independent multi-worker demonstration
```

Start with one substantive Rust crate and one TypeScript agent package. Split them further only for demonstrated ownership or dependency reasons. The CLI is a composition shell, not a second implementation.

`HostAdapter`, storage/search execution and evaluator dispatch live in Rust. The Pi adapter and capability API live in TypeScript. A semantic evaluator may be a separately authorized Pi agent; Rust still controls its evidence access and records its result. A host adapter may call existing external services or executables in other languages without moving Ribosome-owned infrastructure into TypeScript.

## 5. Evidence and communication

### Event model

An event carries an ID, scope, run and producer identity, local sequence, event kind, timestamp, relevant parent/correlation references, payload or payload reference, and referenced artifact versions.

Useful initial event kinds include task start, action completion, tool result, artifact change, check completion, handoff, task completion, cancellation, and intervention completion. This is an extensible vocabulary, not a fixed semantic ontology.

Agent hypotheses are distinguishable from executor-observed events. An agent can claim that a repair probably succeeded; only an execution/check receipt can establish what actually ran and what it returned.

Ordering is local to the producer unless the host provides stronger information. Timestamps do not establish dependency. Cross-agent dependency links record whether they were supplied by the host, observed from artifact reads/writes, or inferred by an agent.

### Evidence views

An evidence view is a bounded projection of events, artifacts, dependencies, and relevant memories. It records its evidence frontier and the versions it read. Agents can ask for more evidence through tools rather than receiving every project transcript by default.

Access control applies before a view, search result, or artifact enters model context. Brief evidence-linked explanations are useful; the product must not depend on access to hidden model reasoning.

### Work and communication

Work items have a subject, owner, scope, reason, evidence references, budget, and status. Findings concerning the same subject can be attached to an existing work item instead of causing competing agents to start duplicate repairs.

Event subscriptions, size/time batching, and topic routing are mechanical wake-up mechanisms. They should not decide semantic diagnoses. Agents decide whether more investigation or action is warranted.

Agents can create follow-up work and request another profile within host-granted budgets. Each request retains causal origin and root budget references. Bounded depth, repeated-subject suppression, and explicit terminal states prevent unbounded agent-on-agent review loops.

### Delivery and recovery

The local store supports at-least-once work delivery with stable IDs and idempotent state transitions. It does not promise exactly-once external effects.

Before retrying an action whose completion is unknown, reconcile with its host adapter. Record started, succeeded, failed, or unknown effect status and any adapter-provided idempotency key. Do not blindly repeat external writes after a crash.

Rust persists operational state in explicit tables and owns acknowledgement, checkpoint and recovery transitions. The TypeScript worker does not maintain a second durable session database. Events and receipts provide evidence; they are not the sole authority for all mutable project state or a requirement to rebuild the whole system by replaying a universal log.

## 6. Behavioral motifs

A motif is a meaningful behavioral pattern or function, not merely a sequence of labels. A definition, an occurrence, and a reusable implementation are different records.

### MotifDefinition

Contains a readable name, version, intended function or transition, applicability description, recognition instructions, expected observable obligations, positive examples, counterexamples, and references to optional machine checks.

Recognition instructions are agent-facing descriptions. A named obligation is not executable unless an appropriate checker is actually supplied.

Example:

```yaml
name: verify-current-artifact-before-handoff
version: 1
intent: Hand off an artifact with applicable validation of its current version.
recognition_instructions: |
  Identify the artifact actually handed off, the checks that ran, and the
  versions they covered. Distinguish an unfinished check from a failed one.
  Investigate relevant edits between validation and handoff.
obligations:
  - Required checks cover the handed-off artifact version.
  - Check results support the claimed validity.
  - Unresolved limitations are communicated rather than silently omitted.
counterexamples:
  - A check ran against an older artifact.
  - The worker claimed it tested the result but no execution evidence exists.
```

### MotifOccurrence

Contains the definition/version, execution and scope, selected event or subgraph references, subject artifacts and versions, an evidence frontier, recognition status, obligation results, supporting evidence, unresolved assumptions, and the identifier/version of the annotating operator.

Occurrences can cross agents, overlap, nest, and be noncontiguous. They should not require partitioning a transcript into exclusive segments.

Recognition status is tentative, supported, or rejected. Obligation state is independently open, satisfied, violated, or unknown. Self-reported confidence may be recorded as such, but must not be represented as a calibrated probability without measurement.

Host check evidence is separate from the authored state of an `Obligation`. A registered checker can bind a semantic property to an exact Obligation record version and target artifact. A passing check supports only its declared properties on the observed versions. Revising the property requires renewed host authorization and fresh evidence. Unknown effect outcomes require explicit host settlement of the stopped executor before continuation; settlement cannot establish execution or restore validity. The runtime exposes current assessments through `artifact.validity`; retained receipts and assessments remain historical observations. See [the protocol](protocol.md) for transfer and invalidation rules.

A motif can be present but unsuccessful. A globally failed execution can contain a locally successful occurrence. A correct-looking output does not by itself establish that the intended motif occurred.

### MotifImplementation

Contains a name/version, the motifs it aims to realize, executable material or agent instructions, parameter and interface contracts, required capabilities, state assumptions, possible effects, failure behavior, provenance, and evaluation references.

Implementations may be code, agent instructions, or workflow fragments. Reuse does not require reducing every behavior to deterministic code. Stable mechanical subprocedures may later become ordinary code; semantic adaptation remains agent-driven.

### Admission

An admission record identifies the implementation and version, allowed usage context, supporting evaluations, restrictions, decision authority, and supersession/retirement state. Admission is contextual, not a universal badge of correctness.

Definitions, occurrences, implementations, and admissions evolve separately. Revising a recognizer can produce a new annotation; it does not rewrite the original observations.

## 7. Operator contracts and tools

An operator is a versioned agentic capability: instructions, required input shape, available tools, allowed effect requirements, output shape, and completion/abstention conditions. It is not necessarily a separate agent or a single model call.

Every operator follows a bounded cycle:

```text
inspect -> form working explanation -> acquire evidence -> plan
        -> act within a grant -> observe -> revise, finish, or abstain
```

The implementation should support open-ended investigation inside explicit resource and authority boundaries rather than forcing every fault through a fixed diagnosis tree.

### Shared records

| Record | Required meaning |
| --- | --- |
| Finding | The observed or suspected problem, supporting evidence, affected subject, uncertainty, and scope |
| Intervention | A proposed transformation against explicit state versions, its effects, checks, limits, and fallback |
| ActionReceipt | What the host/tool actually executed, target versions, outputs, side effects, resource use, and terminal status |
| AgentRun | Profile/operator/backend versions, granted tools, evidence references, actions, resource use, and terminal disposition |

A finding may be descriptive without requiring intervention. An intervention may be rejected as unnecessary. Unknown attribution and no useful action are legitimate outcomes.

### Intervention shape

```yaml
kind: repair
operator: excision-repair@1
subject:
  execution: compare-options/run-42
  workspace_version: 12
  evidence_frontier: planner/e93
finding_ref: finding-27
entry_state: extracted-measurements@3
preserve:
  - original-inputs
  - independent-cost-analysis@2
replace:
  target: normalization-step
  implementation: normalize-measurements@2
bindings:
  measurements: extracted-measurements@3
invalidate:
  - aggregate-table@4
  - ranking@2
recompute:
  - aggregation
  - ranking
required_checks:
  - compatible-scopes
  - dimensional-consistency
  - aggregation-regression
unresolved_assumptions: []
requested_effects:
  writable_scope: comparison-workspace
  external_effects: none
budget_ref: repair-budget-8
fallback: rerun-owning-subtask
```

The host grant is separate from the intervention's requested effects. Application requires both authorization and current-state compatibility.

### Tools

Tools should cover evidence inspection, artifact access, dependency inspection, inventory and memory queries, work-item creation, branch creation, allowed edits, bounded command execution, check execution, implementation execution, annotations, experiment execution, and admission requests.

The Rust tool/host-adapter boundary enforces granted paths, network/effect permissions, and credentials; the TypeScript wrapper is not the final permission check. A textual allowlist in an agent prompt is not isolation. Hosts that expose arbitrary code execution must supply suitable sandboxing; Ribosome's local reference adapter is not a security boundary for hostile code.

Protected observations and receipts are written by the substrate/adapter, not by ordinary model-output tools. Agents write explanations and proposals into separate records.

## 8. Operator families

### Proofreading

The caretaker investigates whether a proposed or completed transition satisfies relevant obligations. It can inspect additional context, request a check, advise the owner, or act within its grant. Cheap argument/version/permission checks remain deterministic.

Pending work is not automatically failure. Online assessment must account for whether the relevant handoff or effect boundary has occurred.

### Excision repair

The caretaker locates a sufficient affected region, preserves independent work, replaces or replans the faulty part, invalidates dependent outputs, and reruns the necessary work.

Entry conditions and exit obligations matter more than the textual size of a patch. If dependency evidence is incomplete, the agent must state the uncertainty and choose a wider repair or abstain rather than promise unsupported minimality.

Repair can target a live continuation, a branch, an artifact, an agent instruction, or a workflow implementation. The host adapter determines which of these it actually supports.

### Recombination

The caretaker or planner retrieves already prepared implementations for a desired transition, checks fit, adapts bindings and instructions, resolves missing preconditions, and executes/tests the transplant.

The output identifies donor/version, recipient state, bindings, adaptations, incompatibilities, checks, and fallback. Similarity is a retrieval clue, not evidence of compatibility. A no-match result is valid.

### Chaperoning

The worker or caretaker helps an artifact become usable and verifies its integration into the task. This can include fixing formatting or interfaces, running consumer-specific checks, and reconciling inconsistencies across sub-results.

This capability should normally be available to a worker or planner. It does not require a dedicated reviewer process for every output. Independent evaluation may still be required at consequential acceptance boundaries.

### Regulation

An agent chooses among permitted strategies, tools, checking depth, or prepared implementations as conditions change. The record includes the observed conditions, selected behavior, affected scope, and expiry or reconsideration condition.

Local regulation cannot change shared permissions, mandatory checks, protected evaluation criteria, or project-wide budgets. Existing workers can exercise this capability directly.

### Immune memory

The curator packages recurring failure mechanisms with recognition guidance, evidence, benign counterexamples, responses, and regression cases. The caretaker uses these memories to recognize and investigate future encounters.

Recognition must allow uncertainty and false positives. A memory saying "this might recur" must not silently become a permanent prohibition or a project-wide instruction.

### Regeneration

The caretaker restores missing desired properties rather than replaying a particular historical path. A changed input can invalidate selected claims or artifacts; the agent chooses how to restore their supporting relationships while preserving valid work.

Regeneration uses the same intervention, dependency, and check machinery as repair. It does not introduce a separate scheduler or a second definition of task success. Explicitly unresolved output is preferable to silently weakening the desired properties.

## 9. Throughput, containment, and concurrent action

The goal is not to prevent every mistake. It is to keep recoverable mistakes contained and owned, and prevent unacceptable effects from escaping.

Workers may continue reversible private work while findings are investigated. Dependent work may proceed speculatively only when the host permits it and its outputs retain the unresolved dependency. Mandatory permission and safety boundaries do not become optional because throughput is desirable.

Track unresolved obligations with an owner, affected outputs, age, consequence boundary, and supporting evidence. An agent decides whether the pattern merits earlier intervention, reduced fanout, or a different strategy. The host enforces hard resource and effect limits.

Every intervention records what state it read. Before applying it, the host validates relevant versions atomically or through an equivalent adapter-supported mechanism. A stale proposal must be re-investigated, adapted, or rejected. Timestamps alone are insufficient.

Concurrent findings should converge on an owning work item. Different agents may contribute evidence or candidate repairs; only a qualifying application reaches the live subject. Host scheduling, leases, and effect reconciliation own this coordination—not negotiations hidden inside chat transcripts.

Missing telemetry remains unknown. It is neither a zero-cost execution nor proof that no harmful behavior occurred.

## 10. Curation, retrieval, and inventories

### Preparation path

```text
closed or bounded evidence window
    -> recognize/discover occurrences
    -> identify a useful fragment
    -> recover dependencies and assumptions
    -> parameterize and package
    -> evaluate local function
    -> retain as candidate
    -> evaluate applicability/transfer
    -> context-specific admission
```

The TypeScript/Pi curator performs the semantic work. Rust stores the result, runs requested searches, and maintains indexes. Search-result interpretation and donor adaptation remain agent-driven even though query execution is infrastructure.

Preparation can be triggered by handoffs, checkpoints, failures, successful recovery, completed tasks, and explicit requests. It is incremental and budgeted. It should not repeatedly reanalyze the whole project history.

### Evidence inventory and usable inventory

The evidence inventory includes failed runs, partial fragments, rejected variants, hypotheses, and useful observations. The usable inventory is a view over implementations admitted for the caller's context.

The runtime normally retrieves prepared implementations. Raw-trajectory mining is a separate work item or explicitly budgeted exploratory action, not a hidden prerequisite to every recovery.

### Retrieval contract

Queries describe a desired function or state transition, current inputs/state, available capabilities, allowed effects, scope, and resource limits.

Authorization and hard compatibility filtering occur before material reaches the model. The agent can search iteratively, compare candidates, inspect evidence, and choose or abstain. Semantic fit, likely utility, and adaptations are agent judgments, not solely embedding distance or fixed ranking scores.

Prepared implementations must not contain donor-specific successful tool responses presented as reusable evidence. Execution produces new observations in the receiving environment.

### Quality-diversity

The initial inventory supports a small, configurable cell-based quality-diversity view over admitted implementations. Preserve useful alternatives for different conditions rather than only a global winner.

The agent may propose useful descriptors; a study fixes how descriptors and quality are assessed. Archive insertion mechanically applies that policy to evaluation results. Agents must not receive admission simply by supplying their own favorable score or descriptor.

A simple Rust map-based archive, persisted through the Rust store, is sufficient initially. More elaborate CVT selection, bandits, distributed search, and embedding indexes are optional extensions, not required dependencies. Evidence retention remains separate from elite selection so discarded elites do not erase useful failed material.

## 11. Project and client memory

Ribosome supplies structured memory records and tools, not an all-purpose memory platform.

| Memory kind | Purpose |
| --- | --- |
| Working | Open obligations, current investigations, short-lived strategy choices |
| Episodic | What happened in one execution or intervention |
| Aggregated | Revisable claims supported by multiple episodes or other evidence |
| Procedural | Links to candidate or admitted motif implementations |
| Failure | Recognition guidance and response knowledge for recurring problems |

A memory record contains scope, kind, content or reference, source evidence, applicability, creation/update information, validity or expiry conditions, supersession state, and unresolved conflicts where relevant.

TypeScript/Pi agents decide what to consolidate, distinguish observations from hypotheses, identify contradictions, and propose generalizations. Rust applies access rules, versions changes, and maintains retrieval indexes. No TypeScript memory database is introduced for convenience.

Client/project scope is enforced before retrieval results and context assembly, not merely stated in a prompt. Cross-client reuse requires an explicit permission and a suitable abstraction or sanitization step. Shared facts do not arise automatically from one local episode.

Persistent memory should not become an unrestricted store of credentials or raw sensitive transcripts. Retention, redaction, and deletion must be supported by the store/adapter; deleting or retiring an entry must also remove or invalidate derived retrieval entries.

A learned temporary condition does not necessarily justify an implementation change. "This endpoint is unavailable during this run" and "replace the standard retrieval procedure" are different records and different decisions.

## 12. Learning, evolution, and the laboratory

Operational recovery changes this run. Memory changes what later runs know. Evolution changes reusable implementations or agent instructions. Evaluation must identify which object changed.

Anecdotal success creates a hypothesis and candidate, not automatic inheritance.

### Experiment specification

An experiment records the candidate and baseline versions, hypothesis, development and evaluation material, scenario-family lineage, feedback visibility, model/tool versions, memory starting states, budgets, repetitions, metrics, independent checks, and acceptance policy.

The TypeScript/Pi experimenter can generate development instances and propose tests. Rust schedules and isolates execution, stores results and controls evidence release. A protected evaluator owns held-out material and established acceptance checks; semantic evaluation may use a separately authorized Pi agent, while deterministic measurements and result attribution stay with Rust and the evaluator adapter. Newly generated checks remain candidate checks until the appropriate owner accepts them. Separation of permissions is mandatory; using a different model alone does not establish evaluator independence.

Matched comparisons use the same declared cases and comparable budgets, with repetitions where outcomes vary. Seed labels do not establish deterministic model behavior.

### Functional experiments

Support knockout/ablation, rescue/substitution, interaction, stress, and transfer as experiment templates. They share one evaluator interface and ordinary experiment records rather than separate research frameworks.

Synthetic scenarios must retain their source mechanism and lineage. Development scenarios derived from the same failure do not automatically constitute independent transfer evidence. Freeze selection before protected evaluation and avoid repeatedly selecting against the same holdout.

Memory-learning experiments isolate treatment and control stores and define exactly what persists between experiences. Evaluation-created memories are discarded by default unless the protocol explicitly studies their retention.

### Outcomes and admission

Measure verified task outcomes, invalid effects, latency, execution plus maintenance cost, unnecessary interventions, repeated failures, and preservation of valid work. Report curation/laboratory costs separately and state any amortization assumption.

Compare against normal execution, extra retry budget, and ordinary critique-and-revise where applicable. Acceptance claims require complete, correctly attributed evidence. Recovery without a usable score remains recovery—not a measured performance improvement.

The experimenter produces an evidence-backed recommendation. An admission policy or separately authorized owner decides whether an implementation can be used in a stated context. This can be automatic for qualifying evidence; manual approval is not inherently required.

Admission creates a new versioned option for future use. Active sessions keep their pinned operator and implementation versions until an explicit safe-point transition. Candidate experimentation cannot silently rewrite a running caretaker or its permission layer.

## 13. Training-data products

The curation pipeline supports three explicitly different products:

**Observed:** Recorded actions and observations from an actual execution.

**Re-executed correction:** A new branch or rerun from a suitable state, with new tool observations and evaluation evidence.

**Synthetic reconstruction:** An authored hypothetical example whose synthetic origin and validation limits remain explicit.

Do not replace an action in a historical transcript and retain incompatible downstream outputs as though the new trajectory occurred. Missing reconstructable state requires a broader rerun or an explicitly synthetic product.

Retain both clean-success and recovery examples when useful. Failed steps can teach recognition and recovery; globally failed runs can supply locally useful behavior.

Training exports preserve source and scenario lineage, scope, visibility restrictions, artifact relationships, and evaluation split. Protected holdout evidence must not leak into training material. Rust performs the actual export and split/scope checks; the TypeScript curator decides which supported material to propose for export. Weight training itself is outside v0.1.

## 14. Validation and testing

Use four test layers: Rust infrastructure tests, TypeScript/Pi operator evaluations, cross-language contract/recovery tests, and end-to-end host tests.

**Rust substrate tests** use deterministic fixtures and scripted agent doubles to test dispatch, persistence, search and index invalidation, authorization, version conflicts, retries, receipts, worker supervision, and lifecycle transitions.

**TypeScript/Pi operator evaluations** use the actual Pi adapter and a real model backend on varied cases to test semantic diagnosis, useful investigation, repair, abstention, extraction, and adaptation. Passing scripted infrastructure tests is not evidence that the maintenance agents work.

**Cross-language tests** check schema conformance, unknown versions, absent/null handling, oversized and malformed messages, safe numeric encoding, bidirectional calls without deadlock, backpressure, cancellation, interrupted pipes, checkpoint restore and duplicate effect reconciliation. Test worker death after an effect but before its response, and deny attempts to forge grants or request direct database writes through the tool API. Package-boundary checks reject a TypeScript database dependency; process-level filesystem isolation is tested only for adapters that actually provide it. Contract generation must be reproducible in CI.

**End-to-end host tests** use an independent reference project with workers, a planner, artifacts, and a host adapter. No aec-bench or Python runtime dependency is allowed. The reference demonstration uses Rust infrastructure and Pi-based TypeScript maintenance agents.

Required adversarial and fault cases include duplicate and out-of-order events, incomplete evidence, stale repairs, interrupted effects, competing caretakers, unavailable tools, false-positive failure memories, malicious instructions embedded in observed content, cross-scope retrieval, leaked holdout data, exhausted budgets, and repeated self-triggering.

The permission boundary must hold even when an agent makes a poor judgment. Behavioral evaluations must also penalize unnecessary interruption and overchecking, not merely reward finding more alleged faults.

## 15. First-release demonstrations

### Demonstration A: maintain a valid handoff

Two workers and a planner produce an artifact. A worker validates it, then makes a relevant edit. The caretaker notices the handoff concern from evidence, inspects the versions and checks, creates a scoped repair, reruns the needed validation, and returns fresh evidence. A benign unrelated edit provides a counterexample. A concurrent modification exercises stale-proposal handling.

This demonstrates agentic proofreading, diagnosis, chaperoning, containment, communication, and action—not only rule-based detection.

### Demonstration B: reuse good behavior from a failed run

A completed run failed globally for an unrelated reason but contains a useful measurement-normalization procedure. The curator identifies and packages it. The experimenter tests its function and applicability, and the admission boundary admits it for a stated context.

A later agent encounters mixed measurement units. It retrieves the prepared implementation, adapts its bindings, executes it, and verifies new outputs. It does not mine the original transcript on the critical path or copy its observations.

This demonstrates motifs, extraction, recombination, failed-run value, experiments, and the usable inventory.

### Demonstration C: preserve knowledge and regenerate validity

A workflow records a scoped failure memory and an evaluated recovery procedure. A later workflow in the same project can use that information; an unrelated client cannot retrieve it.

A source revision then invalidates part of a report. The caretaker identifies the lost obligations and restores their support while retaining unaffected work. It reports unresolved properties honestly when restoration is impossible within the granted budget.

This demonstrates scoped memory, local regulation, regeneration, and the distinction between remembered information and inherited implementation changes.

## 16. Implementation milestones

All milestones below belong to v0.1. They are ordered delivery slices, not separate products or promises about elapsed time.

### M1 — Standalone substrate and a working agent

Implement the canonical contracts, Rust local store/search baseline, scope/grant enforcement, queue, event ingestion, artifact references, receipts, local host adapter, supervisor and CLI. Build the TypeScript package using Pi agent-core/Pi AI with one tested model provider; do not build a substitute agent loop. Implement the bidirectional stdio bridge, generated transport types, grant binding, checkpoint payload and cancellation before adding multiple profiles. Pin the supported build/runtime versions.

The first vertical slice is Rust dispatch -> Pi agent -> Rust evidence/search tool -> Pi decision -> Rust authorized action/check -> durable receipt -> Pi continuation. Exercise a denied action and interrupted-effect recovery in the same slice.

**Exit:** An independently installed Rust + TypeScript Ribosome caretaker consumes evidence, requests additional information, uses a Rust-granted tool, and records a terminal result through the actual Pi loop. Duplicate work, protocol mismatch, cancellation, budget exhaustion, worker restart and an effect completed before pipe failure are covered. Pi context restores through the Rust checkpoint store without a second TypeScript database. It runs without aec-bench or Python installed.

### M2 — Motif interpretation and curation

Implement Rust storage/contracts for definitions and evidence-linked occurrences, and TypeScript/Pi discovery, candidate extraction and the curator profile. Preserve provenance and synthetic/observed labeling through the bridge.

**Exit:** The curator handles both known and novel motifs, recognizes useful fragments in failed runs, distinguishes incomplete from violated obligations, and preserves counterexamples. Model-backed evaluation checks semantic quality.

### M3 — Agentic care and bounded action

Implement TypeScript/Pi proofreading, repair, chaperone and regulation capabilities that create findings/interventions and reason about dependencies. Implement Rust branch/apply dispatch, version checks, invalidation bookkeeping, fresh-check execution and receipt handling. Keep diagnosis and repair selection in the agents.

**Exit:** Demonstration A passes end to end. The caretaker can complete a scoped repair autonomously, abstain when evidence is inadequate, and avoid duplicate or stale live effects.

### M4 — Prepared reuse and scoped continuity

Implement Rust packaging storage, eligibility-filtered search, transplant execution, memory persistence, expiry and derived-index invalidation. Implement TypeScript/Pi binding/adaptation, episodic/aggregated/procedural/failure-memory consolidation and conflict interpretation.

**Exit:** A later workflow can retrieve and adapt prepared material without rereading raw history. Scope separation and inappropriate-transfer rejection are tested. Demonstration B can run using an explicitly supplied admission decision, pending automated laboratory support in M5.

### M5 — Experiments and tested inheritance

Implement the TypeScript/Pi experimenter, controlled scenario-generation capabilities and ablation/rescue/interaction/stress/transfer instructions. Implement Rust evaluator dispatch, isolated learning arms, attributed measurement storage, admission records and the small quality-diversity inventory view. Separately authorized semantic evaluators may use Pi.

**Exit:** Demonstration B completes the entire preparation-to-evaluation-to-admission path. A candidate can be accepted, rejected, or retained as inconclusive without inventing evidence. Protected evaluation and memory isolation hold.

### M6 — Regeneration, exports, and release hardening

Implement TypeScript/Pi desired-property restoration through the shared intervention machinery, Rust training exports and cost/latency accounting, the complete third demonstration, cross-language recovery documentation and fault tests. Measure process startup, bridge overhead and database contention separately from model time before adding native bindings or extra services.

**Exit:** All three demonstrations run independently. Scripted tests and live operator evaluations have separate reports. Unsupported host capabilities and unknown effects are explicit. The package documents how to run, stop, resume, and inspect maintenance work.

## 17. Release boundary and deliberate exclusions

v0.1 includes one complete local Rust infrastructure library/executable and Pi-based TypeScript agent package, all operator families through shared agent profiles, a prepared inventory, scoped memory, a small experimental loop, contextual admission, and the independent demonstrations.

It does not include a distributed scheduler, production multi-tenant identity provider, mandatory vector infrastructure, full graphical interface, weight training, arbitrary cross-runtime checkpoint restoration, unrestricted self-modification of the substrate, a Rust reimplementation of Pi/provider SDKs, a second TypeScript persistence/search/queue stack, mandatory native bindings, a Python runtime, or every model provider.

These exclusions do not remove agentic reasoning or reduce the product to heuristics. They restrict infrastructure and deployment breadth while preserving the full behavioral cycle.

Before publication, confirm Rust/npm distribution names, dependency/licensing choices, supported Node/Rust versions, the exact Pi package versions and the initial provider integration. Pin and test the supported combination in CI. The architecture does not depend on the availability of a particular public package name.

## 18. Definition of success

Ribosome v0.1 is complete when a standalone host can run maintenance agents that understand behavior, investigate unfamiliar failures, carry out permitted repairs, extract and adapt useful implementations, retain appropriate scoped knowledge, and establish through experiments which improvements deserve reuse.

The library must make five distinctions visible in data and behavior:

```text
observation != interpretation
recognized motif != successful occurrence
successful occurrence != reusable implementation
successful local repair != inherited improvement
agent decision != unrestricted authority
```

The result should be useful beside ordinary agents before it becomes sophisticated: a small agentic maintenance system with trustworthy infrastructure, not a large platform waiting for its first practical repair.


## Appendix A. Sources and dependency verification

These are primary implementation references consulted on 12 September 2026. They support dependency names and available mechanisms, not a claim that the proposed integration is already implemented or benchmarked. Verify and pin a compatible published release during M1; repository default branches are not reproducible dependency pins.

- **[P1] Pi agent-core README:** `https://github.com/earendil-works/pi/blob/main/packages/agent/README.md`. Describes the Agent API, Pi AI integration, context hooks, tool execution, events, cancellation/continuation surfaces and separately packaged SQLite backends. Ribosome uses Pi for its agent loop, not the optional TypeScript SQLite backend.
- **[P2] Pi agent-core package manifest:** `https://github.com/earendil-works/pi/blob/main/packages/agent/package.json`. Confirms the upstream package name, module packaging, dependencies and runtime constraints at the inspected source revision. No claim about the latest published npm version is made here.
- **[P3] Pi repository README:** `https://github.com/earendil-works/pi`. Describes package boundaries and explicitly distinguishes Pi from an OS permission/sandbox boundary.
- **[R1] Tokio documentation:** `https://tokio.rs/`. Reference for Rust asynchronous runtime and I/O infrastructure.
- **[R2] Serde documentation:** `https://serde.rs/`. Reference for Rust serialization/deserialization.
- **[R3] SQLite FTS5 documentation:** `https://www.sqlite.org/fts5.html`. Reference for local full-text query/index functionality and its build requirements.
- **[R4] JSON-RPC 2.0 specification:** `https://www.jsonrpc.org/specification`. Reference for request, response, notification and error envelopes. The local pipe framing and durability rules above are Ribosome-specific.

## Whole-agent experimentation

The host can run learned instructions through ordinary Pi execution under the existing experiment and admission contracts. It isolates cases, meters stages against a shared budget, retains missing outcomes and compares complete workflows against retry and critique controls. Functional admission and system-benefit decisions use separate declared objectives. Protected-set exposure persists across grants; archive entries use repeated aggregate evidence. See [protocol](protocol.md#whole-agent-laboratory), [operations](operations.md#running-whole-agent-studies) and [validation](validation.md) for implemented boundaries and observed limitations.

The R7 handoff adds an installed learning example, combined attachment/finalization/validity/withdrawal verification, schema-20 upgrade coverage and a sanitized evidence exporter. The installed live discovery attempt did not produce an executable candidate; learned transfer remains unqualified. See [validation](validation.md#r7-integrated-qualification-and-handoff) for the separate mechanical, installed and model-backed results.
