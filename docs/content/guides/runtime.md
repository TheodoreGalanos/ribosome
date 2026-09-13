---
title: "Runtime and storage"
description: "Start maintenance, share a budget, and resume work from local state."
---

# Runtime and storage

Your application starts Ribosome when it needs maintenance. The Rust host runs Pi workers, schedules follow-up work, and saves progress locally.

## Choose how work starts

| Trigger | How it works |
| --- | --- |
| An application request | Start a run with a profile, operator, prompt, and grant. Use `ribosome run CONFIG.json` or the Rust `Supervisor`. |
| Attached execution events | A configured subscription batches matching events and queues maintenance. The attachment client handles polling and feedback. |
| An agent's request | An active maintenance agent can ask for permitted follow-up work, such as a curator investigation. |
| A host-owned experiment | Use `ribosome study CONFIG.json EXPERIMENT_ID` to execute a prepared study directly. |

The host chooses when work becomes eligible. Once running, the maintenance agent reasons about what to read, investigate, or try. For a custom Rust integration, register a `Subscription` and call `Store::poll_subscription` from your event loop. Its event kinds, batch size, and maximum delay control when it queues work.

Discovery, extraction, evaluation, and later reuse are steps your application can connect. Start a separate run for each step that needs different evidence or permissions.

## Run beside another agent

An [attachment](connect.md) sends observations to a separate Rust host while the source agent continues its own task. Event capture and persistence happen at the connector boundary; maintenance reasoning runs outside that callback.

Configure the events worth investigating and return findings through `onFeedback`. Deliver steering at an agent boundary. For shared-file repair, use the [writer handoff](repair.md) so maintenance can check and apply changes while other writes are paused.

## Where work is saved

`workspace` names the task directory. `state_dir` names Ribosome's state directory; the initializer places it at `workspace/.ribosome`.

```text
workspace/
  task files
  .ribosome/              ← default state_dir
    ribosome.db           ← SQLite records and runtime state
    branches/             ← working copies for maintenance
    exports/              ← managed exported artifacts
```

The database holds events, findings, memory, implementations, experiments, run status, continuation state, queued work, effect receipts, and usage. The TypeScript client keeps the active connection and callbacks. Reopening the database restores the saved work available to an authorized run.

## Share resources between runs

A grant sets the permitted tools, files, profiles, deadline, and budget. Parent runs, children, context compaction, and metered evaluations draw from a shared allowance. Each child or case can also have its own ceiling.

A parent waiting for child work saves its continuation and releases its worker slot. The child can run even with one worker slot available. Evidence reads, status, and accounting use short state operations while commands execute separately.

Attachment usage reports Ribosome's grant usage. To include the source agent in the same measured budget, its host transport must participate in the accounting interface. See [shared resource accounting](../../protocol.md#shared-resource-accounting).

## Continue after interruption

Repeat an interrupted run with its unchanged configuration and a valid grant. Ribosome restores its continuation, checks that its sources remain available, and reconciles recorded effects before proceeding.

An observed action result can complete its pending bookkeeping without executing the action again. An action with an unknown outcome needs owner inspection and settlement. [Operations](../reference/operations.md) gives the commands and recovery steps.
