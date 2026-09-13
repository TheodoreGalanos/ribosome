import { appendFileSync } from 'node:fs';
import { join } from 'node:path';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { RpcError } from '../../packages/agents/dist/client/validation.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
let settings;
function record(value) {
  appendFileSync(join(settings.directory, 'transport.jsonl'), JSON.stringify(value) + '\n');
}
peer.call = async (method, params, signal) => {
  if (method.startsWith('model.')) record({ method, params });
  const result = await call(method, params, signal);
  // Rust has committed the ledger transition. Inject loss at the return to
  // the production Pi adapter, without invoking a paid provider.
  if ((method === 'model.permit' && settings.mode === 'permit_ack_lost') ||
      (method === 'model.dispatch' && settings.mode === 'dispatch_ack_lost')) {
    throw new RpcError(-32012, 'Injected model accounting acknowledgement loss');
  }
  if (method === 'model.permit' && settings.mode === 'cancel_before_dispatch') execution.cancel();
  return result;
};
const execution = createAgentExecution(peer, async model => {
  record({ provider_dispatched: true });
  // Exit after the production adapter has persisted dispatch, before usage
  // reaches Rust. The restart must retain this call's unknown liability.
  if (settings.mode === 'provider_crash') process.exit(23);
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [{ type: 'toolCall', id: 'metered-finish', name: 'finish', arguments: { disposition: 'completed', summary: 'Metered fixture completed.' } }],
    stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream();
  stream.push({ type: 'done', reason: 'toolUse', message });
  return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt); return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
