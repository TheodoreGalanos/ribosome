import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';

// Transport/concurrency fixture: no planner or model provider is simulated.
const peer = new RpcPeer(process.stdin, process.stdout);
let onCancel = async () => {};
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.cancel', async () => { await onCancel(); return { ok: true }; });
peer.handle('agent.run', async request => {
  const { directory, role, experiment, disconnect, action, reportQueued } = JSON.parse(request.prompt);
  const checkpoint = { format: 'pi-0.85.1/1', profile: request.profile, operator: request.operator, provider: request.provider, model: request.model, messages: [], pending_operations: [], event_cursor: '0' };
  if (role === 'cancellation') {
    return new Promise(resolve => {
      onCancel = async () => {
        await peer.call('session.checkpoint', checkpoint);
        resolve({ disposition: 'cancelled', summary: 'Checkpoint persisted after cancellation.' });
      };
      writeFileSync(join(directory, 'command-started'), 'ready');
    });
  }
  const waitFor = async name => {
    const deadline = Date.now() + 10000;
    while (!existsSync(join(directory, name))) {
      if (Date.now() > deadline) throw new Error(`Timed out waiting for ${name}`);
      await delay(5);
    }
  };
  if (role === 'effect' || role === 'experiment') {
    const pending = peer.call('tool.call', { call_id: `${request.run_id}-observation`, method: role === 'effect' ? 'action.execute' : 'experiment.run', arguments: action ?? { id: experiment } });
    if (reportQueued) {
      await peer.call('session.grant', {});
      writeFileSync(join(directory, 'request-queued'), 'queued');
    }
    if (disconnect) { await waitFor('command-started'); process.exit(23); }
    const result = await pending;
    return { disposition: 'completed', summary: JSON.stringify(result) };
  }
  if (role === 'command') {
    await waitFor('probe-ready');
    const receipt = await peer.call('action.execute', { operation_id: 'slow-command', kind: 'check', tool: 'slow' });
    if (receipt.status !== 'succeeded') throw new Error(`Command failed: ${receipt.output}`);
    return { disposition: 'completed', summary: 'Command settled.' };
  }
  writeFileSync(join(directory, 'probe-ready'), 'ready');
  await waitFor('command-started');
  const timings = await Promise.all([
    ['evidence.read', { cursor: '0', limit: 10 }],
    ['session.checkpoint', checkpoint],
    ['model.permit', { max_output_tokens: 10, input_tokens_bound: '10', cost_microusd_bound: '10' }],
  ].map(async ([method, params]) => {
    const start = performance.now();
    await peer.call(method, params);
    return [method, performance.now() - start];
  }));
  return { disposition: 'completed', summary: JSON.stringify(Object.fromEntries(timings)) };
});
peer.onClose = () => process.stdin.destroy();
