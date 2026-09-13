import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { fixtureModel } from './model-fixture.mjs';

for (const mode of ['complete', 'permit_ack_lost', 'dispatch_ack_lost', 'cancel_before_dispatch', 'provider_crash']) test(`R3-07: Pi accounting ${mode} preserves dispatch identity and liabilities`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-model-metering-'));
  try {
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/model-metering-worker-fixture.mjs'),
      grant: { id: 'metered-grant', scope: { client: 'metered-client', project: 'metered-project' }, mode: 'observe', paths: [], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 1, max_tokens: '1000000', max_cost_microusd: '1000000', max_actions: 1, max_work_items: 1, max_depth: 1, deadline_ms: String(Date.now() + 60000) }, context: 'metered-fixture', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'metered-run', profile: 'caretaker', operator: 'proofreading@1', prompt: JSON.stringify({ mode, directory }), provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json');
    await writeFile(configFile, JSON.stringify(config));
    function command(args) {
      const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 30000 });
      assert.ifError(result.error);
      return result;
    }
    const run = command(['run', configFile]);
    assert.equal(JSON.parse(run.stdout).disposition, mode === 'complete' ? 'completed' : mode === 'cancel_before_dispatch' ? 'cancelled' : mode === 'provider_crash' ? 'interrupted' : 'failed', run.stderr + run.stdout);
    const trace = (await readFile(join(directory, 'transport.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
    assert.equal(trace.filter(event => event.method === 'model.permit').length, 1, 'recovery must not issue another allowance');
    assert.equal(trace.filter(event => event.provider_dispatched).length, ['complete', 'provider_crash'].includes(mode) ? 1 : 0);
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try {
      const permits = db.prepare('SELECT * FROM permits').all();
      assert.equal(permits.length, 1);
      const permit = permits[0];
      const expected = mode === 'complete' ? 'settled' : ['dispatch_ack_lost', 'provider_crash'].includes(mode) ? 'dispatched' : 'released';
      assert.equal(permit.state, expected);
      assert.equal(permit.call_id, trace.find(event => event.method === 'model.permit').params.call_id);
      if (mode === 'permit_ack_lost') {
        assert.equal(trace.find(event => event.method === 'model.permit.lookup').params.id, permit.call_id);
        assert.equal(trace.find(event => event.method === 'model.release').params.id, permit.id);
        assert.ok(!trace.some(event => event.method === 'model.dispatch'));
      }
      if (mode === 'dispatch_ack_lost') {
        assert.equal(JSON.parse(permit.usage).complete, false);
        assert.ok(permit.reserved_tokens > 12);
        assert.equal(trace.filter(event => event.method === 'model.dispatch').length, 1);
      }
      if (mode === 'provider_crash') {
        assert.equal(permit.usage, null, 'a terminated worker cannot report observed usage');
        assert.ok(permit.reserved_tokens > 12);
      }
    } finally { db.close(); }
    if (mode === 'provider_crash') {
      const resumed = command(['run', configFile]);
      assert.equal(JSON.parse(resumed.stdout).disposition, 'exhausted', resumed.stderr + resumed.stdout);
      const after = (await readFile(join(directory, 'transport.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
      assert.equal(after.filter(event => event.provider_dispatched).length, 1, 'restart cannot fund a replacement call from unknown usage');
      const reopened = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
      try {
        const permits = reopened.prepare('SELECT state,call_id,usage FROM permits').all();
        assert.equal(permits.length, 1);
        assert.equal(permits[0].state, 'dispatched');
        assert.equal(permits[0].call_id, trace.find(event => event.method === 'model.permit').params.call_id);
        assert.equal(permits[0].usage, null);
      } finally { reopened.close(); }
    }
    const inspection = command(['inspect', join(directory, 'state/ribosome.db'), config.request.run_id]);
    assert.equal(inspection.status, 0, inspection.stderr);
    const status = JSON.parse(inspection.stdout);
    assert.equal(status.model_usage.released_calls, ['permit_ack_lost', 'cancel_before_dispatch'].includes(mode) ? 1 : 0);
    assert.equal(status.model_usage.unknown_calls, ['dispatch_ack_lost', 'provider_crash'].includes(mode) ? 1 : 0);
    assert.equal(status.budget.remaining.max_calls, ['permit_ack_lost', 'cancel_before_dispatch'].includes(mode) ? 1 : 0);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
