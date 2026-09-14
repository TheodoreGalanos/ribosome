import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { prepareLifecycle, runLifecycle, checkRows } from '../../examples/learned-behavior/lifecycle.mjs';
import { fixtureModel } from './model-fixture.mjs';

test('behavior lifecycle revises, evaluates, admits, remembers, retrieves, executes and withdraws', { timeout: 120000 }, async () => {
  const parent = await mkdtemp(join(tmpdir(), 'ribosome-lifecycle-'));
  try {
    const directory = join(parent, 'trial'); await prepareLifecycle(directory);
    const file = join(directory, 'config.json'), config = JSON.parse(await readFile(file, 'utf8'));
    config.request.model = fixtureModel('openai').id;
    config.worker = resolve('tests/integration/lifecycle-worker-fixture.mjs');
    await writeFile(file, JSON.stringify(config));
    const report = await runLifecycle(directory, { environment: {} });
    assert.equal(report.status, 'withdrawn_after_counterexample', JSON.stringify(report));
    assert.equal(report.initial_study.complete, true);
    assert.equal(report.initial_study.decision, 'rejected');
    assert.equal(report.revised_study.decision, 'accepted');
    assert.equal(report.revised_study.evaluation_refs.length, 4);
    assert.equal(report.reuse.passed, true);
    assert.equal(report.reuse.fresh_check, true);
    assert.equal(report.reuse.actual.independent_cost, 250);
    assert.equal(report.challenge.passed, false);
    assert.equal(report.challenge.actual.independent_cost, 250);
    assert.equal(report.memories.length, 1);
    assert.deepEqual(report.memories_after_withdrawal, []);
    assert.equal(report.available_after_withdrawal, false);
    assert.equal(report.usage.unknown_calls, 0);
    assert.equal(report.usage.known_cost_microusd, 0);
    assert.ok(report.stages.filter(stage => stage.model_usage).every(stage => stage.model_usage.complete));
    const latest = JSON.parse(await readFile(file, 'utf8'));
    latest.request = { ...latest.request, run_id: 'after-withdrawal', profile: 'caretaker', operator: 'execute-motif@1', invocation: { implementation: report.revised, bindings: { input: 'input.json', output: 'output.json', checker: 'row-check' }, recipient_refs: [], purpose: 'production' } };
    await writeFile(file, JSON.stringify(latest));
    const denied = spawnSync(resolve('target/debug/ribosome'), ['run', file], { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 15000 });
    assert.notEqual(denied.status, 0);
    assert.match(denied.stderr, /record unavailable|absent or inaccessible/);
    await assert.rejects(runLifecycle(directory, { environment: {} }), /already started/);
  } finally { await rm(parent, { recursive: true, force: true }); }
});

test('row checker reports missing compatibility without editing the output', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-row-check-'));
  try {
    await writeFile(join(directory, 'input.json'), JSON.stringify([{ id: 'a', value: 1, unit: 'm' }, { id: 'a', value: 1, unit: 'cm' }]));
    const pending = { status: 'resolved', rows: [{ id: 'a', value: 1 }], independent_cost: 250 };
    await writeFile(join(directory, 'output.json'), JSON.stringify(pending));
    assert.equal((await checkRows(directory)).passed, false);
    assert.deepEqual(JSON.parse(await readFile(join(directory, 'output.json'), 'utf8')), pending);
    await writeFile(join(directory, 'output.json'), '{unfinished');
    assert.equal((await checkRows(directory)).passed, false, 'malformed agent output is a task failure');
    await writeFile(join(directory, 'output.json'), JSON.stringify({ status: 'unresolved', independent_cost: 250 }));
    assert.equal((await checkRows(directory)).passed, true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
