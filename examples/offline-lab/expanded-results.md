# External-data pilot results

This pilot is closed. Ribosome completed discovery, contrast review, extraction and all eight function-study executions. The instruction respected an applicability boundary but failed its intended path-resolution function. It was not admitted. E6, the broader system comparison, is deferred.

The campaign began locally on 13 September 2026 and closed on 14 September, using the same twelve episodes as the [first pilot](results.md). The [aggregate results](results/expanded.json) contain run counts, costs, retrieval results and case outcomes. Source transcripts and provider settings remain private.

## Allowance and completed work

The additional allowance was **1,000 calls and US$20**, with up to 100 calls per investigation. The expanded campaign used **311 calls**, with **US$3.058088** in known usage at the configured catalogue rates. One call interrupted during the first contrast rerun retains a **US$0.143052** reservation with unknown final usage. The earlier pilot has its separate unresolved connection reservation.

Individual responses initially had a 4,096-token limit. A continuation summary exhausted that allowance. Later runs allowed 16,384 output tokens per response, including summaries. The completed discovery also used a different configured model from the preceding 100-call attempt, so these observations do not isolate the effect of budget alone.

| Experiment | Observed result |
| --- | --- |
| E0: import fidelity | The existing twelve-episode sample remains available: 12 AEC messages and 870 Nebius messages. Focused importer and publication checks passed. |
| E1: blind audit | The full-message AEC audit saved three findings in ten calls. The plain-transcript control saved two findings in eight calls. Both used the same source window and 100-call allowance. |
| E2: discovery and contrast | Discovery saved a definition, investigation and occurrence in 56 calls; a 17-call follow-up linked them. After the access fixes, a 13-call contrast saved a rejected match against two GeoPandas test excerpts. Earlier contrast attempts remain in the totals. |
| E3: prefix observation | A five-call review of the first 40 Nebius messages flagged a visible metadata parsing error and failing tests. |
| E4: extraction and retrieval | Extraction saved one instruction in four calls. Search returned it first for “workspace package path” and returned nothing for “resolve project directories” or the unrelated engineering query. |
| E5: fresh function test | All eight planned executions completed in 48 model calls. The candidate passed two; baseline passed one. The function study rejected the candidate. |
| E6: system benefit | Deferred for this pilot after the rejected function result. A later comparison needs a candidate with demonstrated local function. No system-benefit executions ran. |
| E7: memory and withdrawal | The focused check used imported Nebius material, interrupted a continuation, withdrew its source and inspected two subsequent provider requests. The withdrawn memory and derived context were absent. This used a scripted provider transport. |

Two earlier discovery attempts remain in the totals: one ended during a continuation summary after six calls; another reached 100 calls with a draft definition and no completed investigation. These are retained experiment outcomes.

## What the candidate carried forward

The discovery describes an EDK2 path utility visible in a software-agent trajectory's source excerpts and tests. Its definition covers workspace and package roots, invalid paths, nested packages and relative-path conversion. This is a repository-specific procedure identified from recorded evidence.

The extracted instruction retained applicability checks and abstention conditions. Its directions for calculating the actual mapping were much less specific. Fresh recipients exposed that gap:

The [sanitized candidate record](results/path-candidate.json) includes its full instruction text, declared input/output contract, original recipient prompt and source attribution. The text is unchanged; provider configuration and local acquisition paths are omitted. It was extracted from the Nebius trajectory for `tianocore__edk2-pytool-library-372`, published in [SWE-rebench OpenHands trajectories](https://huggingface.co/datasets/nebius/SWE-rebench-openhands-trajectories) under CC BY 4.0.

| Recipient task | Baseline | Candidate |
| --- | ---: | ---: |
| Resolve paths from a supplied directory inventory | 0/2 | 0/2 |
| Preserve an already correct result for an incompatible URL request | 1/2 | 2/2 |

All executions preserved the independent metadata field. The candidate consistently left the incompatible result in place. On the applicable task, it confused package-relative and workspace-relative paths. One run produced a correct intermediate mapping, then replaced the workspace-relative answer with an incorrect rejection.

The judge expected path strings or null values; several outputs used nested objects. Manual inspection of the saved edits also found incorrect path mappings in all four failed path executions. The next recipient prompt should state the output shape explicitly so formatting and path interpretation can be assessed separately.

The [saved case outputs](results/path-case-outputs.json) include the original judge reports and each output edit. A review of those saved results distinguishes the failures below. These categories are a later inspection; the original study measured combined quality.

| Case | Condition and repetition | Failure categories | Evidence in the output |
| --- | --- | --- | --- |
| Path resolution | Baseline 0 and 1 | Interface, function | Nested result objects; package path retains the extra `Packages/` prefix. |
| Path resolution | Candidate 0 | Interface, function | Nested result objects; final output rejects the workspace-relative README path. |
| Path resolution | Candidate 1 | Interface, function | Nested result objects; final output retains the extra `Packages/` prefix. |
| URL request | Baseline 0 | Function, unnecessary intervention | Replaces a correct incompatible result with `status: resolved`. |
| URL request | Baseline 1; candidate 0 and 1 | None | Leaves the correct incompatible result in place. |

The updated path-study example declares the same output contract and path semantics to both conditions. Its judge reports interface compliance, functional correctness, preservation and intervention separately. An unsupported output representation leaves functional correctness unassessed. Those changes apply to future runs; the original outcomes above are retained.

Two cases and two repetitions support inspection of these failures. They do not establish a reliable improvement rate. The instruction was not admitted for production use.

## What the reviews found and missed

The larger AEC audit read the complete assistant message and correctly recognized its numeric answer block. Both review conditions distinguished described commands from recorded tool execution and identified revisions within the calculation. Neither identified the remaining charging-power error.

An owner recalculation using the task's supplied formula gives approximately **12.87 Mvar**. The source's last numeric block reports **13.85 Mvar**, about **7.6% higher**. The reference calculation stayed outside both reviewers' assigned evidence. The supplied phase spacings also fail the triangle inequality; evaluating the stated formula and assessing physical feasibility are separate checks.

The prefix reviewer identified a failure already visible in the first 40 software messages. Its finding supports investigation at that point; the experiment measured observation of a current blocker. Later messages remained outside its assignment.

The first contrast called `record_read` with the correct assigned definition ID. Rust returned `artifact observation is absent or inaccessible`. Fixing that read exposed a second part of the same access problem: the host rejected the retained result's supporting files during context authorisation. The reviewer repeatedly lost its prior observations. That rerun was stopped after 27 calls and 12 context segments.

The corrected path checks availability of supporting evidence while keeping donor files outside the assignment. The regression now covers reading, retrieval, the next provider request and withdrawal. A new live run used the same model, definition, challenge assignment and call limit. It completed in **13 calls**, costing **US$0.108129**, with settled usage and one context segment throughout.

Its saved investigation rejected applying the EDK2 path definition to the inspected GeoPandas evidence. The cited excerpts concern `GeometryArray` tests: a missing test assertion helper and a NumPy `copy=False` compatibility error. They provide no support for the definition's workspace-root resolution or package-path procedure. The reviewer inspected two truncated test-output excerpts and neighboring events, and recorded that coverage limit. This is a completed negative match assessment; general transfer remains unestablished.

## Follow-up after review

A [bounded agentic retrieval probe](results/retrieval-followup.json) found the candidate from “resolve project directories” using an initial search and three reformulations, then read its definition and instruction. It completed in eight model calls, costing US$0.064909 at the configured catalogue rates. This one query supports using the existing search tools interactively; finding the candidate does not establish its usefulness.

The [behavior follow-up](behavior-results.md) investigates decisions enacted in another source execution and tests a fresh warning configuration. Its results are separate from this closed pilot.

## Checks and closure

Ten focused Node tests passed for provider configuration, the example command entry point, the independent judge, study setup, transcript visibility, per-run budgets and imported-memory continuation. The command-entry regression failed before the import-loop fix and passed afterward. The live eight-execution study also exercised versioned discovery references and the required experiment configuration through the CLI. Rust lint and documentation checks passed.

The closure check passed 42 focused Rust tests covering corpus access, retained results, context, invocation and source withdrawal, plus six Node tests including continuation with the imported Nebius episode. The context regression failed before the correction and passed afterward. Rust lint passed.

E0–E5 and E7 have recorded pilot outcomes; E6 is explicitly deferred. A failed candidate is a completed experimental result. The [instruction lifecycle example](../learned-behavior/README.md#revise-and-reuse-a-known-instruction) separately verifies revision, admission, memory, retrieval, execution and withdrawal with authored provider decisions and real library operations.

A later research round can examine extraction of complete decision procedures, arithmetic verification, retrieval under different wording and a larger task-grouped cohort. Those questions remain separate from this pilot's closure.
