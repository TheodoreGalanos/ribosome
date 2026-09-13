import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { AttachmentClient, WriteCoordinator } from '@ribosome/agents/attachments';
import { fixtureModel } from './model-fixture.mjs';
const hash = content => `sha256:${createHash('sha256').update(content).digest('hex')}`;

test('R7 A: attached repair recovers finalization, preserves downstream staleness and withdraws memory before continuation', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-r7-recovery-'));
  const executable = process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome');
  const database = join(directory, 'state/ribosome.db'), file = join(directory, 'config.json');
  const capture = join(directory, 'provider.jsonl');
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: process.env.RIBOSOME_TEST_WORKER ?? resolve('tests/integration/attachment-worker-fixture.mjs'),
    grant: { id: 'r7', scope: { client: 'test', project: 'integrated' }, mode: 'apply', paths: ['source.txt', 'report.txt', 'downstream.txt', 'independent.txt'], writable_paths: ['report.txt'], tools: ['report-check'], required_checks: ['report-check'], profiles: ['caretaker'], budget: { max_calls: 48, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 15, max_work_items: 8, max_depth: 2, deadline_ms: String(Date.now() + 120000) }, context: 'integrated', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'template', profile: 'caretaker', operator: 'proofreading@1', provider: 'openai', model: fixtureModel('openai').id, prompt: `r7:${JSON.stringify({ capture })}` }, tools: {}, attachment: { allow_coordinated_writes: true, event_kinds: ['tool.completed'], batch_size: 1 } };
  const command = args => { const r = spawnSync(executable, args, { encoding: 'utf8', timeout: 15000, env: { PATH: process.env.PATH } }); assert.equal(r.status, 0, r.stderr); return JSON.parse(r.stdout); };
  let child, client;
  const start = async () => { child = spawn(executable, ['host', file], { stdio: ['pipe', 'pipe', 'pipe'], env: { PATH: process.env.PATH } }); child.stderr.resume(); client = await AttachmentClient.connect(child.stdout, child.stdin, 5000); };
  const stop = async () => { if (child && child.exitCode === null && child.signalCode === null) { const ended = once(child, 'exit'); child.kill('SIGKILL'); await ended; } };
  try {
    await mkdirState();
    await writeFile(file, JSON.stringify(config));
    const provenance = { origin: 'synthetic', source_refs: [], scenario_family: 'r7-fault-fixture', split: 'development', limitations: ['Scripted provider, real attached host and filesystem.'] };
    const obligation = path => ({ kind: 'obligation', provenance, body: { description: `Current ${path} is supported`, owner: 'host', subject: path, state: 'open', affected_outputs: [path], created_ms: '1', consequence_boundary: 'delivery', evidence_refs: [] } });
    const recordsFile = join(directory, 'records.json');
    await writeFile(recordsFile, JSON.stringify([obligation('report.txt'), obligation('downstream.txt'), { kind: 'memory', provenance, body: { kind: 'episodic', content: 'WITHDRAW-R7-MEMORY', applicability: 'fixture', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } }]));
    const [property, , memory] = command(['records', file, recordsFile]);
    config.tools['report-check'] = { program: process.execPath, args: ['-e', "const f=require('fs');if(f.readFileSync('report.txt','utf8')!=='corrected'||f.readFileSync('source.txt','utf8')!=='revision-2')process.exit(1)"], timeout_ms: 2000, reads: ['source.txt', 'report.txt'], validates: ['report.txt'], writes: [], validated_properties: [{ obligation: { id: property.id, version: property.version }, path: 'report.txt' }] };
    await writeFile(file, JSON.stringify(config));
    const db = new DatabaseSync(database);
    for (const [source, dependent] of [['source.txt', 'report.txt'], ['report.txt', 'downstream.txt']]) {
      const edge = { source: { path: source, version: hash(source === 'source.txt' ? 'revision-1' : 'original') }, dependent: { path: dependent, version: hash(dependent === 'report.txt' ? 'original' : 'old derived output') }, basis: 'host', evidence_refs: [] };
      db.prepare('INSERT INTO dependencies VALUES (?,?,?,?,?)').run('test', 'integrated', source, dependent, JSON.stringify(edge));
    }
    db.close(); await start();
    const options = { id: 'r7-attachment', executionId: 'external-worker', coordinatedWrites: true, onFeedback: () => {} };
    const attachment = await client.attach(options);
    const coordinator = new WriteCoordinator();
    await coordinator.run(() => writeFile(join(directory, 'source.txt'), 'revision-2'));
    await attachment.publish({ id: 'source-change', producer: 'worker', sequence: '1', kind: 'tool.completed', timestamp_ms: String(Date.now()), parents: [], correlation: 'edit-source', artifacts: [{ path: 'source.txt', version: hash('revision-2') }], payload: { source_revision: 2 } });
    await attachment.finish();
    const fault = new DatabaseSync(database);
    fault.exec("CREATE TRIGGER interrupt_apply_finalization BEFORE UPDATE OF phase ON effects WHEN NEW.phase='finalized' AND json_extract(NEW.body,'$.action.kind')='apply' BEGIN SELECT RAISE(FAIL,'R7 finalization interruption'); END"); fault.close();
    await assert.rejects(attachment.repair(coordinator));
    assert.equal(await readFile(join(directory, 'report.txt'), 'utf8'), 'corrected');
    await assert.rejects(coordinator.run(async () => 'unsafe'));
    const status = await attachment.status();
    await stop();
    const recovery = new DatabaseSync(database); recovery.exec('DROP TRIGGER interrupt_apply_finalization'); recovery.close();
    await start();
    const restored = await client.attach(options);
    await restored.reconcileRepair(coordinator, status.attachment.handoff.generation);
    await restored.finish();
    assert.equal(await coordinator.run(() => readFile(join(directory, 'independent.txt'), 'utf8')), 'preserve me');
    const inspected = command(['inspect', database, status.attachment.handoff.work_id]);
    const applications = inspected.effects.filter(e => e.action.kind === 'apply');
    assert.equal(applications.length, 1); assert.equal(applications[0].status, 'succeeded');
    assert.equal(applications[0].restored_properties.length, 0, 'lost writer authority cannot restore validity from the previous check');
    await restored.detach(); await stop();
    config.request = { ...config.request, run_id: 'fresh-validation', prompt: `r7:${JSON.stringify({ capture, verify: true })}` };
    await writeFile(file, JSON.stringify(config));
    assert.equal(command(['run', file]).disposition, 'completed');
    const validated = command(['inspect', database, 'fresh-validation']);
    assert.ok(validated.effects.some(e => e.restored_properties?.some(p => p.obligation.id === property.id)));
    const checked = new DatabaseSync(database, { readOnly: true });
    assert.equal(checked.prepare("SELECT count(*) n FROM invalidated WHERE path='report.txt'").get().n, 0);
    assert.equal(checked.prepare("SELECT count(*) n FROM invalidated WHERE path='downstream.txt'").get().n, 1);
    checked.close();
    config.request = { ...config.request, run_id: 'withdraw-and-continue', prompt: `r7:${JSON.stringify({ capture, memory: memory.id, withdraw: true })}` };
    await writeFile(file, JSON.stringify(config));
    const continuation = command(['run', file]); assert.equal(continuation.disposition, 'completed');
    const calls = (await readFile(capture, 'utf8')).trim().split('\n').map(JSON.parse).filter(c => c.run_id === 'withdraw-and-continue');
    assert.ok(calls.some(c => JSON.stringify(c.context).includes('WITHDRAW-R7-MEMORY')));
    assert.ok(calls.length >= 4);
    for (const call of calls.slice(2)) assert.doesNotMatch(JSON.stringify(call.context), /WITHDRAW-R7-MEMORY/, 'the first and subsequent provider requests after retirement must exclude the memory');
    assert.equal(await readFile(join(directory, 'downstream.txt'), 'utf8'), 'old derived output');
  } finally { await stop(); await rm(directory, { recursive: true, force: true }); }
  async function mkdirState() {
    for (const [path, content] of [['source.txt', 'revision-1'], ['report.txt', 'original'], ['downstream.txt', 'old derived output'], ['independent.txt', 'preserve me']]) await writeFile(join(directory, path), content);
  }
});
