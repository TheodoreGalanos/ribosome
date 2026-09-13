import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fixtureModel } from './model-fixture.mjs';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';

for (const change of ['revise', 'delete']) for (const freshness of [undefined, 'historical']) test(`R1-03: ${freshness ?? 'current'} context after artifact ${change}`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-artifact-context-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'PREVIOUS-ARTIFACT-CONTENT');
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/artifact-context-worker-fixture.mjs'),
      grant: { id: 'artifact-grant', scope: { client: 'artifact-client', project: 'artifact-project' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 5, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'artifact-context', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'artifact-run', profile: 'caretaker', operator: 'proofreading@1', prompt: JSON.stringify({ directory, change, freshness }), provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json'); await writeFile(configFile, JSON.stringify(config));
    const result = spawnSync(resolve('target/debug/ribosome'), ['run', configFile], { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 30000 });
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr + result.stdout);
    const after = await readFile(join(directory, 'after.json'), 'utf8');
    if (change === 'revise' && freshness === 'historical') {
      assert.match(after, /PREVIOUS-ARTIFACT-CONTENT/);
      const observation = JSON.parse(JSON.parse(after).messages.at(-1).content[0].text);
      assert.equal(observation.required_freshness, 'historical');
    }
    else assert.doesNotMatch(after, /PREVIOUS-ARTIFACT-CONTENT/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('artifact-aware worker requires the matching host source capability before binding', { timeout: 10000 }, async () => {
  const child = spawn(process.execPath, [resolve('packages/agents/dist/worker.js')], { env: { PATH: process.env.PATH } });
  const exited = once(child, 'exit'); child.stderr.resume();
  const peer = new RpcPeer(child.stdout, child.stdin);
  const hello = { protocol: 'ribosome/1', build: '0.1.0', pi: '0.85.1', session: 'source-capability-test', capabilities: ['agent.run', 'context.v2'] };
  try {
    await assert.rejects(peer.call('bridge.hello', hello), /host lacks context.sources\/4/);
    await assert.rejects(peer.call('bridge.hello', { ...hello, capabilities: [...hello.capabilities, 'context.sources/4'] }), /host lacks context.compaction\/1/);
    await assert.rejects(peer.call('bridge.hello', { ...hello, capabilities: [...hello.capabilities, 'context.sources/4', 'context.compaction/1'] }), /host lacks context.results\/1/);
    await assert.rejects(peer.call('bridge.hello', { ...hello, capabilities: [...hello.capabilities, 'context.sources/4', 'context.compaction/1', 'context.results/1'] }), /host lacks budget.allocations\/1/);
    await assert.rejects(peer.call('bridge.hello', { ...hello, capabilities: [...hello.capabilities, 'context.sources/4', 'context.compaction/1', 'context.results/1', 'budget.allocations/1'] }), /host lacks context.parking\/1/);
    const accepted = await peer.call('bridge.hello', { ...hello, capabilities: [...hello.capabilities, 'context.sources/4', 'context.compaction/1', 'context.results/1', 'budget.allocations/1', 'context.parking/1'] });
    assert.ok(accepted.capabilities.includes('context.sources/4'));
  } finally { peer.close(); child.stdin.end(); await exited; }
});
