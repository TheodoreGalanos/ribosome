# Operate the local attachment host

The consuming application owns `ribosome host CONFIG.json`. There is no listener, background daemon, auto-discovered agent or implicit startup on import. The host acquires the local workspace lock before opening/migrating state. Use one owner per workspace. It processes maintenance serially through the existing Supervisor; a second connection to the same SQLite WAL database handles ingestion and status while a command holds the runtime lock.

## State and limits

`state_dir/ribosome.db` stores grants, events, subscription cursors, work, runs, checkpoints, records, effect receipts, attachment bindings and feedback. `state_dir/branches` and `state_dir/exports` hold filesystem artifacts. SQLite owns Ribosome's durable state; the TypeScript client keeps only pending transport operations, source subscriptions and delivery callbacks.

Schema 1 migrates transactionally to schema 2, preserving existing rows. A failed migration rolls back its new tables/columns; unknown newer schemas are rejected. Existing v0.1 `run` configurations remain valid. Do not open a migrated database with an older binary. Back up before upgrading. The workspace lock is acquired by CLI execution, not by every possible direct `Store::open` caller; library hosts must acquire ownership themselves.

Host defaults are an eight-event or 200 ms batch, a 1,000-event unprocessed backlog, five-minute feedback expiry, 100 producer identities per attachment and 16 attachments per grant. Event batches permit 100 events and 256 KiB; protocol frames permit 1 MiB and transport queues 32 entries. `attachment` configuration can bound `event_kinds`, `batch_size`, `max_delay_ms`, `max_backlog` and `feedback_ttl_ms`. Declared capabilities do not widen the immutable grant. Model calls, tokens, costs, actions, work items and deadlines share that grant across its attachments.

Creating an attachment records source status as `unknown` until source activity establishes it. Status reports source coverage, subscription cursor, observed source status, actionable errors, writer ownership and feedback state counts. Usage reports calls, observed cost and unknown/incomplete calls; it is grant-wide. No external-source usage is inferred. A local file lock does not coordinate arbitrary external tools; use the documented writer handoff for shared repair.

## Finish, stop and reconnect

Call `finish()` after the source execution ends to flush remaining observations and await final feedback. It leaves the attachment connected for another source turn or an explicit repair. `detach()` unsubscribes the connector, cancels pending maintenance and requests cancellation of active maintenance; dispatched effects settle through Supervisor. The source agent is not aborted. `client.close()` also closes an owned host's stdin and waits for shutdown. A normal finished detach records `completed`; an early detach records `detached`. Both are terminal attachment identities.

EOF, Ctrl-C, a failed output pipe or a worker failure stops/cancels work and retains its outcome. On host restart, nonterminal connections are interrupted until reopened with the same attachment ID, execution ID, connector identity, capabilities and original grant. Use persisted producer sequence positions to retry exact event identities. Do not silently declare missing source observations complete. A source host needs its own history to recover events that Ribosome never acknowledged.

Pending ordinary advice survives restart; previously delivered advice can be redelivered with its stable ID. Unknown steering outcomes stay unknown. Completed maintenance whose feedback publication was interrupted is recovered from its saved result. Checkpoints restore the maintenance session, not the external agent or workspace.

An expired grant denies further events, model calls and effects. Status and receipt inspection remain available. A new budget requires explicit owner-issued work and a new grant identity. Do not edit the stored grant or reconnect with a fresh identity to evade an exhausted allowance.

During a writer handoff, timeout/disconnect never releases the external writer automatically. Keep it stopped; inspect and reconcile receipts using the original generation. Unknown outcomes require owner investigation. Aborting an agent is not proof that a tool or descendant process stopped.

## Backup and restore

For a complete operational backup, stop the attachment host **and every external writer**. Let dispatched effects settle. Copy the entire `state_dir`, the workspace files it describes, and the non-secret host configuration into a new backup directory. Include `ribosome.db-wal` and `ribosome.db-shm` if present; do not copy just a live `ribosome.db`. Use an access-controlled backup location with the same privacy requirements as source observations and checkpoints. Provider keys stay in the environment.

SQLite's backup mechanism or `VACUUM INTO` provides a consistent database-only snapshot, including committed WAL content. For example, with a local SQLite CLI:

```sh
sqlite3 /absolute/state/ribosome.db "VACUUM INTO '/absolute/new-backup/ribosome.db';"
```

The destination database must not exist. A database-only snapshot does not capture branch files or freeze the external world. The tests restore this snapshot for inspection and verify scope, retirement and event positions; they do not claim a database copy restores external processes.

Restore into a separate inspection directory first. Verify `PRAGMA integrity_check`, compare receipts and artifact versions with the current workspace, and identify any steering/effects that could have happened after the snapshot. Do not attach a live application or automatically replay control from a restored snapshot. An older backup can also contain records retired or deleted after it was taken; reconcile those withdrawals before using it for retrieval.

For operational resumption, preserve the original configured workspace/state paths: saved branch paths are absolute. Restoring to different paths is an inspection workflow until the owner supplies a supported migration; there is no automatic branch-path rewrite. Resume only after the owner has reconciled external outcomes and ensured every writer remains controlled.

## Rebuild search and handle failed persistence

With all owners stopped, make a backup and rebuild the derived FTS5 index from current visible records:

```sh
sqlite3 /absolute/state/ribosome.db < scripts/rebuild-index.sql
sqlite3 /absolute/state/ribosome.db 'PRAGMA integrity_check;'
```

The [SQL script](../scripts/rebuild-index.sql) rebuilds transactionally, excludes retired and expired records, and keeps authoritative records unchanged. Scope and lineage checks still apply during retrieval. It does not recover damaged authoritative records.

A full disk or failed database write must produce an error, not an ingestion acknowledgement. A batch and its sequence frontier roll back together; routed work and cursor advancement also commit together. Stop the affected attachment, retain any unacknowledged source events in the source host, restore writable capacity, inspect integrity and reconnect explicitly. The fault tests inject rejected writes and migration failure; they do not fill this machine's disk.

Retirement/deletion removes retrieval access and derived support. It does not promise secure erasure of old WAL pages, copied checkpoints, exports, external logs or backups. Apply the owner's retention policy to those copies. Automatic archival, compaction, backup scheduling and a remote service are outside this delivery.
