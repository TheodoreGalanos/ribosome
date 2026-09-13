import { appendFileSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

// Real Pi/bridge execution with fixture decisions, not semantic discovery.
const directory = process.env.RIBOSOME_CORPUS_FIXTURE;
const settings = JSON.parse(readFileSync(join(directory, 'worker-settings.json'), 'utf8'));
const trace = join(directory, 'provider-views.jsonl');
const peer = new RpcPeer(process.stdin, process.stdout);
let request;
const execution = createAgentExecution(peer, async (model, context) => {
  appendFileSync(trace, JSON.stringify({ type: 'provider', run_id: request.run_id, context }) + '\n');
  const messages = context.messages.filter(message => message.role === 'toolResult');
  const step = messages.length;
  const result = name => {
    const message = messages.findLast(message => message.toolName === name);
    return message && !message.isError ? JSON.parse(message.content.find(part => part.type === 'text').text) : undefined;
  };
  let name, args;
  if (request.run_id === 'parent') {
    const sequence = [
      ['evidence_corpus', {}],
      ['evidence_read', { cursor: '0', limit: 100 }],
      ['search_query', { query: '', inventory: 'evidence', offset: 0, limit: 100 }],
      ['artifact_read', { path: 'input.txt', snapshot_id: settings.original_snapshot, required_freshness: 'historical', offset: 0, length: 100 }],
      ['artifact_read', { path: 'input.txt', snapshot_id: settings.future_snapshot, required_freshness: 'historical', offset: 0, length: 100 }],
      ['record_read', { id: settings.hidden_definition }],
      ['work_request', { subject: 'bounded-contrast', profile: 'curator', operator: 'contrast-motif@1', reason: 'Inspect the same host-assigned corpus in a fresh context.', evidence_refs: ['past'] }],
    ];
    if (step < sequence.length) [name, args] = sequence[step];
    else if (step === 7) { name = 'work_wait'; args = { work_ids: [result('work_request').id] }; }
    else if (step === 8) { name = 'work_status'; args = { id: result('work_request').id }; }
    else {
      const status = result('work_status');
      if (status?.status !== 'completed' || !status.result_available || status.result.summary !== 'CORPUS-CHILD') throw new Error('Assigned child outcome was not delivered');
      name = 'finish'; args = { disposition: 'completed', summary: 'Observed assigned evidence and the completed contrast child.' };
    }
  } else {
    const sequence = [
      ['evidence_corpus', {}],
      ['evidence_read', { cursor: '0', limit: 100, query: 'contradiction' }],
      ['artifact_read', { path: 'input.txt', snapshot_id: settings.future_snapshot, required_freshness: 'historical', offset: 0, length: 100 }],
      ['finish', { disposition: 'completed', summary: 'CORPUS-CHILD' }],
    ];
    [name, args] = sequence[step] ?? (() => { throw new Error('Unexpected child continuation'); })();
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [{ type: 'toolCall', id: `${request.run_id}-${step}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async value => {
  request = value;
  appendFileSync(trace, JSON.stringify({ type: 'start', run_id: request.run_id, corpus: request.discovery_corpus, resumed: !!request.checkpoint }) + '\n');
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
