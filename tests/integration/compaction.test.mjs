import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { fixtureModel } from './model-fixture.mjs';

function command(args) {
  const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 60000 });
  assert.ifError(result.error); return result;
}
for (const mode of ['effect_interruption', 'normal', 'large_owner', 'before_commit', 'after_commit', 'withdraw_in_flight', 'withdraw_before_permit', 'withdraw_after_second', 'provider_error', 'cancel', 'budget', 'compaction_budget']) test(`R1-05/07/10: Pi compaction ${mode} retains lineage, metering and continuation integrity`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-compaction-'));
  try {
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/compaction-worker-fixture.mjs'),
      grant: { id: 'summary-grant', scope: { client: 'summary-client', project: 'summary-project' }, mode: mode === 'effect_interruption' ? 'apply' : 'observe', paths: mode === 'effect_interruption' ? ['report.txt'] : [], tools: [], profiles: ['caretaker'],
        budget: { max_calls: mode === 'budget' ? 5 : mode === 'compaction_budget' ? 4 : 30, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'summary-test', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'summary-run', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    if (mode === 'effect_interruption') await writeFile(join(directory, 'report.txt'), 'original');
    const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
    await writeFile(configFile, JSON.stringify(config));
    const memory = content => ({ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'summary-fixture', split: 'development', limitations: [] }, body: { kind: 'episodic', content, applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } });
    await writeFile(recordsFile, JSON.stringify([memory('KEEP-REVIEW-OBLIGATION'), memory('Independent bulky observation. '.repeat(2000))]));
    const seeded = command(['records', configFile, recordsFile]); assert.equal(seeded.status, 0, seeded.stderr);
    const records = JSON.parse(seeded.stdout);
    config.request.prompt = JSON.stringify({ directory, mode, memory: records[0].id, bulk: records[1].id, ...(mode === 'large_owner' ? { owner_notes: 'Owner instruction. '.repeat(3000) } : {}) });
    await writeFile(configFile, JSON.stringify(config));
    let result = command(['run', configFile]);
    if (mode.endsWith('_commit')) {
      assert.equal(result.status, 2, result.stderr + result.stdout);
      assert.equal(JSON.parse(result.stdout).disposition, 'interrupted');
      result = command(['run', configFile]);
    }
    if (mode === 'effect_interruption') {
      for (let restart = 0; restart < 2; restart++) {
        assert.equal(result.status, 2, result.stderr + result.stdout);
        assert.equal(JSON.parse(result.stdout).disposition, 'interrupted');
        result = command(['run', configFile]);
      }
      assert.equal(result.status, 0, result.stderr + result.stdout);
      assert.equal(await readFile(join(directory, 'report.txt'), 'utf8'), 'compaction and effect continuation completed');
    }
    const calls = (await readFile(join(directory, 'calls.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
    const summaries = calls.filter(c => c.compactor), main = calls.filter(c => !c.compactor);
    if (mode !== 'compaction_budget') assert.ok(summaries.length > 0);
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try {
      if (mode === 'effect_interruption') {
        const effects = db.prepare('SELECT phase,body FROM effects').all();
        assert.equal(effects.length, 1);
        assert.equal(effects[0].phase, 'finalized');
        const receipt = JSON.parse(effects[0].body);
        assert.equal(receipt.status, 'succeeded');
        assert.equal(receipt.outcome_basis, 'execution_established');
        assert.match(JSON.stringify(main.at(-1).context.messages), new RegExp(receipt.operation_id));
        assert.match(JSON.stringify(main.at(-1).context.messages), /execution_established/);
      }
      const permits = db.prepare('SELECT * FROM permits').all();
      assert.equal(permits.length, calls.length);
      assert.equal(permits.filter(p => p.compaction_id).length, summaries.length);
      const allocations = new Map(db.prepare('SELECT id, body FROM budget_allocations').all().map(row => [row.id, JSON.parse(row.body)]));
      assert.equal(new Set(permits.map(p => p.call_id)).size, permits.length);
      for (const permit of permits) {
        assert.ok(permit.call_id && permit.request);
        assert.ok(['dispatched', 'settled'].includes(permit.state));
        const allocation = allocations.get(permit.allocation_id);
        assert.equal(allocation.purpose, permit.compaction_id ? 'compaction' : 'caretaker');
        let root = allocation;
        while (root.parent_id) root = allocations.get(root.parent_id);
        assert.equal(root.purpose, 'root');
        assert.equal(root.grant_id, config.grant.id);
      }
      if (mode === 'cancel') {
        assert.equal(JSON.parse(result.stdout).disposition, 'cancelled', result.stderr + result.stdout);
        assert.equal(JSON.parse(permits.find(p => p.compaction_id).usage).complete, false);
        assert.equal(calls.at(-1).compactor, true);
        assert.equal(db.prepare("SELECT count(*) AS n FROM context_summaries WHERE status='committed'").get().n, 0);
      } else if (mode === 'provider_error') {
        assert.equal(JSON.parse(result.stdout).disposition, 'failed', result.stderr + result.stdout);
        assert.match(JSON.parse(result.stdout).summary, /Injected compaction provider failure/);
        assert.equal(JSON.parse(permits.find(p => p.compaction_id).usage).complete, false);
        assert.ok(permits.find(p => p.compaction_id).reserved_tokens > 20);
        assert.equal(calls.at(-1).compactor, true); // No unsafe fallback provider call.
        assert.equal(db.prepare("SELECT count(*) AS n FROM context_summaries WHERE status='committed'").get().n, 0);
      } else if (mode === 'budget' || mode === 'compaction_budget') {
        assert.equal(JSON.parse(result.stdout).disposition, 'exhausted', result.stderr + result.stdout);
        assert.equal(permits.length, mode === 'budget' ? 5 : 4);
        if (mode === 'compaction_budget') {
          assert.equal(summaries.length, 0);
          assert.equal(db.prepare("SELECT count(*) AS n FROM context_summaries WHERE status='prepared'").get().n, 1);
        }
      } else {
        assert.equal(result.status, 0, result.stderr + result.stdout);
        assert.equal(JSON.parse(result.stdout).disposition, 'completed');
        assert.ok(Buffer.byteLength(JSON.stringify(main.at(-1).context.messages)) < 180000);
        if (mode.startsWith('withdraw_')) {
          assert.ok(calls.some(c => c.withdrawn));
          if (mode === 'withdraw_before_permit') assert.ok(summaries.every(c => c.withdrawn));
          if (mode === 'withdraw_after_second') assert.equal(summaries.filter(c => !c.withdrawn).length, 2);
          for (const call of calls.filter(c => c.withdrawn)) assert.doesNotMatch(JSON.stringify(call.context), /KEEP-REVIEW-OBLIGATION/);
          assert.doesNotMatch(JSON.stringify(main.at(-1).context), /KEEP-REVIEW-OBLIGATION/);
        } else {
          assert.match(JSON.stringify(main.at(-1).context.messages), /KEEP-REVIEW-OBLIGATION/);
          const rawToolResults = main.at(-1).context.messages.filter(m => m.role === 'toolResult');
          assert.doesNotMatch(JSON.stringify(rawToolResults), /KEEP-REVIEW-OBLIGATION/); // Only the summary preserves the early obligation.
          assert.ok(summaries.some(c => JSON.stringify(c.context.messages).includes('previous_summary')));
          if (mode.endsWith('_commit')) {
            const crashed = await readFile(join(directory, 'fault'), 'utf8');
            assert.equal(summaries.filter(c => c.plan === crashed).length, mode === 'before_commit' ? 2 : 1);
          }
        }
      }
    } finally { db.close(); }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
