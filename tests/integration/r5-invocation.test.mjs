import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { prepareInvocation } from '../evaluations/r5-invocation.mjs';

test('R5 preparation keeps execution generic and the checker cannot repair the recipient', async () => {
  const parent = await mkdtemp(join(tmpdir(), 'ribosome-r5-'));
  try {
    const directory = join(parent, 'trial');
    const prepared = await prepareInvocation(directory);
    assert.deepEqual(prepared, { donor_episodes: 2, maximum_calls: 36, maximum_cost_microusd: '350000' });
    const config = JSON.parse(await readFile(join(directory, 'config.json'), 'utf8'));
    assert.equal(config.request.operator, 'extraction@1');
    assert.deepEqual(config.grant.writable_paths, ['result.json']);
    assert.deepEqual(Object.keys(config.tools), ['recipient-check']);
    assert.deepEqual(config.tools['recipient-check'].writes ?? [], []);
    const check = () => spawnSync(process.execPath, [new URL('../evaluations/r5-invocation.mjs', import.meta.url).pathname, '--check'], { cwd: directory, encoding: 'utf8', env: {} });
    const pending = await readFile(join(directory, 'result.json'), 'utf8');
    assert.equal(check().status, 1);
    assert.equal(await readFile(join(directory, 'result.json'), 'utf8'), pending);
    await writeFile(join(directory, 'result.json'), JSON.stringify({ status: 'resolved', total: 5, unit: 'm', period: 'current' }));
    assert.equal(check().status, 0);
    await writeFile(join(directory, 'recipient.json'), JSON.stringify({ a: { value: 2, unit: 'm', period: 'current' }, b: { value: 3, unit: 'm' } }));
    assert.equal(check().status, 1);
    await writeFile(join(directory, 'result.json'), JSON.stringify({ status: 'unresolved' }));
    assert.equal(check().status, 0);
    await assert.rejects(prepareInvocation(directory), /EEXIST/);
  } finally { await rm(parent, { recursive: true, force: true }); }
});
