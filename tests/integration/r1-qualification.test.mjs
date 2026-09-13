import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, stat, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { prepareContinuation, runContinuation } from '../evaluations/r1-continuation.mjs';
import { DatabaseSync } from 'node:sqlite';
import { expectedDelivery, checkDelivery } from '../evaluations/r1-continuation-check.mjs';

test('R1 live qualification preparation is offline and its checker rejects wrong delivery', async () => {
  const temporary = await mkdtemp(join(tmpdir(), 'ribosome-r1-qualification-'));
  try {
    const directory = join(temporary, 'case');
    await prepareContinuation(directory);
    const plan = JSON.parse(await readFile(join(directory, 'qualification.json'), 'utf8'));
    assert.equal(plan.status, 'prepared');
    assert.equal(plan.provider_calls, 0);
    await assert.rejects(stat(join(directory, 'state/ribosome.db')), { code: 'ENOENT' });
    await assert.rejects(prepareContinuation(directory), { code: 'EEXIST' });
    const workspace = join(directory, 'workspace');
    const config = JSON.parse(await readFile(join(directory, 'config.json'), 'utf8'));
    assert.equal(config.grant.budget.max_cost_microusd, '2000000');
    const bytes = await Promise.all(config.grant.paths.filter(path => path.startsWith('inspection-')).map(path => stat(join(workspace, path)).then(file => file.size)));
    assert.ok(bytes.reduce((sum, count) => sum + count, 0) > 180000);
    const correct = expectedDelivery(workspace);
    assert.throws(() => checkDelivery(workspace));
    await writeFile(join(workspace, 'delivery.json'), JSON.stringify(correct));
    assert.deepEqual(checkDelivery(workspace), { assets: 18, unresolved: 1 });
    const wrong = structuredClone(correct);
    wrong[0].total_kwh++;
    await writeFile(join(workspace, 'delivery.json'), JSON.stringify(wrong));
    assert.throws(() => checkDelivery(workspace));
    const fabricated = structuredClone(correct);
    fabricated.find(row => row.status === 'unresolved').total_kwh = 0;
    await writeFile(join(workspace, 'delivery.json'), JSON.stringify(fabricated));
    assert.throws(() => checkDelivery(workspace));
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

test('R1 qualification retains incomplete reporting without provider calls when evidence collection fails', async () => {
  const temporary = await mkdtemp(join(tmpdir(), 'ribosome-r1-report-failure-'));
  const names = ['RIBOSOME_PROVIDER', 'RIBOSOME_MODEL', 'OPENAI_API_KEY'];
  const previous = Object.fromEntries(names.map(name => [name, process.env[name]]));
  try {
    const directory = join(temporary, 'case');
    await prepareContinuation(directory);
    // This model is absent from the pinned catalogue. The real worker rejects
    // it before permitting or dispatching any provider request.
    process.env.RIBOSOME_PROVIDER = 'openai';
    process.env.RIBOSOME_MODEL = 'missing-qualification-test-model';
    process.env.OPENAI_API_KEY = 'offline-test-value';
    await writeFile(join(directory, 'provider-boundaries.jsonl'), 'invalid trace containing PRIVATE-TEST-DETAIL\n');
    let report;
    await assert.doesNotReject(async () => { report = await runContinuation(directory); });
    assert.equal(report.status, 'incomplete');
    assert.equal(report.passed, false);
    assert.equal(report.attempts[0].disposition, 'failed');
    assert.doesNotMatch(JSON.stringify(report), /PRIVATE-TEST-DETAIL/);
    assert.equal(JSON.parse(await readFile(join(directory, 'qualification.json'), 'utf8')).status, 'incomplete');
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try { assert.equal(db.prepare('SELECT count(*) AS n FROM permits').get().n, 0); }
    finally { db.close(); }
    await assert.rejects(runContinuation(directory), /already started/);
    // Exercise the inspected connection-retry path without contacting a model.
    // The deliberately missing catalogue entry still stops the real worker.
    const configFile = join(directory, 'config.json');
    const before = JSON.parse(await readFile(configFile, 'utf8'));
    const state = new DatabaseSync(join(directory, 'state/ribosome.db'));
    try {
      const allocation = state.prepare('SELECT allocation_id FROM run_allocations WHERE run_id=?').get(before.request.run_id).allocation_id;
      state.prepare("INSERT INTO permits(id,grant_id,run_id,reserved_tokens,reserved_cost,allocation_id,state) VALUES ('unknown-test-call',?,?,100,20000,?,'dispatched')").run(before.grant.id, before.request.run_id, allocation);
    } finally { state.close(); }
    await writeFile(join(directory, 'provider-boundaries.jsonl'), '');
    await writeFile(join(directory, 'qualification.json'), JSON.stringify({ status: 'executed', attempts: [{ code: 2, disposition: 'failed' }] }));
    await assert.rejects(runContinuation(directory, { retryRejectedRequest: true }), /not a connection or tool-schema rejection/);
    await writeFile(join(directory, 'attempt-1-diagnostics.json'), JSON.stringify({ code: 2, stdout_tail: JSON.stringify({ disposition: 'failed', summary: 'Connection error.' }), stderr_tail: '' }));
    const retried = await runContinuation(directory, { retryRejectedRequest: true });
    const after = JSON.parse(await readFile(configFile, 'utf8'));
    assert.deepEqual(after.grant, before.grant);
    assert.equal(after.state_dir, before.state_dir);
    assert.equal(after.request.run_id, `${before.request.run_id}-retry`);
    assert.equal(retried.prior_attempts.length, 1);
    assert.equal(retried.attempts[0].disposition, 'failed');
    assert.equal(retried.usage.calls, 1);
    assert.equal(retried.usage.incomplete, 1);
    assert.equal(retried.usage.unknown_reserved_cost_microusd, 20000);
    assert.equal(retried.conditions.run_usage_complete, true);
    assert.equal(JSON.parse(await readFile(join(directory, 'rejected-1-report.json'), 'utf8')).attempts.length, 1);
    await assert.rejects(runContinuation(directory, { retryRejectedRequest: true }), /not a connection or tool-schema rejection/);
  } finally {
    for (const name of names) {
      if (previous[name] === undefined) delete process.env[name]; else process.env[name] = previous[name];
    }
    await rm(temporary, { recursive: true, force: true });
  }
});
