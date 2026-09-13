import { writeFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout), call = peer.call.bind(peer);
let settings, phase = 0, saved = false, changed = false;
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (method === 'session.context.append' && params.entries.at(-1)?.message?.toolName === 'artifact_read') saved = true;
  if (method === 'session.checkpoint' && saved && !changed) {
    changed = true;
    if (settings.change === 'delete') unlinkSync(join(settings.directory, 'report.txt'));
    else writeFileSync(join(settings.directory, 'report.txt'), 'CURRENT-ARTIFACT-CONTENT');
  }
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  let name, args;
  if (phase === 0) {
    name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000, ...(settings.freshness ? { required_freshness: settings.freshness } : {}) };
  } else {
    writeFileSync(join(settings.directory, 'after.json'), JSON.stringify(context));
    name = 'finish'; args = { disposition: 'completed', summary: 'Captured artifact authorization at the provider boundary.' };
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content: [{ type: 'toolCall', id: `artifact-${++phase}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(), usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt); return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
