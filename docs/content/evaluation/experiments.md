---
title: "Experiments"
description: "Test a procedure's function or compare complete agent workflows."
---

# Experiments

The laboratory runs comparisons using cases, evaluators, and acceptance rules supplied by your application. An experimenter can propose a study and interpret its results. The host executes the comparison and writes the evaluation and admission records.

For experiments starting from public trajectories, see [External agent records](external-records.md).

## Choose the question

| Study objective | Question |
| --- | --- |
| `function` | Does the candidate perform its declared local function on these inputs? |
| `system_benefit` | Does adding it improve verified task success compared with the available alternatives? |

A system-benefit study compares five arms: the ordinary agent, another attempt, critique and revision, caretaker help, and the candidate behavior. Each arm starts with the same ordinary worker. The host defines any later stages and gives each case its own ceiling within one shared budget.

Use a function study to test a specific procedure. Use a system-benefit study when deciding whether its contribution justifies the additional work.

To check the complete library workflow, begin with a known instruction and a deliberate error. The [instruction lifecycle example](../../../examples/learned-behavior/README.md#revise-and-reuse-a-known-instruction) carries it through revision, testing, admission, memory, later use and reconsideration. Its provider fixture makes the decisions repeatable; its live configuration lets you measure a model's decisions separately.

## Define and run a study

1. Register cases, evaluators, and an `AdmissionPolicy` in host configuration.
2. Save an `Experiment` identifying the implementation versions, arms, cases, repetitions, starting memory, measurements, and budget.
3. Run it through the host's laboratory, or start it directly with the command below.
4. Read the full report and any resulting contextual admission before reusing the candidate.

```sh
ribosome study CONFIG.json EXPERIMENT_ID
```

The study fixes its inputs and policy when it starts. Use development cases while refining a candidate. Reserve protected cases for acceptance; their exposure is tracked by client, project, and family so later studies can recognize previously used material.

See the [configuration guide](../reference/configuration.md) for the relevant host fields and the [laboratory reference](../../protocol.md#whole-agent-laboratory) for their contracts.

## Run complete agents

Register an `agent_evaluators` entry to run actual Pi subjects with prepared instructions. Each case gets a fresh workspace, project namespace, restricted grant, and selected starting memory.

A host can define up to four stages per arm. A later stage receives the preceding summary, current branch, and fresh receipt references. For example, a caretaker can work on the ordinary worker's result. Local case exhaustion leaves other funded cases eligible to run.

The built-in adapter starts each case with fixed memory. Use a host evaluator with explicit learning-state handling for studies that retain learning across cases.

## Check task outcomes

The protected judge receives expected answers and other oracle data after subject execution, outside the subject's workspace and model context. Built-in judges are offline commands that inspect the actual result. Optional secondary judges retain disagreements as inconclusive outcomes.

A failed task is an observed result. A judge that cannot execute is an evaluation failure. Reports preserve both, along with exhaustion, cancellation, and cases that never ran.

An LLM judge needs a host evaluator that accounts for its provider calls through `EvaluationAccount`. Subject, maintenance, compaction, and metered evaluator calls share the study's allowance.

## Read the report

Start with verified successes out of all planned cases. Then read execution status, usage completeness, case and family counts, and the paired comparisons. System-benefit acceptance requires the declared improvement over every control and complete usage.

Learning cost records its observed amount, completeness, and declared reuse count. Report it alongside execution cost when assessing the total cost of reuse.

An admission supports one implementation version in a stated context. The archive can retain evaluated alternatives for different conditions, with their full evidence references. [Current support](support.md) summarizes the completed development study and its inconclusive result.
