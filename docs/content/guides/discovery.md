---
title: "Discover a behavior"
description: "Ask a curator to investigate execution evidence and describe a useful local function."
---

# Discover a behavior

Give a curator a set of execution observations to investigate. It can compare decisions, examine their consequences, and describe a behavior that may be useful elsewhere.

For example, a task may fail overall even though one worker correctly reconciled conflicting measurements. A discovery investigation can preserve that local behavior with the conditions under which it worked.

## Supply the evidence

Capture the source events and artifact versions first. Then register a `DiscoveryCorpus` through `Store::register_discovery_corpus`, or include it in the CLI configuration's `corpora` list. Set `request.discovery_corpus` to its `{ id, version }` reference.

The corpus selects event windows, historical artifact snapshots, and any definitions the curator may inspect. It also records whether the investigation represents information available during execution or a retrospective view.

For an investigation meant to represent a decision during execution, supply only evidence available at that point. Use a separate retrospective investigation to study later outcomes.

## Run the investigation

Use the `curator` profile and `discovery@1` operator. The curator can read the assignment, search raw events, inspect neighboring events, and retrieve the selected artifact versions. It can request a bounded `contrast-motif@1` child to compare an interpretation with alternatives.

The host checks that cited sources were retrieved. The curator supplies the interpretation: what conditions matter, what action serves the function, and what the evidence supports.

## Read the result

| Record | What to inspect |
| --- | --- |
| Definition | The function, conditions, required observations, and expected result. |
| Occurrence | The actual events and artifact versions supporting the interpretation. |
| Discovery | The investigation, alternatives considered, uncertainty, and outcome. |

`no_motif` and `inconclusive` are useful outcomes when the supplied evidence does not support a candidate. Review the saved records before requesting extraction into executable instructions.

## Try the learning example

Start with the [agent-strategy walkthrough](../../../examples/offline-lab/README.md#investigate-an-agent-strategy) to investigate an imported execution, challenge a proposed behavior and test extracted instructions. Its [recorded follow-up](../../../examples/offline-lab/behavior-results.md) includes source excerpts, owner interventions, the instruction and both function studies. With an explicit task contract, candidate and baseline each passed four executions; this establishes local function, not an advantage.

The [installed-consumer example](../../../examples/learned-behavior/README.md) is a smaller alternative using authored source episodes. Its earlier R7 live trial stopped after an inconclusive investigation. Both examples make model calls when run live; see [Current support](../evaluation/support.md) for results and limits.

Next: [Reuse a procedure](reuse.md).
