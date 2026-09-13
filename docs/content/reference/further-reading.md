---
title: "Detailed references"
description: "Contracts, connector details, operating procedures, and validation evidence."
---

# Detailed references

Use these repository documents when implementing an adapter, operating stored state, or reviewing the evidence behind a capability.

| Document | What it contains |
| --- | --- |
| [Connector reference](../../attachments.md) | Pi and generic events, feedback delivery, steering, and coordinated repair. |
| [Protocol](../../protocol.md) | Worker and host interfaces, source-aware context, resource accounting, prepared invocation, and laboratory contracts. |
| [Operations reference](../../operations.md) | Detailed inspection, settlement, backup, migration, and cleanup procedures. |
| [Validation](../../validation.md) | Recorded automated checks and live trials, including incomplete outcomes. |
| [Qualification evidence](../../qualification/evidence.json) | Sanitized machine-readable snapshot of the R7 handoff and R6 development recheck. |
| [Canonical schema](../../../contracts/schema.json) | Shared data contracts and method shapes. |

## Examples

The [attached-agent driver](../../../examples/attached-agent/demo.mjs) demonstrates startup or mid-run observation and checked repair. The [learning example](../../../examples/learned-behavior/README.md) prepares discovery evidence and attempts extraction and recipient evaluation from an installed consumer.

[Evaluation cases](../../../tests/evaluations/README.md) describe the repository's other runnable development tasks and their acceptance checks. Live examples use the configured model provider and retain their reports locally.

## Design and implementation plans

The [design specification](../../specification.md) records the design's rationale and intended scope. The [R1–R7 implementation plan](../../plans/reliable-behavioral-learning/README.md) links each increment to its requirements and acceptance evidence. Use the guides and validation report for current behavior and observed results.
