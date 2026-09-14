import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { fixtureModel } from './model-fixture.mjs';
import { supportedDiscovery, studyShape } from '../../examples/offline-lab/studies.mjs';
import { renderTranscript } from '../../examples/offline-lab/lab.mjs';
import { judge as pathJudge, recipientCases } from '../../examples/offline-lab/path-study.mjs';

const json = value => JSON.stringify(value);

test('offline study command reaches plan validation', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-offline-cli-'));
  try {
    const plan = join(directory, 'plan.json');
    await writeFile(plan, json({ objective: 'invalid' }));
    const result = spawnSync(process.execPath, ['examples/offline-lab/lab.mjs', 'study', directory, plan], { encoding: 'utf8', timeout: 10000 });
    assert.ifError(result.error);
    assert.match(result.stderr, /Study objective must be function or system_benefit/);
    assert.equal(result.status, 1);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('offline study setup preserves case counts and requires a supported investigation', () => {
  const cases = ['applicable', 'incompatible'].map((id, index) => ({ id, family: `family-${index}`, input: { subject: {}, oracle: {} } }));
  assert.deepEqual(studyShape({ objective: 'function', cases }), { arms: 2, repetitions: 2, planned: 8, families: ['family-0', 'family-1'] });
  assert.equal(studyShape({ objective: 'system_benefit', cases }).planned, 20);
  assert.throws(() => studyShape({ objective: 'system_benefit', cases: cases.map(c => ({ ...c, family: 'same' })) }), /two recipient families/);
  assert.throws(() => supportedDiscovery([{ kind: 'definition', id: 'candidate', body: {} }], 'corpus'), /No supported investigation/);
  assert.throws(() => supportedDiscovery([{ kind: 'discovery', body: { corpus: { id: 'corpus' }, decision: 'no_motif', definition_refs: [], occurrence_refs: [] } }], 'corpus'), /No supported investigation/);
});

test('path study judge detects wrong answers and changes to an already correct result', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-path-judge-'));
  try {
    const [applicable, incompatible] = recipientCases();
    const input = { workspace: directory, branches: [], executions: [], task: { case_input: applicable.input } };
    const output = structuredClone(applicable.input.oracle.expected);
    await writeFile(join(directory, 'output.json'), json(output));
    assert.equal((await pathJudge(input)).passed, true);
    output.resolutions['/field/repository/other.c'] = 'other.c';
    await writeFile(join(directory, 'output.json'), json(output));
    assert.equal((await pathJudge(input)).passed, false);
    input.task.case_input = incompatible.input;
    await writeFile(join(directory, 'output.json'), json(incompatible.input.oracle.expected));
    assert.equal((await pathJudge(input)).passed, true);
    input.executions = [{ effects: [{ status: 'succeeded', action: { kind: 'edit' } }] }];
    assert.equal((await pathJudge(input)).passed, false);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('transcript control uses the same message window and excludes owner annotations', () => {
  const episode = { annotations: { reward: 'HIDDEN-OUTCOME' }, raw: { final: 'HIDDEN-SIDECAR' }, decoded: { messages: [{ role: 'user', content: 'visible task' }, { role: 'assistant', content: 'HIDDEN-SUFFIX' }] } };
  const transcript = renderTranscript(episode, { start: 0, end: 1 }, 'source');
  assert.match(transcript, /visible task/);
  assert.match(transcript, /event:source:0/);
  assert.doesNotMatch(transcript, /HIDDEN/);
});
function command(program, args, expected = 0) {
  const result = spawnSync(resolve('target/debug', program), args, { env: { PATH: process.env.PATH }, encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error);
  assert.equal(result.status, expected, result.stderr + result.stdout);
  return JSON.parse(result.stdout);
}

// Set this to an acquired cohort directory to repeat the same provider-boundary
// check with real external material. The transport is scripted; no model calls.
test('offline evidence supports saved memory, interrupted continuation and source withdrawal', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-offline-'));
  try {
    let cohort = process.env.RIBOSOME_OFFLINE_COHORT;
    if (!cohort) {
      cohort = join(directory, 'cohort');
      command('ribosome-import', ['prepare', resolve('examples/offline-lab/profiles/local-chat.yaml'), cohort]);
    }
    const episode = JSON.parse(await readFile(join(cohort, 'episodes/episode-0000.json'), 'utf8'));
    await mkdir(join(directory, 'workspace'));
    await writeFile(join(directory, 'workspace/report.txt'), 'Independent current observation');
    const config = {
      workspace: join(directory, 'workspace'), state_dir: join(directory, 'state'), node: process.execPath,
      worker: resolve('tests/integration/context-worker-fixture.mjs'), tools: {},
      grant: { id: 'offline-owner', scope: { client: 'test', project: 'offline' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker', 'curator'],
        budget: { max_calls: 20, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 1, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 60000) },
        context: 'offline', visible_splits: ['development'], allow_export: false },
      request: { run_id: 'continuation', profile: 'caretaker', operator: 'proofreading@1', prompt: '', provider: 'openai', model: fixtureModel('openai').id },
    };
    const configFile = join(directory, 'config.json'), studyFile = join(directory, 'study.json');
    const manifest = { version: '1', id: 'offline', cohorts: { sample: resolve(cohort) }, episodes: [{ cohort: 'sample', episode: episode.id, split: 'development' }],
      assignments: [{ id: 'prefix', visibility: 'online', windows: [{ cohort: 'sample', episode: episode.id, start: 0, end: 1 }], required_capabilities: ['text'] }] };
    await writeFile(configFile, json(config)); await writeFile(studyFile, json(manifest));
    const assigned = command('ribosome-import', ['assign', studyFile, configFile, join(directory, 'assignments')]);
    const owner = JSON.parse(await readFile(join(directory, 'assignments/owner-config.json'), 'utf8'));
    const source = assigned[0].source_records[0], event = assigned[0].corpus.source_windows[0].event_refs[0];
    const material = episode.decoded.messages.map(m => typeof m.content === 'string' ? m.content : '').join('\n').slice(0, 30000);
    const submissions = [{ kind: 'memory', provenance: { origin: 'observed', source_refs: [event], scenario_family: episode.family, split: 'development', limitations: ['Mechanical continuation test; the observation is copied by the owner.'] },
      body: { kind: 'episodic', content: 'IMPORTED-MEMORY-MARKER\n' + material, applicability: 'Later source review', evidence_refs: [event], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }];
    const recordFile = join(directory, 'records.json'); await writeFile(recordFile, json(submissions));
    await writeFile(configFile, json(owner));
    const [memory] = command('ribosome', ['records', configFile, recordFile]);
    owner.request.prompt = json({ directory, memory: memory.id, mode: 'resume' });
    await writeFile(configFile, json(owner));
    assert.equal(command('ribosome', ['run', configFile], 2).disposition, 'interrupted');
    assert.match(await readFile(join(directory, 'before.json'), 'utf8'), /IMPORTED-MEMORY-MARKER/);
    assert.equal(command('ribosome-import', ['withdraw', studyFile, configFile, 'sample', episode.id]).withdrawn_source, source);
    assert.equal(command('ribosome', ['run', configFile]).disposition, 'completed');
    const after = await readFile(join(directory, 'after.json'), 'utf8') + await readFile(join(directory, 'after-again.json'), 'utf8');
    assert.doesNotMatch(after, /IMPORTED-MEMORY-MARKER|DERIVED-CONTEXT-MARKER/);
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try { assert.equal(JSON.parse(db.prepare('SELECT body FROM records WHERE id=?').get(source).body).retired, true); }
    finally { db.close(); }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('CLI run_budget stops a run at its smaller allowance', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-run-budget-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'fixture');
    const budget = { max_calls: 20, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 0, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 60000) };
    const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: resolve('tests/integration/context-worker-fixture.mjs'), tools: {},
      grant: { id: 'owner', scope: { client: 'test', project: 'budget' }, mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'], budget, context: 'budget', visible_splits: ['development'], allow_export: false },
      run_budget: { ...budget, max_calls: 1 },
      request: { run_id: 'bounded', profile: 'caretaker', operator: 'proofreading@1', prompt: json({ directory, memory: 'missing-record', mode: 'active' }), provider: 'openai', model: fixtureModel('openai').id } };
    const path = join(directory, 'config.json'); await writeFile(path, json(config));
    assert.equal(command('ribosome', ['run', path], 2).disposition, 'exhausted');
    const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
    try { assert.equal(db.prepare("SELECT count(*) n FROM permits WHERE state='settled'").get().n, 1); }
    finally { db.close(); }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
