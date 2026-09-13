import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fixtureModel } from './model-fixture.mjs';
import { DatabaseSync } from 'node:sqlite';
import { createTools } from '../../packages/agents/dist/tools/index.js';
import { boundedContext } from '../../packages/agents/dist/pi/context.js';

test('context pressure preserves recent bounded reads instead of replacing every result', () => {
  const results = Array.from({ length: 10 }, (_, index) => ({
    role: 'toolResult', toolName: 'artifact_read', toolCallId: `read-${index}`, timestamp: 1,
    content: [{ type: 'text', text: `CHUNK-${index}:` + 'x'.repeat(800) }],
    details: { artifact: { path: `ribosome-result:read-${index}`, version: '1' }, total_bytes: '810', sources: [] },
  }));
  const messages = [{ role: 'user', content: 'Inspect the source chunks.', timestamp: 1 },
    { role: 'assistant', content: results.map(result => ({ type: 'toolCall', id: result.toolCallId, name: result.toolName, arguments: {} })), timestamp: 1 }, ...results];
  const visible = boundedContext(messages, 6000);
  assert.ok(Buffer.byteLength(JSON.stringify(visible)) <= 6000);
  assert.equal(visible.filter(message => message.role === 'toolResult').length, 10);
  assert.match(JSON.stringify(visible), /retained_result/);
  assert.match(JSON.stringify(visible.at(-1)), /CHUNK-9/);
  assert.match(JSON.stringify(messages[2]), /CHUNK-0/);
});

function command(args, timeout = 30000) {
  const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout });
  assert.ifError(result.error);
  return result;
}
for (const mode of ['resume', 'active']) for (const withdrawal of ['retirement', 'delete', 'expiry', 'access']) test(`R1-01/02: ${mode} context excludes ${withdrawal} memory and its interpretation at the provider boundary`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-context-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'current independent observation');
    const expires = String(Date.now() + 4000);
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/context-worker-fixture.mjs'),
      grant: { id: 'context-grant', scope: { client: 'context-client', project: 'context-project' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 20, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'context-test', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'context-run', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
    await writeFile(configFile, JSON.stringify(config));
    await writeFile(recordsFile, JSON.stringify([{ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'context-fixture', split: 'development', limitations: [] }, body: { kind: 'episodic', ...(withdrawal === 'expiry' ? { expires_ms: expires } : {}), content: 'WITHDRAWN-CONTEXT-MARKER', applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }]));
    const seeded = command(['records', configFile, recordsFile]); assert.equal(seeded.status, 0, seeded.stderr);
    const memory = JSON.parse(seeded.stdout)[0].id;
    config.request.prompt = JSON.stringify({ directory, memory, mode, withdrawal, expires });
    await writeFile(configFile, JSON.stringify(config));
    const first = command(['run', configFile]);
    assert.equal(first.status, mode === 'resume' ? 2 : 0, first.stderr + first.stdout);
    if (mode === 'resume') {
      assert.equal(JSON.parse(first.stdout).disposition, 'interrupted', first.stdout);
      const retirement = structuredClone(config);
      retirement.request.run_id = 'retirement-run';
      retirement.request.prompt = JSON.stringify({ directory, memory, mode: 'retire', withdrawal, expires });
      const retirementFile = join(directory, 'retirement.json'); await writeFile(retirementFile, JSON.stringify(retirement));
      const retired = command(['run', retirementFile]); assert.equal(retired.status, 0, retired.stderr + retired.stdout);
      const resumed = command(['run', configFile]); assert.equal(resumed.status, 0, resumed.stderr + resumed.stdout);
    }
    assert.match(await readFile(join(directory, 'before.json'), 'utf8'), /WITHDRAWN-CONTEXT-MARKER/);
    const after = await readFile(join(directory, 'after.json'), 'utf8') + await readFile(join(directory, 'after-again.json'), 'utf8');
    assert.doesNotMatch(after, /WITHDRAWN-CONTEXT-MARKER|DERIVED-CONTEXT-MARKER/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('R1-06: more than ten bridge frames of retained visible tool results resume through a bounded descriptor and protocol tail', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-long-context-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'Fresh observation after the long run.');
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/context-worker-fixture.mjs'),
      grant: { id: 'long-grant', scope: { client: 'long-client', project: 'long-project' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 500, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 120000) }, context: 'long-context', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'long-run', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
    await writeFile(configFile, JSON.stringify(config));
    await writeFile(recordsFile, JSON.stringify([{ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'long-context-fixture', split: 'development', limitations: [] }, body: { kind: 'episodic', content: 'Retained observation. '.repeat(2500), applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }]));
    const seeded = command(['records', configFile, recordsFile]); assert.equal(seeded.status, 0, seeded.stderr);
    config.request.prompt = JSON.stringify({ directory, memory: JSON.parse(seeded.stdout)[0].id, mode: 'long', reads: 210 });
    await writeFile(configFile, JSON.stringify(config));
    const first = command(['run', configFile], 120000); assert.equal(first.status, 2, first.stderr + first.stdout);
    assert.equal(JSON.parse(first.stdout).disposition, 'interrupted', first.stdout);
    const metrics = JSON.parse(await readFile(join(directory, 'metrics.json'), 'utf8'));
    assert.ok(metrics.retainedBytes > 10 * 1048576);
    assert.ok(metrics.maxFrameBytes < 1048576);
    assert.ok(metrics.maxCheckpointBytes < 256 * 1024);
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try {
      const retained = db.prepare("SELECT sum(length(CAST(result_content AS BLOB))) AS bytes FROM artifact_snapshots WHERE result_run_id='long-run'").get();
      assert.ok(retained.bytes > 10 * 1048576);
      const descriptor = JSON.parse(db.prepare('SELECT body FROM checkpoints').get().body);
      assert.equal(descriptor.format, 'pi-0.85.1/2'); assert.deepEqual(descriptor.messages, []);
      assert.ok(Number(descriptor.context.tail_after) > 100);
    } finally { db.close(); }
    const resumed = command(['run', configFile], 120000); assert.equal(resumed.status, 0, resumed.stderr + resumed.stdout);
    const context = JSON.parse(await readFile(join(directory, 'after.json'), 'utf8'));
    assert.ok(Buffer.byteLength(JSON.stringify(context.messages)) < 180000);
    assert.match(JSON.stringify(context.messages), /Retained observation/);
    assert.match(await readFile(join(directory, 'after-again.json'), 'utf8'), /Fresh observation after the long run/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('R1-10: model tools do not expose context append, authorization, or policy generation', () => {
  const tools = createTools({}, 'run', 'proofreading@1', ['finding'], () => {});
  assert.ok(!tools.some(tool => /context|session/.test(tool.name)));
});
