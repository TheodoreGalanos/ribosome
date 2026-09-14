import { appendFileSync, existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const directory = process.env.RIBOSOME_PARK_FIXTURE;
const trace = join(directory, 'workers.jsonl');
const peer = new RpcPeer(process.stdin, process.stdout);
let request;
function record(value) { appendFileSync(trace, JSON.stringify(value) + '\n'); }
const execution = createAgentExecution(peer, async (model, context) => {
  record({ type: 'provider', run_id: request.run_id, pid: process.pid });
  const result = name => {
    const message = context.messages.findLast(message => message.role === 'toolResult' && message.toolName === name);
    if (!message) return undefined;
    const text = message.content.find(part => part.type === 'text').text;
    if (message.isError) throw new Error(`${name}: ${text}`);
    return JSON.parse(text);
  };
  let name, args;
  const parent = request.run_id === 'parent';
  if (parent || (process.env.RIBOSOME_PARK_MODE === 'nested' && request.profile === 'curator')) {
    const work = result('work_request'), wait = result('work_wait'), status = result('work_status');
    // Memory work permits the general nested scheduling exercised here.
    // Discovery follow-ups are restricted to a single contrast investigation.
    if (!work) { name = 'work_request'; args = { subject: parent ? 'child-check' : 'grandchild-check', profile: parent ? 'curator' : 'caretaker', operator: parent ? 'memory@1' : 'proofreading@1', reason: 'Execute child-check and report its actual receipt.', evidence_refs: [] }; }
    else if (!wait) { name = 'work_wait'; args = { work_ids: [work.id] }; }
    else if (!status) { name = 'work_status'; args = { id: work.id }; }
    else {
      if (status.status !== 'completed' || !status.result_available || status.result.summary !== 'CHILD-RESULT') throw new Error(`Parent resumed without the child outcome: ${JSON.stringify(status)}`);
      name = 'finish'; args = { disposition: 'completed', summary: parent ? 'Parent inspected the completed child result after parking.' : 'CHILD-RESULT' };
    }
  } else if (!result('action_execute')) {
    name = 'action_execute'; args = { kind: 'check', tool: 'child-check' };
  } else {
    if (result('action_execute').status !== 'succeeded') throw new Error(`Child check did not succeed: ${JSON.stringify(result('action_execute'))}`);
    name = 'finish'; args = { disposition: 'completed', summary: 'CHILD-RESULT' };
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [{ type: 'toolCall', id: randomUUID(), name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async value => {
  request = value;
  const starts = existsSync(trace) ? readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse).filter(event => event.type === 'start') : [];
  const previous = starts.at(-1)?.pid;
  let previousAlive = false;
  if (previous) { try { process.kill(previous, 0); previousAlive = true; } catch (error) { if (error.code !== 'ESRCH') throw error; } }
  record({ type: 'start', run_id: request.run_id, pid: process.pid, checkpoint: !!request.checkpoint, previous_alive: previousAlive });
  if (previousAlive) throw new Error('Parked or completed worker still occupies a process');
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
