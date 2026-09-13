# R6 — Whole-agent experiments, functional testing, and diverse admission

**Implementation status:** Library mechanisms closed. The live development comparison is inconclusive; transfer, learned diversity and integrated attachment qualification remain unproven. See [validation](../../../validation.md#r6-whole-agent-laboratory-library-mechanisms-closed).

**Priority:** Establish what the machinery accomplishes.\
**Outcome:** Learned behaviors are evaluated through actual agent execution; comparative claims include all relevant cost and uncertainty. Useful specializations can be retained without overstating transfer.\
**Dependencies:** R3 and R5; R4 discovery material.

## 1. Keep procedure tests; add real agent evaluation

The current laboratory's protected cases, frozen policies, complete evidence requirements, and separated admission authority remain the foundation. The reference normalizer-versus-unconverted-sum evaluator remains a useful deterministic test. It does not measure the value of adding Ribosome to a capable agent. [S8][S10]

Add a host-owned evaluator adapter that runs an actual subject agent through the ordinary Pi/host tool interfaces. For a learned-instruction candidate, use the same generic invocation path as R5. Do not translate candidate names into prewritten reference procedures.

## 2. Separate actors and information

| Actor | May see | Must not control |
| --- | --- | --- |
| Discovering curator | Authorized source episodes and released development evidence | Protected recipient cases/answers |
| Experimenter | Candidate, baseline, development results, permitted aggregate outcomes | Protected checks, hidden answers, its own admission |
| Subject worker/candidate | Its task inputs, granted tools, condition-specific instructions and memory | Evaluation oracle and sealed outcomes |
| Maintenance agents | Evidence and effects granted for the treatment | Wider tools, hidden answers, or more unaccounted budget |
| Protected evaluator | Expected properties/answers, final artifacts, actual execution evidence | Candidate behavior during execution except declared evaluation controls |

Rust owns execution isolation, budget allocations, and result attribution. A semantic evaluator may use Pi under a separate grant. Model choice alone does not create independence; access separation and protected policy do.

Do not place expected values or test source in a candidate-readable workspace. The evaluator reads output through its own capability. Record contamination tests in both the filesystem and retrieval/context paths.

## 3. Two kinds of study

### Component/function study

Tests whether a particular motif implementation fulfills its stated function under conditions and counterexamples. This can support contextual admission even before claiming that the full Ribosome system saves cost.

Use ordinary `Experiment`/`Evaluation` records with an explicit study objective. Avoid a second measurement framework. Include exact implementation, operator, source corpus, tools, model, binding, and memory starting-state versions.

### System-benefit study

Compares complete agent workflows, including maintenance, against simpler uses of a comparable budget.

At minimum, include:

| Arm | Treatment |
| --- | --- |
| Unchanged worker | Current subject agent without Ribosome assistance |
| Extra retry | Same task capabilities, with additional attempts within the same aggregate allowance |
| Critique-and-revise | Same task capabilities, with an ordinary review/revision loop |
| Ribosome care | Caretaker intervention within the same aggregate allowance |
| Ribosome prepared behavior | Prepared learned instructions/memory plus care, when that is the claim being tested |

The last arm can be omitted from a study whose stated question excludes inheritance, but must be present in the pass's learned-reuse qualification. Report online costs and separately measured discovery/laboratory costs. Any amortization assumption must state the reuse count; never silently hide learning cost.

Do not give a baseline fewer tool capabilities or an intentionally broken algorithm merely to make improvement easy. Same caps do not require every arm to spend the full allowance. Judge achieved outcomes, consumption, and latency together.

## 4. Functional experiments

Support the existing templates through the same generic evaluator:

- **Ablation/knockout:** remove the candidate motif or a named decision/check while keeping the rest and its allowed budget comparable.
- **Rescue/substitution:** restore the omitted component or substitute another implementation.
- **Interaction:** test two components individually and together; do not assume individually beneficial changes compose.
- **Stress:** vary the mechanism that makes the behavior necessary, including missing metadata, unavailable tools, concurrency, and incomplete evidence.
- **Transfer:** evaluate a frozen implementation on recipients distinct from donor/selection material within the claimed applicability.

Interpret ablations carefully. A replacement prompt can change behavior in several ways. Record the exact intervention and avoid claiming a uniquely causal internal mechanism from a crude deletion experiment.

A check that was never needed in the tested cases has not been proven useless. Targeted counterexamples matter.

## 5. Corpus partitioning and experimental discipline

Define discovery, development/selection, and protected recipient partitions by source mechanism/scenario-family lineage before qualification. Use known lineage and source-family IDs to avoid accidental overlap; no cryptographic dataset ledger is required.

Randomly renaming files or changing numbers in the donor episode does not make an independent transfer family. Generated development examples retain their synthetic origin and parent mechanism. They may improve a candidate, but cannot be reclassified as protected evidence after selection.

Freeze the implementation, retrieval corpus version, admission policy, prompts, tool versions, model configuration, and memory starting state before protected qualification. If the candidate is revised, its previous protected run is a recorded development exposure, not a reusable untouched holdout.

A project-wide qualification record should track protected-set consumption across grants. Issuing a new grant must not silently reset a study's holdout-exposure policy. The experimenter cannot evade it by changing only the experiment ID.

For memory-learning comparisons, isolate starting stores and control what persists between experiences. Probe-created state is discarded unless its retention is explicitly studied. The source agent and caretaker must not share memories across control/treatment arms accidentally through the same project namespace.

Randomize or counterbalance execution order where provider/time effects may matter. Keep all failed, cancelled, exhausted, and missing-result cases. An incomplete case does not disappear from the planned matrix.

## 6. Proposed first qualification design

Use at least two task families with different mechanics, not two variants of unit normalization. Suggested families:

1. **Multi-agent integration:** independent contributions must be joined despite incompatible assumptions or missing metadata; benign cases need no correction.
2. **Evidence refresh and regeneration:** a source revision invalidates selected claims/artifacts while independent work must remain valid.
3. **Coding/analysis failure localization** is a useful third family when the local adapter/evaluator can execute it without hidden repair helpers.

A reasonable initial planned matrix is eight recipient cases per family and three repetitions per case per selected arm. This is a proposed small qualification study, not a sufficient sample size for universal reliability. Set the total grant before launching. Reduce the matrix explicitly if necessary and report the reduced evidence; do not selectively stop after favorable runs.

Include source history withheld from the recipient, renamed tools/interfaces, changed decomposition, insufficient evidence, benign lookalikes, and at least one interference case for combined motifs.

## 7. Measurement and acceptance policy

Separate four questions:

| Question | Evidence |
| --- | --- |
| Did the agent recognize the function correctly? | Grounded occurrences, contrasts, blind semantic assessment |
| Did the implementation fulfill its local contract? | Actual artifacts, obligation results, independent checks |
| Did the function transfer? | Frozen candidate on distinct recipients, with donor history withheld |
| Did Ribosome improve the whole workflow? | Matched complete-agent comparison including maintenance and learning costs |

Primary outcome: verified task success under the declared budget. Secondary outcomes: invalid effects escaping the system, unnecessary interventions, preserved valid work, unresolved-but-honest outcomes, total calls/tokens/cost, latency, and recovery overhead. Report unknown usage explicitly.

For repeated runs, report paired case-level differences and uncertainty; repetitions of the same case are not independent task families. Prefer a case-level resampling interval or another preregistered method appropriate to the metric. State the method and sample size. Do not treat provider seed labels as deterministic model executions.

The owner fixes the admission criterion and practical improvement threshold before protected evaluation. A candidate can be:

- **Functionally admitted** for a narrow context when it meets the declared local validity requirement.
- **Supported as beneficial** only when the comparative study supports that particular claim.
- **Rejected or inconclusive** when evidence does not support the relevant criterion.

Do not encode positive uplift as a requirement that forces the testing machinery to find a winner. The implementation can be complete while a particular candidate or system treatment remains unproven.

## 8. Quality-diversity backed by actual outcomes

Retain the small Rust archive. Demonstrate at least two genuinely different implementations of the same function, useful under different conditions, before expanding the archive algorithm.

Descriptors should capture functionally relevant conditions or behavior: uncertainty handling, available tools, degree of decomposition, or resource profile. The study fixes how descriptors are measured; candidates do not win a cell by writing a favorable self-description.

Compute cell quality from the designated aggregate evidence across repeated cases, not the best single lucky evaluation. Keep exact implementation version, context, policy, admission reference, evaluation set, and limitations with each entry. Retirement of required evidence/implementation invalidates its usable entry under R1.

Archive presence does not override permission or applicability. Runtime agents still investigate fit. Keep evidence inventory separate so displacing an elite does not destroy rejected discoveries or failed-run fragments.

## 9. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R6-01 | The evaluator runs a curator-produced `instructions` candidate through actual Pi and general task tools. No name-based solution selector exists. |
| R6-02 | Candidate and baseline see equivalent task capabilities and comparable aggregate budgets; every participating source/maintenance/evaluator call is attributed. |
| R6-03 | Hidden answers/check code are inaccessible through artifacts, evidence, search, memory, and continuation. |
| R6-04 | Development-generated cases keep source/synthetic lineage and cannot directly certify admission. |
| R6-05 | Reusing a protected corpus via a fresh grant remains visible and controlled by the same qualification policy. |
| R6-06 | An exhausted or missing arm/case makes the comparison incomplete rather than increasing the pass rate. |
| R6-07 | Ablation, rescue, and interaction use actual modified agent material and fresh runs, not manually supplied scores. |
| R6-08 | A frozen learned behavior is attempted on distinct recipients with donor history withheld. Independent checks establish success/failure and limitations. |
| R6-09 | At least one benign case rewards appropriate non-intervention. More findings alone cannot improve the score. |
| R6-10 | Two contextual implementations can coexist in the archive. A lucky single trial cannot displace one supported by the fixed aggregate policy. |
| R6-11 | Complete-agent result reports include unchanged/retry/critique controls and confidence/uncertainty treatment without asserting general reliability from one set. |
| R6-12 | Semantic evaluation disagreement is retained; function names and exact transcript spans are not the sole ground truth. |

**Exit:** The laboratory can support, reject, or leave inconclusive both functional and system-benefit claims from actual agent executions. Publish the resulting limitations as prominently as positive results.

## 10. Primary files

`crates/ribosome-core/src/{experiments,accounting,inventory,records}.rs`; protected evaluator adapters; `packages/agents/src/operators/index.ts`; generic implementation invocation; `tests/evaluations/`; new neutral examples alongside, not replacing, the existing local normalizer example.
