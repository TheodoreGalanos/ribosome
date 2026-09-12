import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { Agent } from '@earendil-works/pi-agent-core';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { fixtureModel } from './model-fixture.mjs';
import { Type } from 'typebox';
import { AttachmentClient, WriteCoordinator } from '@ribosome/agents/attachments';
import { attachPi } from '@ribosome/agents/attachments/pi';

const model = fixtureModel('openai');
const hash = text => `sha256:${createHash('sha256').update(text).digest('hex')}`;
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-pi-attachment-'));
  const settings = { workspace: directory, state_dir: join(directory, '.ribosome'), node: process.execPath,
    worker: process.env.RIBOSOME_TEST_WORKER ?? resolve('tests/integration/attachment-worker-fixture.mjs'),
    grant: { id: 'pi-host', scope: { client: 'test', project: 'pi' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
      budget: { max_calls: 30, max_tokens: '1000000', max_cost_microusd: '1000000', max_actions: 10, max_work_items: 10, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'pi', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'template', provider: 'openai', model: model.id, profile: 'caretaker', operator: 'proofreading@1', prompt: 'Inspect the external report' }, tools: {},
    attachment: { allow_steering: true, event_kinds: ['execution.completed'] } };
  await writeFile(join(directory, 'report.txt'), 'original');
  const config = join(directory, 'host.json'); await writeFile(config, JSON.stringify(settings));
  const client = await AttachmentClient.start({ executable: process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome'), config });
  return { directory, client, async close() { try { await client.close(); } finally { await rm(directory, { recursive: true, force: true }); } } };
}
function sourceAgent(onTool = async () => {}) {
  let step = 0;
  const seenContexts = [];
  const agent = new Agent({ initialState: { model, tools: [{ name: 'inspect_report', description: 'Inspect report', parameters: Type.Object({}), execute: async () => { await onTool(); return { content: [{ type: 'text', text: 'Report requires inspection' }], details: {} }; } }] }, streamFn: async (model, context) => {
    seenContexts.push(context);
    const tool = step++ === 0;
    const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content: tool ? [{ type: 'toolCall', id: 'inspect-1', name: 'inspect_report', arguments: {} }] : [{ type: 'thinking', thinking: 'private-reasoning-marker' }, { type: 'text', text: 'Source execution finished.' }], stopReason: tool ? 'toolUse' : 'stop', timestamp: Date.now(), usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
    const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: message.stopReason, message }); return stream;
  } });
  return { agent, seenContexts };
}
for (const midRun of [false, true]) test(`application-owned Pi agent supports ${midRun ? 'mid-run' : 'startup'} attachment and queued steering`, async () => {
  const f = await fixture();
  let release; let entered;
  const started = new Promise(resolve => { entered = resolve; });
  const gate = new Promise(resolve => { release = resolve; });
  const { agent, seenContexts } = sourceAgent(async () => { entered(); if (midRun) await gate; });
  const observed = [], feedback = [];
  let attachment;
  const attach = async () => { attachment = await attachPi(f.client, agent, { executionId: 'application-task', allowSteering: true, captureMessages: true,
    artifacts: () => [{ path: 'report.txt', version: hash('original') }], redact: event => { observed.push(event); return event; },
    onFeedback: async value => { feedback.push(value); await attachment.steer(value, 'Review the saved finding.'); },
  }); };
  try {
    let run;
    if (midRun) { run = agent.prompt('Inspect the report'); await started; await attach(); release(); }
    else { await attach(); run = agent.prompt('Inspect the report'); }
    await run; await attachment.finish();
    assert.equal(agent.state.errorMessage, undefined);
    assert.ok(feedback.length > 0);
    assert.equal(observed.some(e => e.kind === 'execution.started'), !midRun);
    assert.ok(observed.some(e => e.kind === 'tool.completed'));
    assert.ok(!JSON.stringify(observed).includes('private-reasoning-marker'));
    assert.ok(agent.hasQueuedMessages(), 'acknowledged steering means queued, not already consumed');
    const record = await attachment.readRecord(feedback[0].record_refs[0]); assert.equal(record.kind, 'finding');
    await agent.prompt('Continue');
    assert.ok(seenContexts.some(c => JSON.stringify(c.messages).includes('Review the saved finding.')));
    assert.ok(!observed.some(e => e.payload.role === 'user' && JSON.stringify(e.payload).includes('Review the saved finding.')), 'steering echoes are not source input');
    await attachment.finish();
    await attachment.detach();
    const count = observed.length; await agent.prompt('Continue after detach'); assert.equal(observed.length, count);
    assert.equal(await readFile(join(f.directory, 'report.txt'), 'utf8'), 'original');
  } finally { release(); agent.abort(); await f.close(); }
});

test('Pi observation failure is surfaced while the application-owned loop finishes', async () => {
  const f = await fixture(); const { agent } = sourceAgent(); let reported, observations = 0;
  try {
    const attachment = await attachPi(f.client, agent, { executionId: 'broken-observer', onFeedback: () => {}, onError: e => { reported = e; }, artifacts: async () => { observations++; throw new Error('source artifact unavailable'); } });
    await agent.prompt('Inspect the report');
    assert.equal(agent.state.errorMessage, undefined);
    assert.match(reported.message, /source artifact unavailable/);
    assert.equal(observations, 1, 'failed observation unsubscribes instead of delaying every later source event');
    await assert.rejects(attachment.finish(), /source artifact unavailable/);
  } finally { await f.close(); }
});

test('writer timeout never starts a late handoff and requires explicit reconciliation', async () => {
  const coordinator = new WriteCoordinator(); let release, invoked = false;
  const active = coordinator.run(() => new Promise(resolve => { release = resolve; }));
  await assert.rejects(coordinator.handoff(async () => { invoked = true; }, 20), /timed out/);
  await assert.rejects(coordinator.run(async () => 'unsafe'), /timed out/);
  release(); await active; await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(invoked, false);
  await assert.rejects(coordinator.reconcile(async () => { throw new Error('outcome unknown'); }), /unknown/);
  await coordinator.reconcile(async () => {});
  assert.equal(await coordinator.run(async () => 'resumed'), 'resumed');
});
