import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { fixtureModel } from './model-fixture.mjs';

function command(args) {
  const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 60000, maxBuffer: 4 * 1048576 });
  assert.ifError(result.error); return result;
}
for (const mode of ['active', 'resume', 'exchange', 'export', 'revision', 'revision-resume']) test(`R1-04/06/07: ${mode} retained artifact is read in bounded chunks and withdrawn`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-tool-results-'));
  try {
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/tool-results-worker-fixture.mjs'),
      grant: { id: 'result-grant', scope: { client: 'result-client', project: 'result-project' }, mode: 'observe', paths: [], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 20, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'result-test', visible_splits: ['development'], allow_export: mode === 'export' },
      request: { run_id: 'result-run', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
    await writeFile(configFile, JSON.stringify(config));
    const memory = () => ({ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'result-fixture', split: 'development', limitations: [] }, body: { kind: 'episodic', content: 'bulkyneedle ' + 'a'.repeat(10000) + 'BEYOND-EXCERPT-MARKER' + 'z'.repeat(48000), applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } });
    const records = Array.from({ length: mode === 'export' || mode.startsWith('revision') ? 1 : 20 }, memory);
    if (mode === 'export') records[0].provenance.origin = 'synthetic';
    if (mode === 'exchange') records[0].body.content = 'a'.repeat(6000) + 'BEYOND-EXCERPT-MARKER' + 'z'.repeat(13000);
    await writeFile(recordsFile, JSON.stringify(records));
    const seeded = command(['records', configFile, recordsFile]); assert.equal(seeded.status, 0, seeded.stderr);
    let memoryId = JSON.parse(seeded.stdout)[0].id;
    const source = memoryId;
    if (mode.startsWith('revision')) {
      const derived = memory(); derived.provenance.source_refs = [source];
      await writeFile(recordsFile, JSON.stringify([derived]));
      const next = command(['records', configFile, recordsFile]); assert.equal(next.status, 0, next.stderr);
      memoryId = JSON.parse(next.stdout)[0].id;
    }
    config.request.prompt = JSON.stringify({ directory, mode, memory: memoryId, source });
    await writeFile(configFile, JSON.stringify(config));
    let result = command(['run', configFile]);
    if (mode === 'resume' || mode === 'revision-resume') {
      assert.equal(result.status, 2, result.stderr + result.stdout);
      assert.equal(JSON.parse(result.stdout).disposition, 'interrupted');
      result = command(['run', configFile]);
    }
    assert.equal(result.status, 0, result.stderr + result.stdout);
    const reference = JSON.parse(await readFile(join(directory, 'reference.json'), 'utf8'));
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try {
      assert.equal(db.prepare("SELECT count(*) AS n FROM artifact_snapshots WHERE result_method=?").get(mode === 'exchange' || mode.startsWith('revision') ? 'record.read' : mode === 'export' ? 'training.export' : 'search.query').n, mode === 'exchange' ? 12 : 1, 'Recovery must reuse the retained observation');
      assert.equal(db.prepare('SELECT result_content FROM artifact_snapshots WHERE path=?').get(reference.artifact.path).result_content, null);
      assert.equal(db.prepare('SELECT count(*) AS n FROM effects').get().n, 0);
      if (mode.startsWith('revision')) {
        const current = JSON.parse(db.prepare('SELECT body FROM records WHERE id=?').get(memoryId).body);
        assert.equal(current.version, '2'); assert.equal(current.retired, false);
        assert.equal(current.body.content, 'Independent current observation');
      }
    } finally { db.close(); }
    if (mode === 'export') {
      const product = JSON.parse(await readFile(join(directory, 'export-file.json'), 'utf8'));
      await assert.rejects(stat(product.path), { code: 'ENOENT' });
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
