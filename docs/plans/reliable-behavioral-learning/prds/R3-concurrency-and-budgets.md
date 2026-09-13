# R3 — Responsive execution and aggregate budgets

**Priority:** Infrastructure hardening before broader agent experiments.\
**Outcome:** Slow work does not block unrelated maintenance, and every participating execution consumes a correctly attributed share of one aggregate budget.\
**Dependencies:** R2's effect lifecycle; coordinate checkpoint/compaction metering with R1.

## 1. Baseline and constraints

The worker supervisor currently holds the runtime mutex around the complete tool call, including blocking host commands and laboratory execution. Attachment ingestion/status already uses a separate connection to the same SQLite file. Preserve that existing improvement; do not describe this work as fixing an ingestion path that is already separated. [S7]

The laboratory bounds the declared experiment budget by the grant, but passes a copy of the full budget to each evaluation task. This is not an aggregate reservation mechanism for model-backed evaluations. [S8]

## 2. Short state work, separately owned execution

Separate an operation into:

```text
validate request and reserve budget
    -> prepare immutable execution request
    -> acquire the required workspace/executor permit
    -> recheck current authority and versions
    -> execute without a broad runtime/database lock
    -> capture outcome
    -> finalize through R2
```

Do not hold a SQLite write transaction while waiting for a workspace permit, a provider response, a subprocess, a child agent, or another database transaction. State transitions remain short and serialized where needed. SQLite WAL allows readers and a writer to coexist, but does not make multiple long-lived writers or application locks harmless. [E1]

Use a bounded Rust store service or a small connection arrangement over the same store. The first implementation need not introduce a large pool or service topology. An executor owns the synchronous host adapter and serializes effects for the relevant workspace. Independent evidence reads, usage settlement, cancellation, and checkpoints must not wait for an unrelated long command. A read that requires consistency with an actively mutated artifact may use an explicit snapshot or wait for that artifact's owner; this is distinct from blocking unrelated evidence queries.

Keep a documented lock acquisition order. Never acquire a workspace lock while retaining the general store lock. If an operation holds a workspace permit, any state transaction it needs must be short and must not depend on another owner waiting for that permit.

### Cancellation and ownership

Track issued executions independently from an awaiting RPC future. Aborting a waiting task is not proof that a blocking operation stopped; Tokio explicitly documents that an already running `spawn_blocking` task cannot normally be aborted that way. [E2]

Cancellation stops new work, signals the executor, and settles or marks issued effects unresolved. Keep the workspace fence until the actual process/effect settles. A completion timeout must not release write ownership underneath a still-running process.

A supervisor/store connection created during normal service operation must not mark another active host's runs interrupted. Perform orphan recovery only after acquiring the appropriate host ownership, once per host lifecycle.

### Nested maintenance without deadlock

A parent agent must not occupy the last worker slot while synchronously waiting for a child invocation that needs the same slot. Return a work/invocation reference. Park the parent at a coherent checkpoint and release its execution slot when it waits, or use an explicitly reserved child capacity policy. Do not fix this with an unbounded pool.

The existing Rust work queue remains the durable scheduler. TypeScript does not gain a second child-run queue or retry service.

## 3. Hierarchical accounting

Define one accounting tree:

```text
owner/root grant
    -> curation or experiment allocation
        -> arm/repetition/case allocation
            -> source worker / caretaker / curator / semantic evaluator
                -> provider permits and metered tool executions
```

A child allocation is a ceiling within the parent, not fresh money. Store parent IDs and causal work IDs; derive rollups from the same charged leaves so the same cost is not added repeatedly at every level.

Before dispatch, reserve the configured resource dimensions atomically. After an observation, settle once using adapter-observed consumption. An unknown result retains a reservation or unknown liability according to policy; it is not charged as zero. Repeated settlement is idempotent and conflicting settlement is rejected.

Resource dimensions include provider calls, input/output token bounds, monetary accounting, actions, child work, depth, and deadlines. Declare whether denied action attempts consume the existing action counter and apply the rule consistently. Do not silently change current public accounting semantics as part of a refactor.

### Evaluators participate

An evaluator that runs an agent requests real permits from this same accounting owner. Each evaluation receives an allocation/reference to remaining capacity, not the entire experiment budget cloned into every arm. A process that cannot honor permits or report usage cannot support a strict aggregate-budget qualification claim.

This does not require a Rust provider SDK. Pi remains responsible for provider transport; Rust issues permits and records observations reported by the trusted adapter. Do not accept candidate-authored numeric claims as billing evidence.

Include semantic compaction, contrast/review agents, generated-development studies, retrieval-related model calls, and the external source agent when it is part of the measured treatment. Costs outside controlled instrumentation must be reported as unknown or separately excluded, with the comparison limitation explicit.

Catalogue accounting is not a guarantee about provider invoices. In-flight work may incur spend after cancellation. Strict monetary ceilings need enforceable reservations based on supported limits or provider-side controls; retain the existing honest boundary.

### Exhaustion semantics

If the root cannot fund a remaining case, stop new dispatch and mark the experiment incomplete or exhausted. Keep already obtained failures and results. Do not drop unfinished cases from the denominator and call the candidate accepted.

A cancelled permit that provably never dispatched may release its reservation. A lost response after dispatch may not. Recovery must find permits by stable call/operation ID rather than issue a new allowance.

## 4. Implementation work

Primary files: `crates/ribosome-core/src/{supervisor,rpc,effects,process,accounting,experiments,work,store}.rs`, `attachments/service.rs`; `packages/agents/src/pi/agent.ts`; canonical budget/evaluator contracts.

1. Add contention and metered-evaluator regressions.
2. Extract short prepare/finalize operations from the long-running executor path while preserving R2.
3. Introduce explicit execution handles and cancellation/settlement ownership.
4. Connect laboratory allocations to the existing permit ledger and add nested invocation scheduling.
5. Make inspection report queue delay, execution time, store contention, model time, settled usage, and outstanding reservations separately.

## 5. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R3-01 | Hold a registered command for five seconds. A second worker's evidence read, checkpoint, and model-permit request each complete within the existing one-second local responsiveness target. Test both workers, not only attachment status. |
| R3-02 | Repeat with a long evaluator. Attachment ingestion/status and unrelated worker operations remain responsive. No SQLite write transaction spans evaluator execution. |
| R3-03 | Two repairs target the same workspace. Effects serialize or one becomes stale; unrelated reads remain available. |
| R3-04 | Cancel a blocking command and interrupt its RPC waiter. Write ownership is not released until settlement/reconciliation. |
| R3-05 | Root allows five model calls. Multiple evaluator arms/cases together attempt six. The sixth dispatch is denied even if each case individually fits a five-call limit. |
| R3-06 | Concurrent requests compete for the last token/cost reservation. Exactly one allocation succeeds when only one fits. |
| R3-07 | Crash after provider dispatch; unknown usage remains reserved across restart. Repeated usage settlement does not double count. |
| R3-08 | Curation, compaction, source worker, caretaker, and semantic evaluator calls all appear under the declared parent allocations. |
| R3-09 | An experiment exhausts capacity halfway through. Result is incomplete/inconclusive; finished and unfinished cases remain visible. |
| R3-10 | Worker capacity is one. A motif invocation requests child work. The parent parks, the child runs, and continuation resumes without deadlock or unbounded extra workers. |
| R3-11 | Compare contention/startup/bridge metrics before and after. Report observed measurements, not a claimed Rust performance benefit without data. |

The one-second test is a fixed local regression target already used by the project, not a production latency guarantee. Diagnose reproducible contention rather than merely inflating timeouts.

**Exit:** Independent operations remain responsive, conflicting effects retain ownership, and all metered participants demonstrably share aggregate limits.
