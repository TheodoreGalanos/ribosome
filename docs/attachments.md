# Connect an agent harness

Ribosome can observe an application-owned harness at startup or join it while it is running. The application keeps its agent loop. A connector sends bounded observations to an explicitly started Rust host and receives findings or repair outcomes. Pi is the included adapter; other harnesses use the same event contract.

## Startup integration

Build or install the Rust executable and `@ribosome/agents` as described in the [README](../README.md). Configure the same workspace, state directory, model, grant and registered tools used by `ribosome run`. `ribosome host CONFIG.json` serves `ribosome-host/1` on stdin/stdout. Importing the package starts nothing.

```ts
import { AttachmentClient } from '@ribosome/agents/attachments';
import { attachPi } from '@ribosome/agents/attachments/pi';

const client = await AttachmentClient.start({
  executable: '/absolute/path/to/ribosome',
  config: '/absolute/path/to/host.json',
});
const attachment = await attachPi(client, agent, {
  executionId: task.id,
  onFeedback: async feedback => {
    for (const id of feedback.record_refs) {
      present(await attachment.readRecord(id));
    }
  },
  onError: error => reportAttachmentFailure(error),
});
try {
  await agent.prompt(task.prompt);
  await attachment.finish(); // Flush observed events and await final feedback.
} finally {
  await client.close();
}
```

`agent`, `task`, `present` and `reportAttachmentFailure` belong to the application. The same `attachPi(client, agent, options)` call works while that agent is running. Events before subscription are absent; status records that coverage limit. `start: 'history'` includes previously ingested source history, but does not scrape an agent transcript. A host importing its own history must preserve stable event IDs and producer sequences, label missing history, and deduplicate overlap with its subscription.

Pi capture includes lifecycle and tool boundaries by default. `captureMessages` and `captureToolContent` are explicit options. Provider thinking is excluded from captured messages. An optional `artifacts(event)` callback supplies actual `{path, version}` references, with versions calculated as `sha256:` plus the hexadecimal SHA-256 of the file bytes. `redact(event)` can remove sensitive fields or return `undefined` to omit an observation. Hosts are responsible for redacting credentials and private tool content before ingestion.

Pi awaits subscribers. Artifact capture and event persistence each have a one-second limit; an observation can therefore add up to both waits. Maintenance reasoning does not run in that callback. Failures set `attachment.error`, call `onError`, stop observation and request an interrupted coverage status. The application agent continues. `finish()` reports the error instead of claiming complete capture.

## A different harness

The generic export has no Pi runtime import or Pi types. Supply an event source, or call `attachment.publish(event)` from the application's existing loop:

```ts
const attachment = await client.attach({
  executionId: task.id,
  connector: 'my-harness',
  connectorVersion: '1',
  onFeedback: feedback => present(feedback),
});
await attachment.publish({
  id: 'tool-result-17', producer: 'worker-session-1', sequence: '1',
  kind: 'tool.completed', timestamp_ms: String(Date.now()),
  parents: [], correlation: 'tool-call-17', artifacts: [],
  payload: { tool: 'inspect_report', is_error: false },
});
await attachment.finish();
await attachment.detach();
```

For callback-based harnesses, provide `source: { subscribe: emit => unsubscribe }`. `subscribe` installs a handler that calls `emit(normalizedEvent)` and returns the application's unsubscribe function. An async iterator can call `publish` for each event. Await persistence, retain stable identities when retrying, and use a new producer identity if its sequence restarts. Reusing an event ID with changed content fails the entire batch. Missing sequence ranges enter `status.attachment.coverage`.

The host derives client/project, external execution identity and provenance from the attachment. Event text cannot grant permissions or become a Rust effect receipt. The `ribosome.*` event namespace is reserved. Pi suppresses its own steering messages when capturing messages. Custom connectors must likewise avoid feeding returned advice back as new source activity.

The [canonical contracts](../contracts/schema.json) define `HarnessEvent` and `x-host-methods`. A language-independent exchange starts with these newline-delimited JSON requests:

```json
{"jsonrpc":"2.0","id":"1","method":"host.hello","params":{"protocol":"ribosome-host/1"}}
{"jsonrpc":"2.0","id":"2","method":"attachment.open","params":{"id":"a1","execution_id":"task-17","connector":"custom","connector_version":"1","start":"now","capabilities":["observe"]}}
{"jsonrpc":"2.0","id":"3","method":"attachment.status","params":{"attachment_id":"a1"}}
```

Acknowledge feedback with `attachment.ack`, preserving `feedback_id` and reporting `acknowledged`, `rejected` or `unknown`. These are delivery outcomes, not task outcomes. The worker pipe uses its separate `ribosome/1` allowlist; it cannot administer attachments.

## Feedback and steering

`onFeedback` receives a stable ID, target attachment, maintenance run ID, disposition, record/evidence references, relevant artifact versions and expiry. Fetch full records with `readRecord`. Status includes queued/running work, pending feedback, counts by feedback state and **aggregate grant** model usage. It does not include the external agent's model cost.

Before delivery, Rust checks scope, record visibility, expiry and current artifact versions. Stale advice becomes expired. Ordinary advice can be delivered again after a disconnect, at most three attempts. A handler should be quick and idempotent by feedback ID; its timeout is 30 seconds. Do not call `finish()` inside that handler because `finish()` waits for delivery handlers. Acknowledgement means the callback accepted the advice, not that a human agreed or a repair succeeded.

Steering requires both `attachment.allow_steering: true` in host configuration and `allowSteering: true` for Pi (or an explicit `steer` function for a custom connector). Inside `onFeedback`, call `await attachment.steer(feedback, message)`. Rust records an uncertain control intent before the adapter queues it. Pi processes the message at its own next boundary. Subsequent source observations establish what happened. A crash before acceptance is acknowledged leaves the control outcome unknown; it is not automatically repeated.

## Coordinated repair

Enable `attachment.allow_coordinated_writes: true`, request `coordinatedWrites: true` when attaching, and supply an `apply` grant with `writable_paths`, registered tools and nonempty `required_checks`. Every external writer to the workspace must participate in one `WriteCoordinator`:

```ts
import { WriteCoordinator } from '@ribosome/agents/attachments';
const writer = new WriteCoordinator();
// Use this wrapper inside every external effectful tool.
await writer.run(() => updateArtifact());

// At an application-controlled boundary, after ordinary maintenance settles:
await attachment.finish();
await attachment.repair(writer);
await attachment.finish(); // Read the repair disposition and saved receipts.
```

The coordinator closes admission to new writes and waits for in-flight tools. Rust then binds one repair run to a bounded generation token. The maintenance agent diagnoses, edits a branch and requests application. Existing Rust mandatory checks, input-version checks and effect receipts govern the result. The token is checked again before applying after slow checks. Releasing the writer proves that ownership was settled; `repair()` resolving does **not** prove that the model fixed the defect. Check the feedback, actual receipts and application outcome.

A timeout or disconnect leaves the writer stopped. Inspect `attachment.status().attachment.handoff` and the maintenance run's receipts. After reconnecting the same attachment, call `reconcileRepair(writer, generation)` only with the original coordinator still preventing writes. Unknown effects prevent release. A restarted application must keep its own writers stopped while reconciling; a new JavaScript coordinator cannot reconstruct whether an old process stopped.

One nonterminal attachment per workspace is supported for coordinated repair. Ordinary observation supports up to 16 attachments per grant. An uncooperative external writer can race validation/application. Pi steering or abort is not a workspace pause or an OS sandbox.

## Run the live example

Configure `RIBOSOME_MODEL` and the selected provider credentials in the local environment or root `.env` before running:

```sh
npm run demo:attached
npm run demo:attached -- --mid-run
```

The [driver](../examples/attached-agent/demo.mjs) creates a separate application-owned Pi agent and Rust host. It seeds a report total of 201 for 1 m + 200 cm, returns a finding, checks and applies a repair to 3, preserves independent cost analysis, and continues the external agent. `--mid-run` subscribes while the source's read tool is in flight. The driver uses live provider calls with no scripted fallback. Each invocation allows US$0.40 for the source agent and US$0.60 for Ribosome under conservative Pi catalogue accounting; Azure billing can differ. Output contains actual usage, findings and receipt inspections in `result.json`.

For an installed consumer, copy the driver, set `RIBOSOME_CLI` to its installed executable, and run it there with the selected provider environment. Worker resolution uses the installed package export. See [operation and recovery](operations.md) for restart, expiry, backup and index maintenance. R5 and R6 add prepared retrieval and complete-agent studies. Their automatic orchestration alongside an attached agent is not enabled by this connector; see the [current implementation boundaries](runtime-integration-plan.md) and [R7 qualification](validation.md#r7-integrated-qualification-and-handoff).
