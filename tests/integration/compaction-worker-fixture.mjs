import { appendFileSync, existsSync, writeFileSync } from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
const instance = randomUUID();
let settings, plan, counter = 0, summariesCommitted = 0, withdrawn = false;
async function withdraw() {
  await call('record.retire', { id: settings.memory, expected_version: '1', delete: true });
  withdrawn = true;
}
function state(query) {
  const db = new DatabaseSync(join(settings.directory, 'state/ribosome.db'), { readOnly: true });
  try { return db.prepare(query).get(); } finally { db.close(); }
}
function crash(point) {
  const marker = join(settings.directory, 'fault');
  if (settings.mode === point && !existsSync(marker)) { writeFileSync(marker, plan.id); process.exit(23); }
}
peer.call = async (method, params, signal) => {
  if (method === 'session.compaction.commit') crash('before_commit');
  const result = await call(method, params, signal);
  if (settings.mode === 'effect_interruption') {
    const point = method === 'session.compaction.commit' ? 'compaction' : method === 'tool.call' && params.method === 'action.execute' ? 'effect' : undefined;
    if (point && !existsSync(join(settings.directory, `${point}-fault`))) {
      writeFileSync(join(settings.directory, `${point}-fault`), 'response lost after host persistence');
      process.exit(23);
    }
  }
  if (method === 'session.compaction.read') {
    plan = result.plan;
    if (settings.mode === 'withdraw_before_permit' && !withdrawn) await withdraw();
  }
  if (method === 'session.compaction.commit') {
    crash('after_commit');
    if (settings.mode === 'withdraw_after_second' && ++summariesCommitted === 2) await withdraw();
  }
  return result;
};
const execution = createAgentExecution(peer, async (model, context, options) => {
  const compactor = context.tools?.some(tool => tool.name === 'commit_summary');
  const phase = state("SELECT count(*) AS n FROM context_items WHERE json_extract(body,'$.toolName')='record_read'").n;
  appendFileSync(join(settings.directory, 'calls.jsonl'), JSON.stringify({ compactor, phase, plan: compactor ? plan.id : undefined, withdrawn, context }) + '\n');
  let name, args;
  if (compactor) {
    if (context.tools.length !== 1) throw new Error('Compactor was given task tools');
    if (settings.mode === 'provider_error') throw new Error('Injected compaction provider failure');
    const hasObligation = JSON.stringify(context.messages).includes('KEEP-REVIEW-OBLIGATION');
    name = 'commit_summary';
    args = { text: hasObligation ? 'KEEP-REVIEW-OBLIGATION remains unresolved. Interpret the observation and verify current receipts.' : 'Continue the owner task using the authorized observations.' };
    if (settings.mode === 'withdraw_in_flight' && !withdrawn) {
      await withdraw();
    }
  } else if (phase < 8) {
    name = 'record_read'; args = { id: phase === 0 ? settings.memory : settings.bulk };
  } else if (settings.mode === 'effect_interruption' && state('SELECT count(*) AS n FROM effects').n === 0) {
    name = 'action_execute'; args = { kind: 'edit', path: 'report.txt', expected_version: `sha256:${createHash('sha256').update('original').digest('hex')}`, content: 'compaction and effect continuation completed' };
  } else {
    name = 'finish'; args = { disposition: 'completed', summary: 'Actual provider continuation captured.' };
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [...(!compactor && phase > 0 && phase < 8 ? [{ type: 'text', text: 'Interpreting the returned observation. '.repeat(1600) }] : []), { type: 'toolCall', id: `compaction-fixture-${instance}-${++counter}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream();
  if (compactor && settings.mode === 'cancel') {
    execution.cancel();
    if (!options.signal.aborted) throw new Error('Parent cancellation failed to abort compactor');
    message.stopReason = 'aborted'; message.content = [];
    stream.push({ type: 'error', reason: 'aborted', error: message }); return stream;
  }
  stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt); return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
