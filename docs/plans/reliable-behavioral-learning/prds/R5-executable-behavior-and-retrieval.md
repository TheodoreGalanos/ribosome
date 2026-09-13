# R5 — Executable behavioral implementations and prepared retrieval

**Priority:** Turn discovered knowledge into something agents can use.\
**Outcome:** An instruction-based candidate produced by the curator runs through the actual Pi loop, adapts to recipient state, and generates fresh observations without a hard-coded implementation-name dispatcher.\
**Dependencies:** R1, R2, R4; R3 for accounting and nested execution.

## 1. Preserve the existing formats; add the missing generic path

The current implementation contract already has `format: "instructions"` and `format: "registered_tool"`. Use those formats. Do not rename them to new enums merely because this document describes "agentic implementations." [S2]

The registered-tool `ActionKind::Execute` path remains supported. Instruction material must become a first-class, pinned input to an ordinary Pi-backed agent run. The same runtime path must evaluate candidates and execute admitted implementations; only the grant/visibility/admission authority differs.

No evaluator or runtime may select a solution by comparing a learned implementation's name/material with a special list of demo procedures. A generic instruction executor loads the submitted material and runs Pi. It does not `eval` agent-written TypeScript.

## 2. Instruction implementation contract

Retain current name/version, motifs, material, parameters, required capabilities, state assumptions, possible effects, failure behavior, and evaluation references. Add the following where not already representable:

| Element | Meaning |
| --- | --- |
| Input and output contracts | Bounded structured bindings and required produced evidence/artifacts |
| Entry/exit obligations | What must be established before and after attempting the behavior |
| Conditional policy | The actual agent instructions, including uncertainty handling and alternatives |
| Adaptation slots | What may be rebound locally without silently changing the evaluated implementation |
| Known limitations | Conditions under which applicability or usefulness is untested |
| Discovery linkage | Exact source definition/occurrence/discovery versions |

Instruction material describes goals, decisions, checks, and termination—not just a polished story about the donor run. It must be useful with new artifact paths, observations, and tool bindings.

Parameter schemas validate shape and named capabilities, not arbitrary executable validators supplied by the agent. They do not prove semantic compatibility. Tool availability is necessary, not sufficient, for a transfer.

### Example instruction material

```text
Given independently produced contributions and a required join:
1. Inspect the task-relevant assumptions for each contribution.
2. Establish compatibility from source evidence. Do not assume that matching
   field names imply matching units, periods, revisions, or populations.
3. For a supported mismatch, repair or transform only affected contributions.
4. If critical assumptions cannot be established, return an unresolved join
   with the missing evidence rather than inventing a conversion.
5. Combine compatible results and run the applicable host checks.
6. Return the combined artifact, compatibility evidence, transformations,
   preserved work, and remaining limitations.
```

This policy can lead to different tool calls in different recipients. Its decisions are made by Pi, not a hidden command-selection table. A test should demonstrate two different branches, including a legitimate abstention when information is unavailable.

## 3. Generic invocation through existing machinery

Extend `AgentRunRequest` with a versioned optional implementation invocation, or introduce a small typed invocation request translated into the existing work/run records. Specify exact implementation reference, validated parameter bindings, recipient state/evidence references, parent work, budget allocation, and execution purpose.

Use a generic operator such as `execute-motif@1`. The operator loads the pinned instruction material through Rust and presents it at the correct instruction priority beneath owner policy/grants. Pin the implementation and binding snapshot in checkpoints and results. A narrative statement that an implementation was used is not sufficient attribution.

Production invocation requires contextual admission. An explicitly authorized experimental invocation may execute an unadmitted candidate inside the laboratory. That authority comes from the evaluator/host path, not a model-set `experimental: true` escape hatch on an ordinary tool.

Required execution trace:

```text
selected implementation + receiving context
    -> Rust admission/experimental-authority check
    -> typed invocation and root allocation
    -> Pi receives exact instructions and permitted tools
    -> Pi investigates and takes actions through Rust
    -> new receipts/artifacts/obligation evidence
    -> invocation result linked to the selected version
```

Use the existing supervisor and work queue. Nested invocations are scheduled asynchronously with handles and proper parent parking under R3. Do not hold an effect lock, database transaction, or the last worker slot while waiting for a child to run.

## 4. Adaptation and recombination

The recombinase investigates a desired state transition, searches prepared implementations, checks preconditions, and proposes bindings or modifications.

A binding fills a declared slot without changing the implementation policy. A material instruction change creates a derived implementation version/candidate with provenance. It must not inherit its parent's evaluations or admission as if those measurements applied unchanged. The laboratory may test whether the change is compatible; do not assume it.

Expand the existing `Transplant` record with actual invocation/run references, recipient entry state, resulting evidence, and remaining incompatibilities. Keep donor version and local binding snapshot explicit.

A transplant does not require the original agent runtime to be Pi. The maintained host can remain any supported harness. Ribosome's own execution of the learned capability uses Pi; integration with an external source agent can deliver advisory material or request a permitted action at a host-selected boundary.

The default reuse path consumes prepared material, not raw donor history. Metadata may retain provenance references, but that does not automatically grant access to the underlying donor transcript. If further source investigation is necessary, create a separately scoped/budgeted curator work item. Restrict the tools/grant for the reuse run accordingly, rather than relying only on hiding one tool description.

## 5. Better retrieval without making retrieval the reasoner

The current tool documentation correctly distinguishes record search from event evidence reads. Preserve existing defaults while adding explicit search targets. [S11]

Rust should support bounded record/event search, exact filters, semantic-function metadata, capability/effect eligibility, scope, admission context, source freshness, and stable pagination. The agent decides queries, evaluates results, seeks alternatives, and chooses or abstains.

For the first improvement, keep SQLite FTS5. Benchmark current literal-AND behavior and ID ordering against lexical alternatives and relevance ranking on a fixed retrieval corpus. Add a tested explicit query mode rather than silently changing existing callers' semantics. Use normal schema fields for function, applicability, and capability metadata instead of relying on incidental words embedded in a JSON body.

Apply scope and hard eligibility before returning titles, snippets, counts, or bodies. Filter retired/invalid sources before presentation, not only when full content is opened. Apply R1 to caches and derived indexes. Permission-aware queries must not leak another client's match counts.

Use cursor/keyset pagination bound to query and index generation where needed. Avoid repeated full offset scans and ambiguous "next offset" behavior at the end. Exact-ID lookup remains distinct from semantic retrieval.

Do not introduce embeddings solely because the domain is semantic. Add them only after recording important misses that query reformulation and lexical/metadata retrieval cannot address. A vector score would still not establish applicability.

## 6. Memory integration

Use the existing memory kinds and records. The curator should deliberately consolidate episodes into hypotheses or procedures after closed windows, repairs, or explicit requests. It must distinguish a one-run temporary condition from a change worth inheriting.

A useful failure memory links a mechanism, evidence, benign counterexamples, possible responses, and regression cases. The caretaker interprets that material; it does not become an unconditional ban or automatic rule.

Conflicts are retained with their evidence. Supersession/retraction updates retrieval and context using R1. A finding about one client or tool version must not silently become a global instruction. Propagate applicability and source versions into implementation retrieval.

## 7. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R5-01 | A curator-authored `instructions` implementation is invoked by the generic Pi path. There is no corresponding host-registered solution procedure or implementation-name branch. |
| R5-02 | The same implementation handles two recipients requiring different investigative/actions paths and abstains on a third unsupported context. |
| R5-03 | Required capabilities and task tools are identical across candidate and baseline eligibility; treatment does not secretly receive the complete solution tool. |
| R5-04 | The receiving run has no raw donor-history access. It executes prepared instructions and obtains new tool observations. |
| R5-05 | Semantic incompatibility survives a high lexical match. The agent rejects or adapts the transfer rather than executing it automatically. |
| R5-06 | Material changes produce a new candidate reference. Existing admission cannot certify the modified policy. |
| R5-07 | Parent/child invocation with one worker slot completes under R3 parking semantics, or returns explicit unsupported scheduling before dispatch. |
| R5-08 | Cross-client, expired, and retired sources cannot enter retrieved snippets, invocation instructions, summaries, or continuation. |
| R5-09 | Retrieval tests measure top-k usefulness, eligibility errors, pagination completeness, and agent search effort—not only whether one seeded keyword returns a record. |
| R5-10 | A memory-derived response is inappropriate for a benign lookalike. The caretaker investigates and avoids unnecessary repair. |
| R5-11 | Existing registered-tool demonstrations still execute through their original supported path. |

**Exit:** At least one genuinely curator-produced conditional instruction implementation runs end to end through Pi, with exact version attribution and fresh evidence. Functional success, transfer, and net benefit are assessed separately in R6.

## 8. Primary files

`packages/agents/src/{operators/index,tools/index,pi/agent,profiles/index}.ts`; `crates/ribosome-core/src/{records,inventory,work,supervisor,rpc,experiments}.rs`; canonical invocation schemas; generic candidate-execution integration tests. Extract modules only where ownership is clearer; do not create a new agent framework.
