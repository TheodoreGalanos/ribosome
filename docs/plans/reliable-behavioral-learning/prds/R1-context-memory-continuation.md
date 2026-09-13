# R1 — Context, memory, and continuation correctness

**Priority:** First correctness track.\
**Outcome:** No resumed or continuing agent receives withdrawn information through a stale context path; long executions use bounded continuation state without losing effect identity.\
**Dependencies:** Shared contract/migration decisions. Can proceed alongside R2.

## 1. Baseline and problem

The current Pi adapter restores completed messages directly from a Rust checkpoint. The store separately retires records and invalidates derived records. Model context is trimmed for an upcoming request, but checkpoints retain the full message array. These mechanisms need a shared source-validity boundary. [S3][S4]

Two problems must be handled together: previously retrieved material can remain in a checkpoint after it becomes unavailable, and full checkpoint transport grows with execution length even when model context is trimmed.

Do not solve this with a prompt telling the agent to ignore stale memory. The material must not enter a new provider request when it is unavailable under the current policy.

## 2. Required semantics

### Access and freshness are different

A source can be:

- **Unavailable:** deleted, expired, retired from retrieval, redacted, or no longer authorized. Do not place its content or dependent summaries in subsequent model requests.
- **Available but revised:** the old version may remain legitimate historical evidence. Identify it as historical; it cannot satisfy a requirement for current state without reinspection.
- **Available and current:** usable within the active grant and declared purpose.

An old failed run can still be evidence for discovery. A deleted client memory is not permissible merely because it is historical. Encode the distinction instead of invalidating all historical evidence whenever any artifact changes.

### Source-aware context items

Represent the context delivered to Pi as items with:

| Field | Meaning |
| --- | --- |
| Item identity and content reference | Bounded reference to persisted content; not a second transcript database |
| Kind | Owner instruction, observed evidence, agent-authored interpretation/summary, or protocol continuation tail |
| Source references | Exact records/events/artifact snapshots and applicable versions |
| Required freshness | Historical evidence allowed, or current state required |
| Derivation references | Context items/evidence used to produce this item |
| Attribution | Host observation, source-agent message, or maintenance-agent interpretation |
| Coverage | Evidence frontier and relevant source-policy generation |

The trusted TypeScript adapter reports which evidence was supplied to the model. Rust persists the lineage. The model cannot make a summary appear independent by omitting source IDs. By default, derived content conservatively inherits the union of evidence visible to the call that produced it. Narrower attribution requires a separate checked transformation; it is not a claim accepted solely from the authoring agent.

Use structured reference edges in SQLite for availability checks and reverse lookup. Do not rely solely on scanning arbitrarily named JSON fields, and do not require a graph database.

### Authorization before continuation and provider dispatch

On resume, reconcile effects first. Resolve continuation references through current scope/access policy. Revalidate again before subsequent provider calls, so retirement in another workflow also affects a still-running caretaker.

Rust can use a source-policy generation and cached validated reference sets to avoid rewalking all ancestry on every call. Cache invalidation must happen after retirement/expiry/access changes. The safe fallback for missing lineage is to discard and rebuild the affected continuation, not to reuse it unverified.

A retirement acknowledged before a new context authorization must be respected. An already dispatched provider request cannot be retroactively withdrawn. Document the authorization/dispatch ordering and in-flight exception; do not claim control over content already sent to a provider.

### Rebuild without laundering withdrawn content

If an old tool result is unavailable, remove its dependent agent interpretations and summaries as well. Never ask a model to clean an old summary by sending the withdrawn content back to it.

If removal would break an assistant/tool exchange, use a supported clean continuation segment: owner task, current grants, fresh authorized evidence, outstanding obligations, and reconciled operation references. A transcript segment discontinuity is explicit and linked to its predecessor. It is not a forged continuation of an exchange whose observations no longer exist.

Do not synthesize a successful tool response. A pending action requires a real receipt, a safely reconciled state, or an explicit unresolved outcome. Unknown effects block further conflicting action under R2.

## 3. Retirement that survives interruption

Source retirement/deletion writes a tombstone and marks source-policy change in one short transaction. After that transaction, every retrieval surface must honor the tombstone, including record reads, search snippets, event projections, summaries, checkpoints, exports, and admissible inventory views.

For small dependency sets, retire derivatives in the same transaction. For large sets, enqueue an idempotent cleanup job in that transaction. Queries must deny affected derivatives through ancestry/availability checks while cleanup is incomplete. "The cleanup job has not reached this row yet" is never authorization to return its content.

Physical removal/redaction is a separate, observable operation with completion and failure status. Clean FTS entries, copied context payloads, cached projections, and other Ribosome-owned stores. Document backup retention separately. Do not equate a logical tombstone with secure erasure from all backups or provider systems.

Retirement need not erase protected evaluation evidence automatically; apply its configured retention and access policy. If retained, that evidence must remain inaccessible to ordinary context and training exports. Authorized aggregate evaluation decisions do not become raw holdout access through lineage traversal.

## 4. Bounded continuation format

Introduce a versioned continuation format that references retained messages instead of embedding the entire transcript. It contains:

- Exact run, profile/operator, selected implementation, model, and Pi-adapter versions.
- A bounded continuation summary reference plus its source closure.
- The recent complete protocol exchanges needed for supported Pi continuation, fetched in bounded chunks.
- Pending operation IDs, completed receipt references, evidence cursor/frontier, and source-policy generation.
- Open work/obligation references, without copying the whole project history.

Use the current bridge frame limit unchanged. Initial proposed engineering limit: a continuation descriptor of at most 256 KiB, measured after UTF-8 serialization; content exceeding that is stored and referenced. This is a local transport limit, not a claim about an appropriate model context size. An oversized required single tool result uses bounded excerpts and a readable artifact reference.

Compaction is an agentic summary task when semantic synthesis is required. It uses the same root budget, source authorization, and Pi integration. Mechanical trimming may preserve recent exchanges, but it cannot claim a semantic summary. The durable execution record remains available under retention policy; reducing continuation size does not delete execution history.

Integrate around the pinned Pi adapter's supported context/stream hooks. Do not assume that upstream `main` behavior matches the pinned version. Provider dispatch must fail safely if context cannot be authorized; hook errors must not silently revert to unsafe old messages. [E3]

## 5. Implementation work

Primary files: `packages/agents/src/pi/{agent,context,activity}.ts`; `crates/ribosome-core/src/{runs,records,evidence,rpc,store}.rs`; canonical schema and generator fixtures.

1. Add the real cross-language retirement/resume reproduction before modifying restoration.
2. Introduce typed source/derivation relationships and context authorization using existing store ownership.
3. Make retirement immediately effective across all reads; add cleanup resumption and status.
4. Add the bounded continuation format and authorized restore/rebuild path.
5. Add compaction through Pi, with independently attributable calls and source closure.
6. Migrate legacy checkpoints conservatively. Those without reconstructable lineage remain non-resumable until a fresh authorized continuation is created.

## 6. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R1-01 | Retrieve a distinctive marker, checkpoint, interrupt, retire its memory, resume. Capture the actual next provider request: marker and dependent summary are absent. |
| R1-02 | Repeat R1-01 for expiry, deletion, access loss, and an actively running agent's next call. |
| R1-03 | Revise a still-authorized artifact. Historical inspection remains possible, but the old result does not satisfy a current-state requirement. |
| R1-04 | Fail midway through derivative cleanup. Searches, events, continuation, and export cannot recover withdrawn content before or after restart. |
| R1-05 | A summary citing another summary inherits the underlying withdrawn source; removing only the original tool result is insufficient. |
| R1-06 | Grow retained visible evidence beyond ten bridge-frame capacities. Persist/restore continuation successfully without increasing the bridge limit or dropping pending-operation identity. |
| R1-07 | Interrupt around compaction and around a pending effect. Resume from real receipts or return explicit unresolved status; never replay an effect to fill a missing transcript slot. |
| R1-08 | Migrate legacy records/checkpoints and inject migration failure. Preserve observations; unsupported continuation fails with an actionable reason. |
| R1-09 | Cross-client content is unavailable both through direct lookup and transitive summary/continuation references. |
| R1-10 | A model-facing tool cannot remove its own context lineage or change source-policy generation to bypass validation. |

**Exit:** All tests pass through actual Rust/Pi integration with captured provider fixtures. A model-backed long-run scenario separately demonstrates that the reconstructed context supports useful continuation. Report those as distinct kinds of evidence.
