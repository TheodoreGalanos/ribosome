// Actual Pi execution with a scripted provider and a pipe fault at the host
// effect boundary. Only this fixture pauses stdin or exits deliberately.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
let restoring = false, phase = 0, artifactFile, runId;
peer.call = async (method, params, signal) => {
  if (method !== 'tool.call' || params.method !== 'action.execute' || restoring) return call(method, params, signal);
  // The Pi adapter has already checkpointed the assistant tool call and sent
  // its visible activity. The response to this edit cannot be consumed.
  process.stdin.pause();
  setInterval(() => {
    if (readFileSync(artifactFile, 'utf8') === 'crash edit completed') process.exit(23);
  }, 1);
  void call(method, params, signal).catch(error => {
    console.error('Fault-injection edit dispatch failed:', error.message);
    process.exit(25);
  });
  setTimeout(() => process.exit(24), 5000);
  return new Promise(() => {});
};

const execution = createAgentExecution(peer, async (model, context) => {
  const previous = context.messages.at(-1);
  let name, args, id;
  if (restoring) {
    if (phase === 0) {
      assert.equal(previous.role, 'user');
      assert.match(JSON.stringify(previous.content), /crash-edit/);
      assert.doesNotMatch(JSON.stringify(previous.content), /crash edit completed/);
      name = 'action_lookup'; args = { id: `${runId}/crash-edit` }; id = 'resumed-lookup';
    } else if (phase === 1) {
      assert.equal(previous.role, 'toolResult');
      assert.equal(previous.toolName, 'action_lookup');
      assert.equal(previous.isError, false);
      const receipt = JSON.parse(previous.content[0].text);
      if (existsSync(`${artifactFile}.settlement-test`)) {
        assert.equal(receipt.status, 'unknown');
        assert.equal(receipt.settlement.request.executor_stopped, true);
        assert.equal(receipt.outcome_basis, 'current_postcondition_observed');
        assert.deepEqual(receipt.restored_validity, []);
      } else assert.equal(receipt.status, 'succeeded');
      name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000 }; id = 'resumed-read';
    } else {
      assert.equal(JSON.parse(previous.content[0].text).content, 'subsequent owner edit');
      name = 'finish'; args = { disposition: 'completed', summary: 'Pi rebuilt current context, retrieved the durable receipt and observed the subsequent owner edit without reapplying it.' }; id = 'resumed-finish';
    }
    phase++;
  } else if (phase++ === 0) {
    name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000 }; id = 'initial-read';
  } else {
    const artifact = JSON.parse(previous.content[0].text).artifact;
    name = 'action_execute'; args = { kind: 'edit', path: 'report.txt', expected_version: artifact.version, content: 'crash edit completed' }; id = 'crash-edit';
  }
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content: [{ type: 'toolCall', id, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(), usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream();
  stream.push({ type: 'done', reason: 'toolUse', message });
  return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => {
  restoring = Boolean(request.checkpoint); phase = 0; artifactFile = request.prompt; runId = request.run_id;
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
