import assert from 'node:assert/strict';
import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
let settings, phase = 0, restoring = false, artifactSaved = false, withdrawn = false;
async function withdraw() {
  await call('record.retire', { id: settings.memory, expected_version: '1', delete: true });
  withdrawn = true;
}
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (method === 'session.context.append' && params.entries.at(-1)?.message?.toolName === 'artifact_read') artifactSaved = true;
  if (settings.role === 'receiver' && method === 'session.checkpoint' && artifactSaved && phase === 2 && !restoring && !withdrawn) {
    if (settings.behavior === 'resume') process.exit(23);
    await withdraw();
  }
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  let name, args, text;
  const previous = context.messages.at(-1);
  if (settings.role.startsWith('sender')) {
    if (phase === 0) { name = 'record_read'; args = { id: settings.memory }; }
    else if (phase === 1) {
      const memory = JSON.parse(previous.content[0].text);
      name = 'message_send'; args = { recipient: 'receiver', topic: 'Observed memory', correlation: 'source-message', body: `Sender interpretation of ${memory.body.content}` };
    } else { name = 'finish'; args = { disposition: 'completed', summary: 'SENDER-RESULT-MARKER: interpretation based on the retrieved memory.' }; }
  } else if (phase === 0) { name = 'message_inbox'; args = {}; }
  else if (phase === 1) {
    assert.equal(previous.toolName, 'message_inbox');
    assert.equal(previous.isError, false);
    assert.match(JSON.parse(previous.content[0].text).messages[0].body, /WITHDRAWN-MESSAGE-MARKER/);
    writeFileSync(join(settings.directory, 'before.json'), JSON.stringify(context));
    text = 'RECEIVER-DERIVED-MARKER: an interpretation of the sender message.';
    name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000 };
  } else {
    assert.equal(previous.role, 'user');
    assert.doesNotMatch(JSON.stringify(context.messages), /WITHDRAWN-MESSAGE-MARKER|RECEIVER-DERIVED-MARKER/);
    writeFileSync(join(settings.directory, 'after.json'), JSON.stringify(context));
    name = 'finish'; args = { disposition: 'completed', summary: 'Captured an authorized continuation after sender-source withdrawal.' };
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [...(text ? [{ type: 'text', text }] : []), { type: 'toolCall', id: `${settings.role}-${restoring ? 'resumed' : 'initial'}-${phase++}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => {
  settings = JSON.parse(request.prompt); restoring = Boolean(request.checkpoint);
  if (settings.role === 'retire') { await withdraw(); return { disposition: 'completed', summary: 'Source withdrawn by another workflow.' }; }
  if (settings.role === 'receiver' && !existsSync(join(settings.directory, 'receiver-ready'))) {
    writeFileSync(join(settings.directory, 'receiver-ready'), 'Registered recipient');
    return { disposition: 'interrupted', summary: 'Receiver parked before its provider starts.' };
  }
  if (restoring && settings.role === 'receiver') phase = 2;
  const result = await execution.run(request);
  if (settings.role === 'sender-return-withdraw') await withdraw();
  return result;
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
