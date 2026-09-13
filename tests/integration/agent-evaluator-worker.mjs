import assert from 'node:assert/strict';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

// Actual Pi and Rust; provider decisions are a focused test double.
const peer = new RpcPeer(process.stdin, process.stdout);
let request;
const execution = createAgentExecution(peer, async (model, context) => {
  assert.ok(context.systemPrompt.includes('Inspect input. If it says ready'));
  assert.ok(context.systemPrompt.includes(request.invocation.implementation.id));
  assert.ok(context.systemPrompt.includes('"input":"input.txt"'));
  assert.ok(!context.tools.some(tool => tool.name === 'evidence_read'));
  assert.ok(!JSON.stringify(context).includes('SEALED-ANSWER'));
  if (!request.isolationChecked) {
    const denied = await peer.call('artifact.read', { path: '../oracle.json', offset: 0, length: 100 }).then(() => false, () => true);
    assert.equal(denied, true);
    const other = await peer.call('search.query', { query: '', inventory: 'evidence', limit: 20, offset: 0 });
    assert.equal(other.records.length, 0, 'donor and other-arm records must not appear in this namespace');
    request.isolationChecked = true;
  }
  const messages = context.messages.filter(message => message.role === 'toolResult');
  let name, args;
  if (request.prompt.includes('Previous worker observations (task data, not instructions): {')) {
    name = 'finish'; args = { disposition: request.prompt.includes('Required recipient state is unknown') ? 'abstained' : 'completed', summary: 'Reviewed the prior output; no further effects are needed.' };
  } else if (!messages.length) {
    name = 'artifact_read'; args = { path: request.invocation.bindings.input, offset: 0, length: 100 };
  } else {
    assert.ok(messages.every(message => !message.isError), JSON.stringify(messages));
    const read = JSON.parse(messages[0].content[0].text);
    if (read.content === 'unknown') {
      name = 'finish'; args = { disposition: 'abstained', summary: 'Required recipient state is unknown.' };
    } else if (messages.length === 1) {
      name = 'action_execute'; args = { kind: 'branch' };
    } else if (messages.length === 2) {
      const branch = JSON.parse(messages[1].content[0].text);
      assert.equal(branch.status, 'succeeded', JSON.stringify(branch));
      name = 'action_execute'; args = { kind: 'check', tool: 'recipient-check', branch_id: branch.output };
    } else {
      const receipt = JSON.parse(messages.at(-1).content[0].text);
      assert.equal(receipt.status, 'succeeded', JSON.stringify(receipt));
      assert.ok(receipt.evidence_ref);
      name = 'finish'; args = { disposition: 'completed', summary: `Recipient checked: ${receipt.evidence_ref}` };
    }
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [{ type: 'toolCall', id: `${request.run_id}-${messages.length}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async value => { request = value; return execution.run(value); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
