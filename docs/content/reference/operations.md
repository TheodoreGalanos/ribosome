---
title: "Operations"
description: "Run, inspect, stop, recover, and back up the local host."
---

# Operations

Your application owns the Rust host's lifetime. Use one host owner per workspace, and keep the Rust executable and TypeScript worker on matching builds.

## Common commands

From a checkout, use `target/debug/ribosome`. After installation, use your installed executable.

| Command | Purpose |
| --- | --- |
| `ribosome run CONFIG.json` | Run maintenance and permitted follow-up work. |
| `ribosome host CONFIG.json` | Serve an application attachment over stdin/stdout. |
| `ribosome study CONFIG.json EXPERIMENT_ID` | Execute a saved host-owned study. |
| `ribosome inspect DATABASE RUN_ID` | Read status, effects, continuation metadata, timing, and usage. |
| `ribosome effect CONFIG.json OPERATION_ID` | Inspect an uncertain effect and current workspace versions. |
| `ribosome settle CONFIG.json SETTLEMENT.json` | Record an owner's inspected decision about an unknown outcome. |
| `ribosome search CONFIG.json QUERY.json` | Query records within the configured grant. |
| `ribosome validate CONTRACT VALUE.json` | Validate a value against a named contract. |

Run `ribosome --help` for all commands. Direct model execution uses provider settings exported in the shell.

## Stop and inspect

Ctrl-C requests cancellation. Already-dispatched work must settle before the host releases its execution ownership.

```sh
target/debug/ribosome inspect /absolute/state/ribosome.db RUN_ID
```

Read `budget`, `model_usage`, and `timings` alongside run status. Unknown usage represents a retained liability. Timing spans describe the measured operations and can overlap.

For attachments, `finish()` flushes observations and waits for maintenance and feedback. `detach()` unsubscribes and requests cancellation of attachment maintenance while the application's agent continues. `client.close()` also closes a host it started.

## Resume an interrupted run

Repeat the interrupted run's unchanged configuration while its grant remains valid. Ribosome restores available source context and reconciles recorded effects. A captured action outcome can finish bookkeeping without repeating the action.

For an unknown outcome, keep competing writers stopped. Establish that the original executor has stopped, then use `ribosome effect` to inspect its receipt and all possibly affected files. Submit the returned versions and your reason with `ribosome settle`. Run fresh authorized checks to establish what is valid now.

The [settlement reference](../../operations.md#settle-an-executor-with-an-unknown-outcome) gives the JSON shape. For cooperative repair, reconcile the original writer handoff after all unknown effects are settled. Start a terminal run's next task with a new work identity.

## Back up and upgrade

For an operational backup, stop the host and external writers, let effects settle, and copy the workspace, full state directory, and non-secret configuration. This includes the SQLite database and its related files, branches, and exports.

The current database schema is 21. Opening an older supported schema creates a consistent database backup before a transactional migration. Keep that backup and use the matching current binary after migration.

Restore into an inspection location first. Check artifact versions and unresolved effects before resuming. Saved branch paths are absolute, so preserve their locations when restoring a run.

## Maintain stored sources

Retirement immediately removes source access. Deletion also queues cleanup of managed copies. Library hosts can inspect progress with `Store::source_cleanup_status` and advance pending jobs with `Store::cleanup_sources` after restart.

See the [operations reference](../../operations.md) for pagination, SQLite backup, search-index rebuilding, and failure recovery details. [Runtime and storage](../guides/runtime.md) shows the directory structure.
