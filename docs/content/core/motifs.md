---
title: "Behavioral motifs"
description: "Separate a pattern, an occurrence, and a reusable implementation."
---

# Behavioral motifs

A behavioral motif describes a useful function in an agent's work, such as checking an artifact before handoff. It is more specific than a label like “verification,” but need not prescribe every tool call.

## Three separate records

| Record | Question |
| --- | --- |
| Definition | What behavior are we describing? |
| Occurrence | Where did it appear, and what evidence supports that interpretation? |
| Implementation | What instructions or procedure could produce it again? |

A separate admission record states the context in which an implementation has been accepted for reuse.

## Example: check before handoff

A worker produces a report, runs a check, and sends it to a planner.

The definition describes checking the artifact that will actually be handed off. The occurrence identifies the worker's actions, the check result, and the report version. The implementation describes how to repeat that behavior.

A later edit may make the earlier check insufficient. The maintenance agent needs to investigate what changed; the mere presence of a check does not settle the question.

## Keep local results in context

A failed task can contain a useful procedure. A successful task can contain an unnecessary step.

Recognition and outcome are recorded separately. An obligation can also remain open or unknown while the task is still underway.

## Discover and prepare behavior

Curators inspect selected execution evidence, search nearby events, and compare examples. A definition describes the conditions, required observations, and expected result of a behavior. An occurrence ties that interpretation to retrieved evidence, including events across producers.

A curator can save an inconclusive investigation when the evidence is insufficient. When a candidate is ready, extraction produces an implementation. Ribosome can execute prepared instructions through Pi with task-specific bindings, or invoke a host-registered procedure.

Start with [Discover a behavior](../guides/discovery.md), then [Reuse a procedure](../guides/reuse.md). [Current support](../evaluation/support.md) describes the observed learning trials.
