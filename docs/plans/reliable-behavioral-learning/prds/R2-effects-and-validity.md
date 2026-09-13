# R2 — Crash-safe effect finalization and version-bound validity

**Priority:** First correctness track.\
**Outcome:** Normal completion and recovery establish the same durable bookkeeping without replaying uncertain external effects. A successful repair updates exactly the validity it has established.\
**Dependencies:** Shared contracts; coordinate source/version representation with R1.

## 1. Baseline and problem

The current effect path persists an intent, performs an effect, invalidates dependencies, and saves a receipt/evidence event. The recovery path can infer completion from current bytes without repeating dependency finalization. Successful branch application also needs an explicit validity transition for the repaired artifact. [S5]

The desired fix is not a transaction spanning arbitrary filesystem and SQLite operations. That guarantee is unavailable through the current interface. Instead, represent the uncertainty and make internal finalization idempotent.

## 2. One execution/finalization lifecycle

Extend the existing effect record with internal phase and evidence; do not replace all public outcomes with an unrelated state machine.

| Internal phase | Required evidence |
| --- | --- |
| Prepared | Pinned action, grant, target, expected versions, relevant checks/policy, and operation ID |
| Dispatching | Authority and version preflight succeeded; adapter dispatch was about to begin or began |
| Observed | Actual adapter outcome captured, including before/after versions, declared writes, and validation evidence |
| Finalized | Receipt, evidence publication, dependency updates, and validity transitions committed |

A denial before dispatch finalizes a denied outcome with no claim of execution. A failure after an effect may have happened is not an ordinary "nothing changed" failure.

Before dispatch, persist the intent and execution-stage information required to interpret a later crash. After an adapter outcome is available, use one common `finalize_effect` operation for normal completion and recovery. Finalization is keyed by operation identity and safe to retry.

Persist the captured adapter outcome before retryable bookkeeping, or atomically with that bookkeeping. If neither can be made durable, recovery keeps the occurrence uncertain rather than inferring it solely from matching bytes. The short finalization transaction commits the receipt/evidence event and applicable dependency/validity transitions together. If it fails, retain the pending operation and retry finalization; do not repeat the external action. Version checks prevent a delayed finalizer from overwriting knowledge about a later change.

## 3. Reconciliation evidence is not a single boolean

An adapter reconciliation response must distinguish:

- **Execution established:** an adapter receipt or equivalent durable operation evidence establishes what ran.
- **Current postcondition observed:** the intended state is present, but the original operation's occurrence or prerequisites are not established.
- **Outcome unresolved:** available evidence does not establish occurrence or a usable postcondition.

Matching bytes may support the second category. They do not prove that a prepared intent was dispatched, that mandatory checks ran, or that a particular operation caused the current state.

For current-state observation without established execution, record a new attributed reconciliation observation. Conservatively invalidate potentially affected consumers and request fresh checks where appropriate. Do not upgrade it to "verified checked application." The owner may subsequently accept the current state through fresh authorized validation; that is a new decision supported by new evidence.

For unresolved non-idempotent effects, preserve the ownership fence and require host reconciliation. An agent summary cannot resolve the uncertainty. Cancellation or worker termination does not reverse an already issued action.

If the original content may have changed and changed back, do not claim detection of that history from a content digest. The local host supports the stated current-content/cooperative-writer guarantee; stronger history requires an adapter-owned revision token or journal.

## 4. Version-bound validation evidence

Persist what each successful check actually validates separately from everything it reads.

A validation result binds the check/tool version, policy/required-check identity, observed input versions, target artifact versions, explicitly validated properties, outcome, receipt reference, and host/evaluator authority. The reference host's `reads` and `validates` distinction must survive into this record. A property not explicitly validated remains unproven. [S6]

Use the existing `Obligation` records for semantic properties. A validation result is mechanical evidence attached to those obligations; it does not replace their meaning with a Rust rule engine.

### Checked application

A checked application can transfer branch validation only when all of the following hold:

1. The applied bytes are the bytes validated in the branch.
2. Relevant checker inputs still match both the checked state and the live recipient.
3. Required host checks and their versions/policy have not changed.
4. The active writer handoff and grant still authorize application.
5. The finalization is not clearing an invalidation caused by a newer source version/generation.

On application, invalidate dependent outputs that were not themselves restored. Clear the applied artifact's invalidation only for supported properties on the applied version. "A checker read this artifact" is not enough to clear its validity state.

If validation cannot be transferred safely, the artifact is applied-but-unverified or application is withheld according to host policy. Require an explicit live check rather than reporting restoration prematurely.

Replace an undifferentiated stale-path flag where necessary with enough version/generation information to distinguish invalidation causes. Do not introduce a complete logical theorem prover. Unknown coverage means conservative invalidation and agent investigation.

## 5. Concurrency and recovery interaction

Workspace effect ownership remains held while an issued action is settling. A lease timeout does not authorize a new writer to race a still-running old process. Where an adapter supports fencing tokens, pass and validate them. The cooperative local adapter must not claim fencing against arbitrary external writers.

R3 may move execution outside the broad runtime lock, but it must retain these ownership and finalization guarantees. Preflight checks are repeated after waiting for a workspace permit; checks made before a long wait are not necessarily current.

Source tombstones from R1 cannot be reversed by a late effect finalizer. If an effect took place just before revocation, preserve a restricted record of the actual outcome and refuse new unauthorized work; do not falsify history to make the states look simpler.

## 6. Implementation work

Primary files: `crates/ribosome-core/src/{effects,host,evidence,runs,store}.rs`, `attachments/repair.rs`, canonical action/receipt/check schemas, and existing recovery tests.

1. Add failure injection at each boundary between preflight, external mutation, adapter observation, finalization, and response.
2. Introduce explicit reconciliation evidence and the shared idempotent finalizer.
3. Persist check coverage/versions and version-aware invalidation causes.
4. Update checked application and handoff release to use finalization state, not an agent's terminal disposition.
5. Migrate existing effects conservatively: missing certificates stay missing, and ambiguous old intents require reconciliation.
6. Keep the existing registered-tool execution path working; new semantics improve its evidence rather than remove it.

## 7. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R2-01 | Crash after file replacement but before dependency updates. Restart performs no second write and downstream outputs become invalid. |
| R2-02 | Fail receipt/evidence persistence. Retry completes the same operation with one terminal effect identity and consistent bookkeeping. |
| R2-03 | Desired bytes existed before an undispatched intent. Reconciliation reports current-state observation, not proven checked execution. |
| R2-04 | Repair an invalidated report in a branch and apply it. Its validated current properties are restored; unvalidated downstream consumers remain stale. |
| R2-05 | A check reads two artifacts but validates one. Application cannot clear the other's invalidation. |
| R2-06 | A source changes after checks, including an input omitted from the agent proposal. Application is stale and does not falsely certify the recipient. |
| R2-07 | A later source change occurs before a delayed finalizer commits. The old finalizer cannot clear the new invalidation. |
| R2-08 | Worker dies after effect completion but before response. Restore references the same finalized receipt; no duplicate execution occurs. |
| R2-09 | Cancellation/expiry during an issued command retains ownership until settlement or explicit unresolved reconciliation. |
| R2-10 | Repeated reconciliation, lookup, and finalization are idempotent; a changed action under the same operation ID is rejected. |
| R2-11 | Migration encounters an old success with no transferable validation evidence. Preserve the observation without inventing certificates. |

**Exit:** The repaired artifact, its supporting validation, dependent validity state, effect receipt, and source evidence agree after both ordinary execution and every injected interruption.
