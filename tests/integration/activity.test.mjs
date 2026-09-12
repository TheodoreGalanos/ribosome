import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { activityMessage, publishActivity } from '../../packages/agents/dist/pi/activity.js';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';

test('Pi activity includes visible content without exposing provider thinking', () => {
  const value = activityMessage({ role: 'assistant', timestamp: 123, content: [{ type: 'thinking', thinking: 'private provider reasoning' }, { type: 'text', text: 'Visible conclusion', textSignature: 'private transport signature' }, { type: 'toolCall', id: 'one', name: 'artifact_read', arguments: { path: 'report.txt' } }] }, 2);
  assert.equal(value.sequence, '3');
  assert.equal(value.timestamp_ms, '123');
  assert.equal(value.content.parts.length, 2);
  assert.ok(!JSON.stringify(value).includes('private'));
  assert.equal(value.content.parts[1].name, 'artifact_read');
});

test('Pi activity waits for acknowledgement and publishes bounded batches', async () => {
  const a = new PassThrough(), b = new PassThrough();
  const host = new RpcPeer(a, b), worker = new RpcPeer(b, a);
  const batches = [];
  host.handle('session.events', async batch => { batches.push(batch); return { ok: true }; });
  try {
    const messages = Array.from({ length: 41 }, (_, index) => ({ role: 'user', content: 'message ' + index, timestamp: index + 1 }));
    await publishActivity(worker, messages, 0);
    assert.deepEqual(batches.map(batch => batch.entries.length), [20, 20, 1]);
    assert.equal(batches[2].entries[0].sequence, '41');
    await assert.rejects(publishActivity(worker, [{ role: 'user', content: 'x'.repeat(250 * 1024), timestamp: 1 }], 0), /exceeds evidence capacity/);
    assert.equal(batches.length, 3);
  } finally { host.close(); worker.close(); a.destroy(); b.destroy(); }
});

test('activity retains retrieved source lineage when publishing after a checkpoint', async () => {
  const a = new PassThrough(), b = new PassThrough();
  const host = new RpcPeer(a, b), worker = new RpcPeer(b, a);
  const batches = [];
  host.handle('session.events', async batch => { batches.push(batch); return { ok: true }; });
  try {
    const messages = [
      { role: 'toolResult', toolName: 'record_read', toolCallId: 'one', isError: false, timestamp: 1, content: [{ type: 'text', text: JSON.stringify({ id: 'memory-source', body: { content: 'A memory' } }) }] },
      { role: 'assistant', timestamp: 2, content: [{ type: 'text', text: 'A conclusion using the memory' }] },
      { role: 'toolResult', toolName: 'evidence_read', toolCallId: 'two', isError: false, timestamp: 3, content: [{ type: 'text', text: JSON.stringify({ events: [{ id: 'event-source' }] }) }] },
    ];
    await publishActivity(worker, messages, 1);
    assert.deepEqual(batches[0].entries[0].source_refs, ['memory-source']);
    assert.deepEqual(batches[0].entries[1].source_refs, ['event-source', 'memory-source']);
    assert.equal(batches[0].entries[0].sequence, '2');
  } finally { host.close(); worker.close(); a.destroy(); b.destroy(); }
});
