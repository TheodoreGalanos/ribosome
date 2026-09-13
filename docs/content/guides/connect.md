---
title: "Connect an agent"
description: "Send execution events and receive findings alongside your agent."
---

# Connect an agent

Attach Ribosome before execution or while an agent is running. Your application keeps its own loop and starts a separate host for maintenance. Begin with observations and feedback, then enable steering or cooperative repair where useful.

## Connect Pi

Install the packages and prepare a [host configuration](../reference/configuration.md). In this example, `agent` and `task` already belong to your application.

```ts
import { AttachmentClient } from '@ribosome/agents/attachments';
import { attachPi } from '@ribosome/agents/attachments/pi';

const client = await AttachmentClient.start({
  executable: '/absolute/path/to/ribosome',
  config: '/absolute/path/to/host.json',
});

try {
  const attachment = await attachPi(client, agent, {
    executionId: task.id,
    onFeedback: feedback => console.log(feedback),
    onError: error => console.error('Observation failed:', error),
  });

  await agent.prompt(task.prompt);
  await attachment.finish();
} finally {
  await client.close();
}
```

Feedback contains record references. Use `attachment.readRecord(id)` to inspect a finding and its evidence. Your application decides how to present or act on it.

## Supply relevant evidence

The Pi connector captures lifecycle and tool boundaries by default. Enable `captureMessages` or `captureToolContent` to include content, and use `redact(event)` to remove sensitive fields before persistence.

Configure the artifact paths and tools maintenance may use. An optional `artifacts(event)` callback supplies observed artifact versions, connecting an event to the files it affected.

Mid-run attachment observes events from subscription onward and records that coverage. Import earlier events explicitly if they matter. `start: 'history'` includes history already supplied to Ribosome.

## Other harnesses

Use `client.attach(...)` with `source.subscribe`, or call `attachment.publish(event)` from your existing loop:

```ts
const attachment = await client.attach({
  executionId: task.id,
  connector: 'my-harness',
  connectorVersion: '1',
  onFeedback: feedback => console.log(feedback),
});

await attachment.publish({
  id: 'tool-result-17', producer: 'worker-session-1', sequence: '1',
  kind: 'tool.completed', timestamp_ms: String(Date.now()),
  parents: [], correlation: 'tool-call-17', artifacts: [],
  payload: { tool: 'inspect_report', is_error: false },
});
await attachment.finish();
```

Use stable event IDs when retrying, increasing sequence numbers per producer, and correlation IDs that connect tool starts and results. Await persistence so the application can detect capture failures.

## Communication in practice

Configure `attachment.event_kinds`, `batch_size`, and `max_delay_ms` to control maintenance batches. Keep feedback handlers short and handle repeat delivery by feedback ID. Fetch the referenced findings, then present advice or queue steering at the source agent's next boundary.

Keep returned advice separate from source activity so it cannot repeatedly trigger itself. Treat observations as task data; permissions come from the host grant. Call `finish()` after the source turn to flush observations and await feedback, then `client.close()` when the host is no longer needed.

See the [connector reference](../../attachments.md) for event contracts, steering, and reconnect handling. Next: [Runtime and storage](runtime.md) or [Repair an artifact](repair.md).
