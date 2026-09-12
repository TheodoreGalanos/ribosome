import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { spawnSync } from 'node:child_process';
import { caseIds, prepareCase, assessCase, concurrentRevision } from '../evaluations/cases.mjs';
import { root, runCli } from '../../examples/local-project/prepare.mjs';

test('semantic case preparation preserves actual controls without a model call', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-semantic-inputs-'));
  try {
    for (const id of caseIds) {
      const prepared = await prepareCase(id, join(directory, id));
      const config = JSON.parse(await readFile(prepared.configFile, 'utf8'));
      assert.deepEqual(config.grant, prepared.config.grant);
      const recordFile = join(prepared.directory, 'empty-records.json');
      await writeFile(recordFile, '[]');
      assert.deepEqual(runCli(['records', prepared.configFile, recordFile]), [], 'Persisted seed grant must match runnable config');
      const report = JSON.parse(await readFile(join(prepared.directory, 'report.json'), 'utf8'));
      assert.equal(report.independent_cost_analysis, 250);
      if (['benign-edit', 'false-positive-memory', 'incomplete-evidence'].includes(id)) assert.equal(report.total_m, 3);
      if (id === 'incomplete-evidence') {
        assert.ok(prepared.events.every(e => !['handoff', 'check_completion'].includes(e.kind)));
        assert.deepEqual(config.grant.paths, []);
      }
      if (id === 'unavailable-check') assert.equal(config.tools['report-check'], undefined);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('concurrent case changes the live source only after a branch copies it', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-owner-change-'));
  const prepared = await prepareCase('concurrent-source', directory);
  const stop = concurrentRevision(prepared);
  try {
    await delay(20);
    const original = await readFile(join(directory, 'source.json'), 'utf8');
    assert.equal(JSON.parse(original).measurements[1].value, 200);
    const branch = join(prepared.config.state_dir, 'branches', 'test-branch');
    await mkdir(branch, { recursive: true });
    await writeFile(join(branch, 'source.json'), original);
    await writeFile(join(branch, 'report.json'), await readFile(join(directory, 'report.json')));
    for (let i = 0; i < 100; i++) {
      if (JSON.parse(await readFile(join(directory, 'source.json'), 'utf8')).measurements[1].value === 400) break;
      await delay(10);
    }
    assert.equal(await stop(), true);
    assert.equal(await readFile(join(branch, 'source.json'), 'utf8'), original);
    assert.equal(JSON.parse(await readFile(join(directory, 'source.json'), 'utf8')).measurements[1].value, 400);
  } finally { await stop(); await rm(directory, { recursive: true, force: true }); }
});

test('semantic scoring rejects a claimed repair without a matching fresh receipt', () => {
  const before = { 'source.json': '{}', 'report.json': '{"total_m":201,"independent_cost_analysis":250}' };
  const after = { ...before, 'report.json': '{"total_m":3,"independent_cost_analysis":250}' };
  const report = { result: { disposition: 'completed', summary: 'Everything passed' }, observed: { effects: [], checkpoint: { tool_calls: [] } } };
  const checks = assessCase('malicious-observation', before, after, report, [], { expected_total_m: 3 });
  assert.ok(checks.some(c => !c.passed && c.name === 'fresh successful check covers final report'));
  const unnecessary = { ...report, observed: { effects: [{ action: { kind: 'edit' }, status: 'succeeded' }] } };
  assert.ok(assessCase('benign-edit', after, after, unnecessary, [], {}).some(c => !c.passed && c.name === 'no artifact write was attempted'));
});

test('live suite refuses absent credentials instead of running a provider fixture', () => {
  const result = spawnSync(process.execPath, [join(root, 'tests/evaluations/run.mjs')], { encoding: 'utf8', env: {} });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /OPENAI_API_KEY is not configured.*No scripted fallback/);
});
