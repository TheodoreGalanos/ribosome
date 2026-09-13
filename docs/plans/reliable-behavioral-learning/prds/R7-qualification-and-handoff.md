# R7 — Integrated qualification, migration, and development handoff

**Implementation status:** Qualification/handoff tooling and installed mechanical checks completed. Demonstration A passes with provider fixtures; live B remains incomplete and C remains inconclusive. The original behavioral definition of done is not fully met. See [validation](../../../validation.md#r7-integrated-qualification-and-handoff).

**Outcome:** The complete pass is usable by a separate consumer and its evidence supports exactly the claims reported.\
**Dependencies:** R1–R6.

## 1. Three integrated demonstrations

### A. Repair, interrupt, and restore validity

Attach to an application-owned worker/planner workflow. Introduce a relevant source change, preserve an independent artifact, and request permitted maintenance. Interrupt after an actual mutation but before final bookkeeping, then resume.

The final result must include coherent effect evidence, restored validity for the checked current artifact, stale state for unvalidated downstream work, and no duplicated effect. Retire a source memory before another continuation and prove it does not re-enter the next model request.

This combines R1/R2 with the existing attachment protocol rather than demonstrating each in a disconnected unit test.

### B. Discover a behavior and execute it elsewhere

Give the curator bounded raw traces without the desired motif definition or a solution tool. Include a globally failed run with locally useful behavior and a benign contrast. The curator investigates, formulates a conditional definition, locates occurrences, and produces an instruction-based candidate.

Run local functional tests, freeze the selected version, and invoke it in a new recipient through the generic Pi path. The recipient cannot access raw donor history. The evaluator checks actual outcomes and preserved work. Include one recipient where the correct behavior is abstention or an unresolved obligation, not forced repair.

The demonstration fails qualification if a host helper supplies the discovered policy, if the evaluator switches on the candidate's name to run a prewritten solution, or if a stored narrative substitutes for fresh execution.

### C. Compare benefit and preserve a useful alternative

Run the R6 complete-agent study with shared aggregate accounting. Retain an inexpensive routine implementation and a more investigative implementation only where the fixed contextual evidence supports them. Demonstrate selection/adaptation in a receiving workflow and an inappropriate-transfer rejection.

A positive benefit is not assumed. An honest rejected or inconclusive candidate is a valid laboratory result, but must not be reported as a proven improvement.

## 2. Required test layers

**Rust mechanical tests:** source availability, cleanup recovery, effect phases/finalization, version validity, accounting, concurrency, query filtering, work leases, migration, and exports.

**Cross-language/Pi integration:** actual supported Pi loop, captured provider requests, continuation rebuild, typed invocation, deadlock-free nesting, interrupted effects, cancellation, malformed/oversized frames, and pinned configuration.

**Model-backed capability evaluations:** discovery, contrasts, semantic boundaries, extraction, invocation, adaptation, appropriate abstention, and transfer. These are separate from tests with scripted provider responses.

**Installed consumer:** packed TypeScript package and installed Rust binary run the existing attachment tests plus demonstration B without source-checkout paths or an AEC-Bench/Python runtime dependency.

Keep existing regression coverage and the normalizer demos. New behavior must not silently remove the working first-version paths.

## 3. Migration and compatibility checklist

- Update only the canonical schemas, regenerate both languages, and verify reproducibility in CI.
- Test current-store upgrade, interrupted upgrade, reopen after upgrade, and rejection of an unknown newer schema.
- Preserve immutable source observations; do not infer missing historical validation or source manifests during migration.
- Mark unsupported legacy continuation explicitly and support rebuilding from permitted evidence.
- Negotiate any added bridge/host capabilities. Old clients must receive documented defaults or an explicit compatibility error, not silently different semantics.
- Keep deployment local and dependency growth justified. Pi version upgrades are separate from this pass unless required; then include adapter conformance and restore tests.
- Do not replace human-readable IDs with a new hashing scheme.

## 4. Qualification evidence bundle

Produce a sanitized bundle containing the source commit, dependency/runtime lock identifiers, configuration, corpus partition/lineage manifest, exact candidate/operator versions, planned comparison matrix, budget allocations, aggregate outcomes, uncertainty method, selected redacted receipts, and separate mechanical/model-backed reports.

Do not include provider credentials, private client transcripts, inaccessible raw holdouts, or hidden model reasoning. Where protected material cannot be published, publish the protocol and accessible development examples plus an explicit description of what remains private. Sanitization must not turn an incomplete study into an apparently complete one.

Report failures and unknown usage. Distinguish a successful local fixture, a successful installed integration, a model-backed capability result, and measured system benefit. Do not equate package publication with production readiness.

## 5. Training exports

Carry R1 source validity and R2 receipt truth through export. Preserve observed versus re-executed versus synthetic origins. New learned implementations and their evaluations retain scenario lineage and split restrictions. Retired material cannot return through copied summaries or generated examples.

A corrected training trajectory must have new compatible observations or an explicit synthetic label. Do not simply remove the bad step and retain the old downstream tool responses. Export both recovery and clean-success examples where the protocol selects them.

## 6. Pull-request handoff requirements

Each PR reports: requirement/test IDs addressed, baseline reproduction where applicable, files/interfaces changed, schema/migration implications, tests actually run, model calls and spend if any, unsupported capabilities, and remaining risks.

Do not create unrequested project journals. Durable decisions belong in code, tests, the existing docs, or the corresponding PR/issue. Avoid broad refactors beyond what the acceptance criteria require.

Suggested PR slicing within large work packages:

| Package | Reviewable slices |
| --- | --- |
| R1 | Source relationships and tombstones; authorized context/restore; bounded continuation/compaction |
| R2 | Reproductions and effect phases; idempotent finalization; version-bound validation/application |
| R3 | Short-lock executor ownership; hierarchical accounting; nested invocation and contention tests |
| R4 | Evidence/corpus access; semantic records and curator loop; contrast/novel-discovery evaluations |
| R5 | Generic instruction invocation; prepared retrieval/adaptation; memory integration |
| R6 | Whole-agent evaluator; frozen comparative study; contextual archive/evidence reporting |
| R7 | Installed demonstration, migrations, integrated faults, docs and sanitized handoff |

These are delivery slices, not elapsed-time estimates. Keep shared schema ownership explicit across parallel PRs.

## 7. Final definition of done

All R1–R3 correctness and isolation tests pass. R4/R5 execute real agent-driven discovery and instruction reuse, including a behavior absent from the starting inventory. R6 runs protected whole-agent comparisons and reports their actual result without forcing a positive claim. Existing features still work in an installed consumer. Documentation names the exact supported guarantees and limitations.

The practical outcome is a small system that can learn something worth testing from agent execution—and whose memory, actions, validity state, and evaluation remain trustworthy while doing so.
