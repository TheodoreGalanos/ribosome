import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout), call = peer.call.bind(peer);
let settings, phase = 0, receiptSaved = false, withdrawn = false;
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (method === 'session.context.append' && params.entries.at(-1)?.message?.toolName === 'action_execute') receiptSaved = true;
  if (method === 'session.checkpoint' && receiptSaved && !withdrawn) {
    withdrawn = true;
    await call('record.retire', { id: settings.memory, expected_version: '1', delete: true });
  }
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  let name, args;
  if (phase === 0) {
    name = 'record_read'; args = { id: settings.obligation };
  } else if (phase === 1) {
    name = 'record_read'; args = { id: settings.memory };
  } else if (phase === 2) {
    name = 'evidence_read'; args = { cursor: '0', limit: 20, run_id: 'source-run' };
  } else if (phase === 3) {
    assert.match(JSON.stringify(context.messages), /WITHDRAWN-EFFECT-CONTENT/);
    name = 'action_execute'; args = { kind: 'edit', path: 'report.txt', expected_version: `sha256:${createHash('sha256').update('original').digest('hex')}`, content: 'WITHDRAWN-EFFECT-CONTENT' };
  } else {
    assert.ok(withdrawn);
    assert.doesNotMatch(JSON.stringify(context.messages), /WITHDRAWN-EFFECT-CONTENT/);
    assert.match(JSON.stringify(context.messages), new RegExp(settings.obligation));
    if (phase === 4) {
      name = 'continuation_read'; args = { kind: 'obligation', after: '0', limit: 20 };
    } else if (phase === 5) {
      const page = JSON.parse(context.messages.at(-1).content[0].text);
      assert.equal(page.references[0].id, settings.obligation);
      assert.equal(page.evidence_cursor, '1');
      name = 'action_lookup'; args = { id: `${settings.run}/effect-4` };
    } else {
      const receipt = JSON.parse(context.messages.at(-1).content[0].text);
      assert.equal(receipt.status, 'succeeded');
      assert.equal(receipt.content_available, false);
      writeFileSync(join(settings.directory, 'after.json'), JSON.stringify(context));
      name = 'finish'; args = { disposition: 'completed', summary: 'Continued using the recorded outcome after receipt content withdrawal.' };
    }
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content: [{ type: 'toolCall', id: `effect-${++phase}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(), usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt); return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
