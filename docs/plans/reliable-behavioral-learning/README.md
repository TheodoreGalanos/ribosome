# Ribosome: reliable recovery and agent-driven behavioral learning
## Next development pass — specification and implementation plan

**Status:** R1–R7 library implementation and handoff tooling closed (7 of 7). Behavioral qualification remains incomplete; the original behavioral exit criteria are not all passed. See [validation status](../../validation.md) for observed results and remaining acceptance work.\
**Revision:** next-pass-draft.1\
**Date:** 12 September 2026\
**Baseline:** `TheodoreGalanos/ribosome` at `2b87a6a0d60694a82d746f1ce0b34740a8fd4e20`. The default branch still pointed to this commit when checked for this plan. [S1]\
**Stack:** TypeScript + the pinned Pi agent-core/Pi AI integration for agentic work; Rust for communication, scheduling, persistence, retrieval, permissions, effects, and evaluation execution.\
**Relationship to the original specification:** This pass completes and hardens the existing v0.1 design. It does not propose a rewrite, a new product, a new language, or an automatic release-version change.

## Decision

Keep the architecture. Develop two connected capabilities: reliable lifecycle behavior and genuine agent-driven discovery, execution, and testing of reusable behavioral motifs.

The pass is complete only when a curator can work from unlabelled execution evidence, discover a useful conditional behavior absent from its initial inventory, package it as agent instructions, and have an actual Pi agent attempt that behavior on a new task. Independent evaluation must report whether the behavior helped. Passing this implementation test does not require manufacturing a positive improvement: unsupported candidates remain rejected or inconclusive.

**A motif is a reusable function or policy—not a name attached to a registered command, and not a transcript excerpt.** The existing registered-tool path remains useful and supported, but it is not sufficient evidence of behavioral discovery.

## What remains unchanged

Ribosome stays independent of AEC-Bench. Keep the Rust library, thin CLI, TypeScript package, local SQLite store, generated cross-language contracts, explicit host lifecycle, and application-owned source agents. Do not rebuild Pi's agent loop, add a second TypeScript database, or require a broker, vector service, graph database, or distributed scheduler.

Caretaker, curator, and experimenter remain profiles over shared capabilities. Agents own interpretation, hypothesis formation, investigation, strategy choice, adaptation, semantic consolidation, and experiment design. Rust provides evidence, searches, executes authorized effects, and applies host-owned policies. Fixed code may validate contracts and observe outcomes; it must not contain a hidden semantic decision tree that supplies the behavior supposedly discovered by the agent.

## Review-to-delivery map

| Concern | Required outcome | Owner |
| --- | --- | --- |
| Retired memory returns through checkpoints | Source-aware context authorization on resume and before subsequent model calls | R1 |
| Retirement propagation can be interrupted | Immediate logical unavailability with resumable, observable cleanup | R1 |
| Full checkpoints grow beyond bridge capacity | Bounded continuation state separate from retained transcript evidence | R1 |
| Filesystem success precedes lost dependency updates | Idempotent finalization shared by normal completion and recovery | R2 |
| Matching desired bytes mistaken for verified application | Separate observed state, established effect occurrence, and validation evidence | R2 |
| Repaired artifact remains marked invalid | Version-bound validity transition for exactly the validated properties | R2 |
| Long commands/evaluations monopolize the runtime | Short state transactions; separately owned execution and bounded concurrency | R3 |
| Evaluator tasks each receive the entire budget | Hierarchical reservation/settlement across all participating executions | R3 |
| Motifs mostly demonstrated with installed procedures | Agent-driven discovery, disconfirmation, semantic definition, and evidence-linked occurrences | R4 |
| Discovered instructions lack a generic tested execution path | Pi-backed implementation invocation and adaptation without name-based dispatch | R5 |
| Search and memory are basic | Scoped retrieval across records and evidence; provenance-aware consolidation and reusable inventory | R5 |
| Procedure comparisons do not show agent benefit | Whole-agent comparisons with retry and critique baselines, ablations, and transfer | R6 |
| Archive mechanics do not demonstrate useful diversity | Contextual alternatives scored from repeated execution evidence | R6 |
| Broad completion claims exceed observable evidence | Integrated fault cases, frozen behavioral evaluation, and sanitized evidence bundles | R7 |

The review's findings are input hypotheses for regression tests. This document does not claim they were all reproduced end to end during planning. Reproduce each against the pinned baseline, keep the failing fixture, then implement and demonstrate the correction. The earlier checkpoint probe was an isolated restoration probe, not a substitute for the Rust/Pi regression required in R1.

## Delivery order

1. **R1 — Context, memory, and continuation correctness.** Library implementation closed under the clarified infrastructure scope; autonomous long-run qualification remains failed and explicitly tracked in the validation report.
2. **R2 — Effect finalization and version-bound validity.** Library implementation closed: recovery, property validity, stopped-executor settlement and combined worker interruptions passed focused checks.
3. **R3 — Responsive execution and aggregate budgets.** Library implementation closed: responsive state requests, retained execution ownership, shared participant budgets, provider-dispatch crash recovery and capacity-one child scheduling passed focused checks. R6 now exercises whole-agent stages under those allocations.
4. **R4 — Agent-driven motif discovery.** Library mechanisms closed: bounded corpus investigation, structured submissions and contrast work are tested. Live discovery saved candidates and investigations but did not deliver a valid grounded occurrence; semantic qualification remains incomplete.
5. **R5 — Executable behavioral implementations and prepared retrieval.** Library mechanisms closed: pinned Pi invocation, host experimental authority, prepared-source restrictions, transplant attribution and explicit retrieval modes are implemented. Nested invocation returns explicit unsupported before dispatch. The bounded live trial saved and executed a curator policy, but did not qualify successful transfer; see the validation report.
6. **R6 — Whole-agent laboratory, transfer, and diverse admission.** Library mechanisms closed: isolated Pi studies, matched workflow controls, shared budgets, protected exposure, case-level uncertainty and aggregate archive admission. The bounded live development study was inconclusive; useful transfer and benefit remain unqualified.
7. **R7 — Integrated qualification and release handoff.** Implementation and handoff tooling closed: installed attachment/recovery/invocation, migration checks and sanitized evidence reporting passed. The installed live curator produced no executable candidate; useful learned transfer and diversity remain unqualified. No package publication or new CI matrix was performed.

Close each PRD against its required behaviour and focused acceptance evidence before advancing. Distinguish implementation gaps from missing demonstrations and model task failures. A failed agent answer does not alone establish a library defect; retain failed qualification evidence without treating verified implementation work as unfinished. Use existing artifact-version checks where needed; equivalent representations do not require byte-for-byte identity.

Do not postpone correctness until after adding learning. A learning system built on stale context or untrustworthy effect completion can amplify errors rather than repair them.

## Package contents and use

The `prds/` directory contains the seven authoritative work packages. Each specifies scope, concrete changes, baseline file ownership, regression tests, and exit conditions. `CONTRACT_CHANGES.md` coordinates the cross-language data changes and includes worked record-chain examples. These source documents are authoritative; there is no separately maintained combined specification.

Suggested landing location in the repository is `docs/plans/reliable-behavioral-learning/`. Update `docs/specification.md`, `docs/protocol.md`, `docs/operations.md`, `docs/validation.md`, and the existing runtime-integration plan as each change lands. The repository's current P4/P5 follow-ons should point to the relevant R5/R6 work instead of becoming a competing roadmap.

## Shared implementation rules

### Evolve existing contracts

Extend the canonical `contracts/schema.json`; regenerate Rust and TypeScript types. Existing concepts already include `Definition`, `Occurrence`, `Implementation`, `Transplant`, `Experiment`, `Evaluation`, `Admission`, `ActionReceipt`, and `Checkpoint`. Reuse these rather than adding a second motif or trial ontology. [S2]

Proposed type names and JSON/YAML examples in the PRDs express required semantics; they are not claims that the APIs already exist. Final wire layouts must be recorded in the canonical schema, conformance fixtures, and protocol documentation before implementation relies on them.

Preserve the distinction between an envelope's record revision and an implementation's semantic version. Use explicit references rather than ambiguous `id@version` strings. Keep readable names for people and the existing operational identifier convention. Existing content digests are appropriate artifact-version checks; do not expand this into cryptographic-ledger infrastructure.

### Migrate deliberately

Changes to persisted records or wire shapes require a transactional migration and explicit compatibility behavior. Preserve old observations; never fabricate missing source lineage, validation certificates, or learned-implementation execution evidence. Old unsupported checkpoints may remain inspectable while continuation requires rebuilding from authorized evidence. Unknown newer schemas must fail clearly.

Maintain a schema-change register across PRs. Allocate actual database/protocol versions from the current repository when implementation begins; do not independently assign conflicting version numbers from this plan. Keep a consistent SQLite backup before migration, test failed migration and restart, and document that an older binary cannot necessarily reopen the upgraded store.

### Distinguish five claims

- An event was observed versus the agent's interpretation of it.
- A behavior occurred versus it achieved its local purpose.
- A local behavior succeeded versus it generalizes to another context.
- A current state matches an intention versus a particular checked effect is proven to have executed.
- Software implements an experiment versus the experiment demonstrates a beneficial treatment.

### Keep the boundary usable

No approval is required for every step inside an existing grant. Agents may investigate, edit isolated material, run checks, adapt instructions, and create bounded follow-up work. The host retains grants, protected evaluation material, publication authority, and budget policy. An agent can propose broader access; it cannot grant it to itself.

Use real Pi integration tests in addition to deterministic infrastructure fixtures. Do not substitute a scripted planner, hard-coded diagnosis, or a prewritten expected motif for the agentic path.

## Final acceptance

All correctness regressions must pass. An independently installed consumer must run all existing demonstrations and the new learned-instruction demonstration. A frozen evaluation must report complete-agent outcomes, negative results, unnecessary interventions, and total cost with explicit uncertainty. No successful transfer or net benefit is claimed merely because a record was admitted or a fixture completed.

No repository changes or paid model runs are performed by this planning deliverable. Source observations and the implementation references appear in `SOURCES.md`.
