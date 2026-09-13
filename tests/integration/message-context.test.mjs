import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { fixtureModel } from './model-fixture.mjs';

function command(args) {
  const result = spawnSync(resolve('target/debug/ribosome'), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 60000 });
  assert.ifError(result.error); return result;
}
for (const behavior of ['active', 'resume', 'return']) test(`R1-01/02/04: ${behavior} message and run-result copies inherit their sender context`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-message-context-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'Independent current report');
    const config = {
      workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/message-context-worker-fixture.mjs'),
      grant: { id: 'message-grant', scope: { client: 'message-client', project: 'message-project' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
        budget: { max_calls: 30, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 5, max_work_items: 5, max_depth: 2, deadline_ms: String(Date.now() + 60000) }, context: 'message-test', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'receiver', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id }, tools: {},
    };
    const file = join(directory, 'config.json'), records = join(directory, 'records.json');
    await writeFile(file, JSON.stringify(config));
    await writeFile(records, JSON.stringify([{ kind: 'memory', provenance: { origin: 'observed', source_refs: [], scenario_family: 'message-fixture', split: 'development', limitations: [] }, body: { kind: 'episodic', content: 'WITHDRAWN-MESSAGE-MARKER', applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }]));
    const seed = command(['records', file, records]); assert.equal(seed.status, 0, seed.stderr);
    const memory = JSON.parse(seed.stdout)[0].id;
    const run = async (role, id = role) => {
      config.request.run_id = id; config.request.prompt = JSON.stringify({ directory, memory, role, behavior });
      await writeFile(file, JSON.stringify(config));
      return command(['run', file]);
    };
    const parked = await run('receiver'); assert.equal(JSON.parse(parked.stdout).disposition, 'interrupted');
    const sender = await run(behavior === 'return' ? 'sender-return-withdraw' : 'sender', 'sender');
    assert.equal(sender.status, 0, sender.stderr + sender.stdout);
    if (behavior === 'return') {
      assert.doesNotMatch(sender.stdout, /SENDER-RESULT-MARKER/);
      assert.match(sender.stdout, /withheld/);
    } else {
      let receiver = await run('receiver');
      if (behavior === 'resume') {
        assert.equal(JSON.parse(receiver.stdout).disposition, 'interrupted', receiver.stderr + receiver.stdout);
        const retired = await run('retire'); assert.equal(retired.status, 0, retired.stderr + retired.stdout);
        receiver = await run('receiver');
      }
      assert.equal(receiver.status, 0, receiver.stderr + receiver.stdout);
      assert.match(await readFile(join(directory, 'before.json'), 'utf8'), /WITHDRAWN-MESSAGE-MARKER/);
      assert.doesNotMatch(await readFile(join(directory, 'after.json'), 'utf8'), /WITHDRAWN-MESSAGE-MARKER|RECEIVER-DERIVED-MARKER/);
    }
    const inspected = command(['inspect', join(directory, 'state/ribosome.db'), 'sender']);
    assert.equal(inspected.status, 0, inspected.stderr);
    assert.equal(JSON.parse(inspected.stdout).result, null);
    assert.equal(JSON.parse(inspected.stdout).result_available, false);
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try { assert.equal(db.prepare('SELECT body FROM messages').get().body, '{}'); } finally { db.close(); }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
