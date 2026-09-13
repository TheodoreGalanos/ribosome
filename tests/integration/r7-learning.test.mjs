import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
const { prepare, judge } = await import(pathToFileURL(process.env.RIBOSOME_TEST_LEARNING ?? resolve('examples/learned-behavior/demo.mjs')));

test('installed learning preparation keeps oracle separate and scores unsupported input honestly', async () => {
  const parent = await mkdtemp(join(tmpdir(), 'ribosome-learning-'));
  try {
    const folder = join(parent, 'trial');
    assert.equal((await prepare(folder)).planned_evaluations, 4);
    const config = JSON.parse(await readFile(join(folder, 'config.json'), 'utf8'));
    const cases = JSON.parse(await readFile(join(folder, 'recipients.json'), 'utf8'));
    assert.equal(config.corpora[0].definition_refs.length, 0);
    assert.doesNotMatch(config.request.prompt, /duplicate|conflicting values|recipient-supported/);
    const events = JSON.parse(await readFile(join(folder, 'events.json'), 'utf8'));
    assert.ok(events.some(e => e.payload.disposition === 'failed'));
    for (const c of cases) {
      assert.equal(Object.keys(c.input.subject.files).length, 2);
      const input = { task: { case_input: c.input }, branches: [], workspace: parent, executions: [{ effects: [] }] };
      await writeFile(join(parent, 'output.json'), JSON.stringify(c.input.oracle.expected));
      assert.equal((await judge(input)).passed, true);
      if (c.input.oracle.benign) {
        input.executions[0].effects.push({ status: 'succeeded', action: { kind: 'edit' } });
        assert.equal((await judge(input)).passed, false);
      }
      await writeFile(join(parent, 'output.json'), JSON.stringify({ ...c.input.oracle.expected, independent_cost: 0 }));
      assert.equal((await judge(input)).passed, false);
    }
    await assert.rejects(prepare(folder), /EEXIST/);
  } finally { await rm(parent, { recursive: true, force: true }); }
});

test('learning CLI runs when the consumer directory is reached through a symlink', async () => {
  const parent = await mkdtemp(join(tmpdir(), 'ribosome-learning-cli-'));
  try {
    const entry = join(parent, 'demo.mjs');
    await symlink(process.env.RIBOSOME_TEST_LEARNING ?? resolve('examples/learned-behavior/demo.mjs'), entry);
    const result = spawnSync(process.execPath, [entry, '--prepare', join(parent, 'trial')], { encoding: 'utf8', timeout: 10000 });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(JSON.parse(result.stdout).planned_evaluations, 4);
  } finally { await rm(parent, { recursive: true, force: true }); }
});

test('learning CLI persists infrastructure failure without inventing usage', async () => {
  const parent = await mkdtemp(join(tmpdir(), 'ribosome-learning-failure-'));
  try {
    const folder = join(parent, 'trial'); await prepare(folder);
    const entry = process.env.RIBOSOME_TEST_LEARNING ?? resolve('examples/learned-behavior/demo.mjs');
    const result = spawnSync(process.execPath, [entry, '--run', folder], { encoding: 'utf8', timeout: 10000, env: { PATH: process.env.PATH, RIBOSOME_CLI: process.execPath, RIBOSOME_PROVIDER: 'openai', RIBOSOME_MODEL: 'fixture-only', OPENAI_API_KEY: 'test-not-a-credential' } });
    assert.equal(result.status, 1);
    const report = JSON.parse(await readFile(join(folder, 'report.json'), 'utf8'));
    assert.equal(report.status, 'infrastructure_error');
    assert.equal(report.qualification, 'incomplete');
    assert.equal(report.usage, undefined);
    assert.equal(report.attempts[0].exit_code, 1);
  } finally { await rm(parent, { recursive: true, force: true }); }
});
