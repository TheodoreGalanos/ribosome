import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

test('init resolves Node from PATH and documents all supported commands', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-init-'));
  const binary = resolve('target/debug/ribosome');
  try {
    const result = spawnSync(binary, ['init', directory], {
      encoding: 'utf8', env: { PATH: dirname(process.execPath) },
    });
    assert.equal(result.status, 0, result.stderr);
    const config = JSON.parse(await readFile(join(directory, 'ribosome.json'), 'utf8'));
    assert.equal(config.node, await realpath(process.execPath));
    assert.equal(config.grant.mode, 'observe');
    assert.equal(config.request.model, 'your-model-id');
    const help = spawnSync(binary, ['--help'], { encoding: 'utf8' });
    assert.match(help.stdout, /ribosome records CONFIG.json RECORDS.json/);
    assert.match(help.stdout, /ribosome search CONFIG.json QUERY.json/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('provider check requires an explicit model without making a provider call', () => {
  const check = env => spawnSync(process.execPath, ['scripts/check-provider.mjs'], {
    encoding: 'utf8', env: { OPENAI_API_KEY: 'fixture-key', ...env },
  });
  const missing = check({});
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /RIBOSOME_MODEL must be configured/);
  const configured = check({ RIBOSOME_MODEL: 'configured-model' });
  assert.equal(configured.status, 0, configured.stderr);
  assert.equal(JSON.parse(configured.stdout).model, 'configured-model');
});
