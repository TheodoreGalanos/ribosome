import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fixtureModel } from './model-fixture.mjs';

test('R1: actual Pi continuation preserves independent obligations and withholds deleted effect content', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-effect-context-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'original');
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/effect-context-worker-fixture.mjs'),
      grant: { id: 'effect-grant', scope: { client: 'effect-client', project: 'effect-project' }, mode: 'apply', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 8, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'effect-context', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'effect-run', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
    await writeFile(configFile, JSON.stringify(config));
    await writeFile(recordsFile, JSON.stringify([{ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'effect-context', split: 'development', limitations: [] }, body: { kind: 'episodic', content: 'WITHDRAWN-EFFECT-CONTENT', applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }]));
    const command = args => {
      const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 30000 });
      assert.ifError(result.error); assert.equal(result.status, 0, result.stderr + result.stdout); return result.stdout;
    };
    const memory = JSON.parse(command(['records', configFile, recordsFile]))[0].id;
    await writeFile(recordsFile, JSON.stringify([{ kind: 'obligation', provenance: { origin: 'observed', source_refs: [], scenario_family: 'effect-context', split: 'development', limitations: [] }, body: { description: 'Verify the independent output', owner: 'owner', subject: 'report', state: 'open', affected_outputs: [], created_ms: '1', consequence_boundary: 'Before delivery', evidence_refs: [] } }]));
    const obligation = JSON.parse(command(['records', configFile, recordsFile]))[0].id;
    const eventsFile = join(directory, 'events.json');
    await writeFile(eventsFile, JSON.stringify([{ id: 'independent-source', scope: config.grant.scope, run_id: 'source-run', producer: 'source-worker', sequence: '1', kind: 'observation', timestamp_ms: '1', parents: [], correlation: 'source', artifacts: [], payload: { note: 'Independent source observation' }, provenance: { origin: 'observed', source_refs: [], scenario_family: 'effect-context', split: 'development', limitations: [] } }]));
    command(['ingest', join(directory, 'state/ribosome.db'), eventsFile]);
    config.request.prompt = JSON.stringify({ directory, memory, obligation, run: config.request.run_id });
    await writeFile(configFile, JSON.stringify(config));
    command(['run', configFile]);
    assert.doesNotMatch(await readFile(join(directory, 'after.json'), 'utf8'), /WITHDRAWN-EFFECT-CONTENT/);
    assert.equal(await readFile(join(directory, 'report.txt'), 'utf8'), 'WITHDRAWN-EFFECT-CONTENT');
  } finally { await rm(directory, { recursive: true, force: true }); }
});
