import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, stat, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { prepareDiagnostic, checkAnswer } from '../evaluations/r1-summary-diagnostic.mjs';

test('summary diagnostic prepares offline and rejects invented units or ignored acceptance status', async () => {
  const temporary = await mkdtemp(join(tmpdir(), 'ribosome-summary-diagnostic-'));
  try {
    const directory = join(temporary, 'case');
    await prepareDiagnostic(directory);
    await assert.rejects(stat(join(directory, 'state/ribosome.db')), { code: 'ENOENT' });
    await assert.rejects(prepareDiagnostic(directory), { code: 'EEXIST' });
    const input = JSON.parse(await readFile(join(directory, 'facts.json'), 'utf8'));
    const correct = input.assets.map(asset => {
      const accepted = asset.readings.filter(row => row.status === 'accepted');
      const unresolved = accepted.some(row => row.unit !== 'kWh');
      return { asset: asset.asset, status: unresolved ? 'unresolved' : 'ready', accepted_count: accepted.length, total_kwh: unresolved ? null : accepted.reduce((sum, row) => sum + row.value, 0) };
    });
    assert.equal(checkAnswer(correct), true);
    const invented = structuredClone(correct); invented[1].status = 'ready'; invented[1].total_kwh = 20;
    assert.equal(checkAnswer(invented), false);
    const includedArchived = structuredClone(correct); includedArchived[0].total_kwh += 900;
    assert.equal(checkAnswer(includedArchived), false);
    assert.equal(checkAnswer([]), false);
  } finally { await rm(temporary, { recursive: true, force: true }); }
});
