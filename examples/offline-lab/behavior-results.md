# Investigating an agent strategy

This follow-up asks what decisions an agent made during a real repair, then tests an extracted instruction on a fresh warning-configuration task. The [runnable example](README.md#investigate-an-agent-strategy), [generated records](results/behavior-records.json) and [source excerpts](results/behavior-evidence.json) make the investigation inspectable.

## Evidence and owner choices

The source is the `numpy__numpydoc-101` execution in [Nebius SWE-rebench OpenHands trajectories](https://huggingface.co/datasets/nebius/SWE-rebench-openhands-trajectories), published under CC BY 4.0. The result bundle records the dataset revision and source episode identity. The importer rebuilt these events with call/result parents and separate message order.

The owner selected 53 messages for discovery: the task at index 1 and indexes 36–87. The 20-message challenge uses indexes 88–107 from the same execution. These are development windows chosen after inspecting the source. The challenge tests a nearby boundary; it is dependent evidence from the same task.

The selected trace contains a concrete sequence:

- The source agent considers where warning context is available and chooses a warning route.
- Its first repair emits duplicate generic warnings. Later output removes the duplication but still lacks context.
- The agent reads `FunctionDoc`, observes that its function is stored as `_f`, and revises an earlier `_obj` assumption.
- A subsequent check shows an object and filename. A context-free check still emits a generic warning.

The later window shows a similar problem for `ClassDoc`, which uses `_cls`, followed by another repair and check. These observations let a reviewer examine decisions, actions and consequences separately from the parser procedure described in the source code.

## Discovery and extraction

The first general investigation completed but produced a domain procedure: emit a contextual warning once. The owner then asked specifically for a strategy enacted by the agent, without supplying a strategy name.

That attempt saved a strategy definition but repeatedly submitted inconsistent occurrence boundaries. It stopped during compaction with `compaction ended without a summary and completed model usage`. A new owner-scheduled continuation read the saved definition, received the validation errors as guidance, and completed an occurrence and investigation. The failed attempt remains in usage and outcome records.

The strategy is to inspect the warning route and actual wrapper metadata, adapt the change when observed output contradicts an assumption, and check the contextual and context-free paths. Its strategic dependency links are marked `inferred`.

A separate curator supported the narrow boundary in the later window. Its review concentrated on messages 91 and 99 and left other wrapper types unresolved. Extraction then saved an instruction containing observations to obtain, conditional actions, result checks and limits. Its full text and declared recipient bindings appear in the [record bundle](results/behavior-records.json).

Owner review found limits in these saved interpretations. The discovery asks why the final warning still looks generic, although message 85 contains context. The contrast leaves `ClassDoc` unresolved despite the assigned window containing its later check at 107. The records preserve those statements; the source excerpts let readers assess them. Completing the records did not make every interpretation correct.

The extracted instruction linked directly to its supporting definition. The example runner now accepts definition, occurrence and investigation links supported by a completed investigation, matching the library's invocation contract.

## Fresh function test

The recipient repairs a JSON warning configuration interpreted by a fixed checker. It can read the warning routes and wrapper fields, observe current diagnostics, edit a sandbox branch and check the result. The checker executes no recipient-authored code.

Both conditions receive the same output fields and diagnostic requirements. The applicable case combines an incorrect context-field assumption with duplicate warnings. The nearby benign case already emits the correct generic warning. Expected diagnostics remain with the independent judge.

The judge measures output interface, functional correctness, preservation of an independent note and unnecessary editing separately. A malformed interface leaves functional correctness unassessed. This is a small configuration-repair mechanism test, with two cases, baseline/candidate conditions and two repetitions.

The first eight executions completed. The candidate was rejected and was not admitted. This first task contract needed a clarification, described below.

| Recipient | Baseline | Candidate |
| --- | ---: | ---: |
| Repair context and duplicate warnings | 2/2 | 1/2 |
| Preserve already-correct context-free warnings | 2/2 | 2/2 |

Every output met the declared interface and preserved the independent note. The failed candidate run removed the duplicate warning but still lacked required object/file context. It wrote `readGauge@gauge.js` into `context_field`, where the checker expected a wrapper-property key. The prompt declared the field type but did not explicitly explain that lookup operation. It ran the diagnostic tool after its edit; that tool displayed generic warnings, which the independent judge correctly rejected for the wrapped input. This was a functional error in a completed execution.

The [case bundle](results/behavior-followup.json) retains the original judgments, recipient contract, expected diagnostics, final configurations and action evidence. Every applicable execution made one edit and ran diagnostics before and after it. Each benign execution ran diagnostics once and made no edit. Some first check attempts were denied because sandbox checks require a branch; those attempts are retained too.

The original result counter recorded zero diagnostic calls because it counted `execute` actions but missed `check` actions. The example now counts both. The bundle preserves the original judgments and reports the receipt-derived diagnostic counts separately. Function and pass/fail logic are unchanged.

The first results establish completed execution and identify an output error, but the missing field semantics limit what they say about instruction quality. Baseline inferred the intended lookup in both repetitions.

## Repeat with explicit field semantics

The owner clarified that `context_field` is a key read from `probe.wrapper`, that its selected value supplies `name` and `file`, and how the parser and setter switches control warning emission. The prompt also states that sandbox diagnostic checks use a branch. Both conditions received those clarifications. The candidate text, case inputs, oracle answers, repetitions and budgets were unchanged; the corrected diagnostic counter applied to the repeat.

This is a development repeat after inspecting results. It is not a protected holdout or an isolated test of the effect of wording.

All eight repeat executions completed and passed: both conditions repaired both applicable trials and preserved the already-correct configuration twice. Every output met the declared interface and preserved the independent note. The function study accepted the candidate for these cases. Production admission was not requested.

| Recipient, explicit contract | Baseline | Candidate |
| --- | ---: | ---: |
| Repair context and duplicate warnings | 2/2 | 2/2 |
| Preserve already-correct context-free warnings | 2/2 | 2/2 |

The [repeat case bundle](results/behavior-contract-repeat.json) includes the exact shared prompt, original judgments, final configurations and action receipts. The candidate ran diagnostics before and after one edit in both applicable repetitions. Baseline also reached the required output, with different diagnostic sequences.

This completes a local path from observed agent decisions to a tested instruction. It establishes function on these cases, with no measured advantage over baseline. The shared task itself requests inspection and diagnostic checks, so it supplies part of the strategy to both conditions. A benefit study needs tasks that declare the goal and interface while leaving more of the decision procedure to the candidate.

The whole-system comparison remains deferred. The record bundle and nearby source contrast provide concrete material for revising the instruction and study design.

## Usage and checks

The behavior campaign, including both function studies, used **178 calls and US$1.429113** in known catalogue-priced usage, with all calls settled. It includes the failed strategy attempt. The first function study used 46 calls and US$0.171403; the explicit-contract repeat used 43 calls and US$0.117244. The separate retrieval probe adds eight calls and US$0.064909.

| Stage | Calls | Known cost, US$ | Outcome |
| --- | ---: | ---: | --- |
| General discovery | 36 | 0.443362 | Completed domain-procedure discovery. |
| Focused strategy attempt | 17 | 0.193331 | Definition saved; compaction interrupted completion. |
| Owner-scheduled continuation | 18 | 0.335058 | Occurrence and supported investigation saved. |
| Nearby contrast | 12 | 0.113821 | Narrow dependent contrast completed. |
| Extraction | 6 | 0.054894 | Instruction saved. |
| First function study | 46 | 0.171403 | Eight executions completed; candidate rejected. |
| Explicit-contract repeat | 43 | 0.117244 | Eight executions passed; candidate and baseline tied. |

Focused importer regressions cover call/result dependencies and changed local profile mappings. The Node checks cover the two judges, supported candidate linkage, tool contracts and withdrawal during continuation with imported Nebius evidence. Rust lint, TypeScript compilation and documentation checks complete the implementation validation.

## Retrieval follow-up

The original path candidate was also found from the previously unsuccessful request “resolve project directories.” The agent ran an initial lexical search and three reformulations, read the definition and instruction, then saved applicability and remaining uncertainty. It used eight model calls and US$0.064909 in known catalogue-priced usage. The [search queries and selected records](results/retrieval-followup.json) are retained.

This one query supports trying bounded search reformulation with existing tools. It does not measure retrieval reliability or establish that the path instruction works.
