import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fixtureModel } from './model-fixture.mjs';

test('installed prepared invocation obtains fresh receipts and abstains without donor history', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-installed-invocation-'));
  try {
    const executable = process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome');
    const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: process.env.RIBOSOME_TEST_INVOCATION_WORKER ?? resolve('tests/integration/invocation-worker-fixture.mjs'),
      grant: { id: 'owner', scope: { client: 'test', project: 'installed-invocation' }, mode: 'sandbox', paths: ['input.txt'], tools: ['recipient-check'], profiles: ['caretaker'], budget: { max_calls: 12, max_tokens: '1000000', max_cost_microusd: '100000', max_actions: 4, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 60000) }, context: 'recipient', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'supported', profile: 'caretaker', operator: 'execute-motif@1', prompt: 'Execute the prepared policy.', provider: 'openai', model: fixtureModel('openai').id },
      tools: { 'recipient-check': { program: process.execPath, args: ['-e', "if(require('fs').readFileSync('input.txt','utf8')!=='ready')process.exit(1)"], timeout_ms: 2000, reads: ['input.txt'], validates: ['input.txt'], writes: [] } } };
    const file = join(directory, 'config.json');
    const command = args => { const r = spawnSync(executable, args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 15000 }); assert.equal(r.status, 0, r.stderr); return JSON.parse(r.stdout); };
    await writeFile(file, JSON.stringify(config));
    const material = { name: 'fixture-policy', version: '1', motifs: [], format: 'instructions', material: 'Inspect input. If it says ready, check the recipient. If it says unknown, abstain. Never invent missing inputs.', parameters: {}, required_capabilities: [], state_assumptions: [], possible_effects: [], failure_behavior: 'Abstain when unknown', evaluation_refs: [], instruction_contract: { inputs: [{ name: 'input', kind: 'artifact_path', required: true }], outputs: ['recipient observation'], entry_obligations: ['Read input'], exit_obligations: ['Report observed status'], limitations: ['Mechanical fixture; not live discovery'], discovery_refs: [] } };
    const records = join(directory, 'records.json');
    await writeFile(records, JSON.stringify([{ kind: 'implementation', provenance: { origin: 'synthetic', source_refs: [], scenario_family: 'installed-mechanism', split: 'development', limitations: ['Authored test policy and provider responses.'] }, body: material }]));
    const [saved] = command(['records', file, records]);
    config.request.invocation = { implementation: { id: saved.id, version: '1' }, bindings: { input: 'input.txt' }, recipient_refs: [], purpose: 'experimental' };
    await writeFile(join(directory, 'input.txt'), 'ready'); await writeFile(file, JSON.stringify(config));
    assert.equal(command(['run', file]).disposition, 'completed');
    const supported = command(['inspect', join(config.state_dir, 'ribosome.db'), 'supported']);
    assert.ok(supported.effects.some(e => e.action.kind === 'check' && e.status === 'succeeded' && e.evidence_ref));
    config.request.run_id = 'unsupported'; await writeFile(join(directory, 'input.txt'), 'unknown'); await writeFile(file, JSON.stringify(config));
    assert.equal(command(['run', file]).disposition, 'abstained');
    const unsupported = command(['inspect', join(config.state_dir, 'ribosome.db'), 'unsupported']);
    assert.equal(unsupported.effects.length, 0);
    assert.equal(supported.model_usage.complete && unsupported.model_usage.complete, true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
