import { fixtureModel } from './model-fixture.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { providerEnvironment } from '../../examples/local-project/session.mjs';

const root = resolve('.');
const plainModel = fixtureModel('azure-openai-responses');
const reasoningModel = fixtureModel('azure-openai-responses', true);
const environment = {
  AZURE_OPENAI_API_KEY: 'azure-fixture-key',
  AZURE_OPENAI_BASE_URL: 'https://ribosome-test.openai.azure.com',
  AZURE_OPENAI_API_VERSION: 'v1',
  AZURE_OPENAI_DEPLOYMENT_NAME_MAP: `${plainModel.id}=measurement-deployment,${reasoningModel.id}=reasoning-deployment`,
  OPENAI_API_KEY: 'must-not-forward', ANTHROPIC_API_KEY: 'must-not-forward', UNRELATED_SECRET: 'must-not-forward',
};

test('Azure evaluation configuration retains only its provider settings and rejects malformed configuration', () => {
  const selected = providerEnvironment('azure-openai-responses', environment);
  assert.deepEqual(Object.keys(selected).sort(), ['AZURE_OPENAI_API_KEY', 'AZURE_OPENAI_API_VERSION', 'AZURE_OPENAI_BASE_URL', 'AZURE_OPENAI_DEPLOYMENT_NAME_MAP'].sort());
  const resource = { AZURE_OPENAI_API_KEY: 'fixture', AZURE_OPENAI_RESOURCE_NAME: 'ribosome-test' };
  assert.deepEqual(providerEnvironment('azure-openai-responses', resource), resource);
  assert.throws(() => providerEnvironment('azure-openai-responses', {}), /AZURE_OPENAI_API_KEY is not configured/);
  for (const url of ['not-a-url', 'https://user:password@example.com', 'https://example.com/?key=secret', 'https://example.com/openai/deployments/deployment']) {
    assert.throws(() => providerEnvironment('azure-openai-responses', { ...environment, AZURE_OPENAI_BASE_URL: url }), /AZURE_OPENAI_BASE_URL/);
  }
  for (const map of ['broken', 'model=one,model=two', 'model=', 'model=deployment=extra']) {
    assert.throws(() => providerEnvironment('azure-openai-responses', { ...environment, AZURE_OPENAI_DEPLOYMENT_NAME_MAP: map }), /AZURE_OPENAI_DEPLOYMENT_NAME_MAP/);
  }
});

async function configuration(directory) {
  await writeFile(join(directory, 'report.txt'), 'original');
  const config = {
    workspace: directory, state_dir: join(directory, 'state'), node: process.execPath,
    worker: join(root, 'tests/integration/azure-worker-fixture.mjs'),
    grant: {
      id: 'azure-grant', scope: { client: 'test', project: 'azure' }, mode: 'apply',
      paths: ['report.txt'], writable_paths: [], tools: ['credential-check'], profiles: ['caretaker'],
      budget: { max_calls: 4, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 2, max_work_items: 1, max_depth: 1, deadline_ms: String(Date.now() + 30000) },
      context: 'azure', visible_splits: ['development'], allow_export: false,
    },
    request: { run_id: 'azure-run', profile: 'caretaker', operator: 'proofreading@1', prompt: 'Verify the Azure bridge.', provider: 'azure-openai-responses', model: plainModel.id },
    tools: { 'credential-check': {
      program: process.execPath,
      args: ['-e', "const assert=require('node:assert/strict');assert.deepEqual(Object.keys(process.env).filter(k=>/API_KEY|AZURE_OPENAI|UNRELATED_SECRET/.test(k)),[]);console.log('host credential isolation passed')"],
      timeout_ms: 2000, reads: ['report.txt'],
    } },
  };
  const file = join(directory, 'config.json');
  await writeFile(file, JSON.stringify(config));
  return { config, file };
}

for (const model of [plainModel, reasoningModel]) test(`Azure ${model.reasoning ? 'reasoning' : 'non-reasoning'} uses the pinned Pi transport, deployment mapping and scoped credentials across the Rust bridge`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-azure-'));
  try {
    const { file, config } = await configuration(directory);
    config.request.model = model.id;
    if (model.reasoning) config.model_max_output_tokens = 16384;
    await writeFile(file, JSON.stringify(config));
    const result = spawnSync(join(root, 'target/debug/ribosome'), ['run', file], { env: environment, encoding: 'utf8', timeout: 35000 });
    assert.equal(result.status, 0, result.stderr + '\n' + result.stdout);
    assert.equal(JSON.parse(result.stdout).disposition, 'completed');
    assert.equal(await readFile(join(directory, 'report.txt'), 'utf8'), 'original');
    const inspect = spawnSync(join(root, 'target/debug/ribosome'), ['inspect', join(config.state_dir, 'ribosome.db'), 'azure-run'], { encoding: 'utf8' });
    assert.equal(inspect.status, 0, inspect.stderr);
    const stored = JSON.parse(inspect.stdout);
    assert.equal(stored.model_usage.calls, 3);
    assert.equal(stored.model_usage.complete, true);
    assert.ok(BigInt(stored.model_usage.observed_cost_microusd) > 0n);
    assert.equal(stored.effects[0].status, 'succeeded');
    assert.match(stored.effects[0].output, /host credential isolation passed/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('Azure missing endpoint fails before reserving a model call', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-azure-missing-'));
  try {
    const { config, file } = await configuration(directory);
    config.worker = join(root, 'packages/agents/dist/worker.js');
    await writeFile(file, JSON.stringify(config));
    const result = spawnSync(join(root, 'target/debug/ribosome'), ['run', file], { env: { AZURE_OPENAI_API_KEY: 'unused-fixture-key' }, encoding: 'utf8', timeout: 35000 });
    assert.equal(result.status, 2, result.stderr);
    assert.match(JSON.parse(result.stdout).summary, /AZURE_OPENAI_BASE_URL or AZURE_OPENAI_RESOURCE_NAME/);
    const inspect = spawnSync(join(root, 'target/debug/ribosome'), ['inspect', join(config.state_dir, 'ribosome.db'), 'azure-run'], { encoding: 'utf8' });
    assert.equal(inspect.status, 0, inspect.stderr);
    assert.equal(JSON.parse(inspect.stdout).model_usage.calls, 0);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
