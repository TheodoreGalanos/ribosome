import { fixtureModel } from './model-fixture.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { DatabaseSync } from 'node:sqlite';
import { AttachmentClient, WriteCoordinator } from '@ribosome/agents/attachments';

function config(workspace) {
  return {
    workspace, state_dir: join(workspace, '.ribosome'), node: process.execPath,
    worker: (process.env.RIBOSOME_TEST_WORKER ?? resolve('tests/integration/attachment-worker-fixture.mjs')),
    grant: {
      id: 'attachment-grant', scope: { client: 'test', project: 'attachment' },
      mode: 'observe', paths: ['report.txt'], tools: [], profiles: ['caretaker'],
      budget: { max_calls: 30, max_tokens: '1000000', max_cost_microusd: '1000000',
        max_actions: 20, max_work_items: 10, max_depth: 2, deadline_ms: String(Date.now() + 60000) },
      context: 'attachment-tests', visible_splits: ['development'], allow_export: false,
    },
    request: { run_id: 'host-template', profile: 'caretaker', operator: 'proofreading@1',
      prompt: 'Inspect the observed report.', provider: 'openai', model: fixtureModel('openai').id },
    tools: {},
  };
}

test('explicit attachment host negotiates its own protocol and exits on closed input', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-attachment-'));
  try {
    const file = join(directory, 'host.json');
    await writeFile(file, JSON.stringify(config(directory)));
    const result = spawnSync((process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome')), ['host', file], {
      encoding: 'utf8', timeout: 10000,
      input: JSON.stringify({ jsonrpc: '2.0', id: 'hello', method: 'host.hello', params: { protocol: 'ribosome-host/1' } }) + '\n',
    });
    assert.equal(result.status, 0, result.stderr);
    const reply = JSON.parse(result.stdout.trim());
    assert.equal(reply.id, 'hello');
    assert.equal(reply.result.protocol, 'ribosome-host/1');
    assert.ok(reply.result.capabilities.includes('observe'));
  } finally { await rm(directory, { recursive: true, force: true }); }
});

const hash = content => `sha256:${createHash('sha256').update(content).digest('hex')}`;
function event(sequence = 1, overrides = {}) {
  return { id: `event-${sequence}`, producer: 'custom', sequence: String(sequence), kind: 'tool.completed', timestamp_ms: String(Date.now()), parents: [], correlation: 'task', artifacts: [{ path: 'report.txt', version: hash('original') }], payload: { observation: 'Report needs inspection' }, ...overrides };
}
async function until(predicate, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { const value = await predicate(); if (value) return value; await new Promise(resolve => setTimeout(resolve, 25)); }
  throw new Error('Timed out awaiting attachment condition');
}
async function fixture(change = () => {}, onDiagnostic) {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-attachment-'));
  const settings = config(directory); change(settings);
  await writeFile(join(directory, 'report.txt'), 'original');
  await writeFile(join(directory, 'independent.txt'), 'preserve me');
  const file = join(directory, 'host.json'); await writeFile(file, JSON.stringify(settings));
  const client = await AttachmentClient.start({ executable: (process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome')), config: file, onDiagnostic });
  return { directory, settings, file, client, async close() { try { await client.close(); } finally { await rm(directory, { recursive: true, force: true }); } } };
}
const open = (id = 'attachment') => ({ id, execution_id: `execution-${id}`, connector: 'custom', connector_version: '1', start: 'now', capabilities: ['observe'] });

test('generic startup integration returns scoped findings, deduplicates input and preserves files', async () => {
  const f = await fixture();
  try {
    const received = [];
    const a = await f.client.attach({ executionId: 'custom-loop', onFeedback: value => { received.push(value); } });
    const e = event(); await a.publish(e); await a.publish(e);
    await a.finish();
    assert.equal(a.error, undefined);
    assert.ok(received.length >= 1);
    const record = await a.readRecord(received[0].record_refs[0]);
    assert.equal(record.kind, 'finding');
    assert.deepEqual(record.body.evidence_refs, [`${a.id}:event-1`]);
    assert.equal(await readFile(join(f.directory, 'report.txt'), 'utf8'), 'original');
    const second = await f.client.attach({ executionId: 'another-loop', onFeedback: () => {} });
    await assert.rejects(second.readRecord(record.id), /not published/);
    await a.detach();
    await assert.rejects(a.publish(event(2)), /detached/);
  } finally { await f.close(); }
});

test('ingestion remains responsive while a real registered command holds the runtime', async () => {
  const f = await fixture(c => {
    c.grant.mode = 'apply'; c.grant.tools = ['slow-check']; c.request.prompt = 'slow-check';
    c.tools = { 'slow-check': { program: process.execPath, args: ['-e', "require('fs').writeFileSync('started','1');setTimeout(()=>{},2000)"], timeout_ms: 4000, reads: ['report.txt'], validates: ['report.txt'], writes: [] } };
  });
  try {
    const a = await f.client.attach({ executionId: 'slow-source', onFeedback: () => {} });
    await a.publish(event());
    await until(async () => { try { return await readFile(join(f.directory, 'started'), 'utf8'); } catch { return false; } });
    const before = Date.now();
    await a.publish(event(2, { kind: 'message.completed' }));
    const status = await a.status();
    assert.ok(Date.now() - before < 1000, 'ingestion/status must not wait for the two-second tool');
    assert.equal(status.running_work, 1);
    await a.finish();
  } finally { await f.close(); }
});

test('feedback becomes stale when its artifact changes before delivery', async () => {
  const f = await fixture();
  try {
    await f.client.peer.call('attachment.open', open());
    await f.client.peer.call('attachment.events', { attachment_id: 'attachment', events: [event()] });
    await until(async () => (await f.client.peer.call('attachment.status', { attachment_id: 'attachment' })).pending_feedback === 1);
    await writeFile(join(f.directory, 'report.txt'), 'changed by source');
    assert.deepEqual((await f.client.peer.call('attachment.feedback', { attachment_id: 'attachment' })).items, []);
    assert.equal((await f.client.peer.call('attachment.status', { attachment_id: 'attachment' })).pending_feedback, 0);
  } finally { await f.close(); }
});

test('Pi feedback inherits artifact access and deletion removes its copied summary', async () => {
  const diagnostics = [];
  const f = await fixture(c => { c.request.prompt = 'feedback-source'; }, text => diagnostics.push(text));
  try {
    const marker = 'WITHDRAWN-ATTACHMENT-SOURCE';
    await writeFile(join(f.directory, 'report.txt'), marker);
    await f.client.peer.call('attachment.open', open());
    await f.client.peer.call('attachment.events', { attachment_id: 'attachment', events: [event(1, { artifacts: [] })] });
    await until(async () => (await f.client.peer.call('attachment.status', { attachment_id: 'attachment' })).pending_feedback === 1);
    const first = (await f.client.peer.call('attachment.feedback', { attachment_id: 'attachment' })).items[0];
    assert.ok(first.summary.includes(marker));
    assert.deepEqual(first.record_refs, []);
    assert.deepEqual(first.artifact_versions, []);
    await rm(join(f.directory, 'report.txt'));
    assert.deepEqual((await f.client.peer.call('attachment.feedback', { attachment_id: 'attachment' })).items, []);
    const db = new DatabaseSync(join(f.directory, '.ribosome', 'ribosome.db'));
    try {
      const row = db.prepare('SELECT body FROM attachment_feedback WHERE id=?').get(first.id);
      assert.ok(!row.body.includes(marker), 'delivery rewrote a deleted summary into durable feedback');
    } finally { db.close(); }
  } catch (error) { throw new Error(`${error.message}\n${diagnostics.join('')}`, { cause: error }); }
  finally { await f.close(); }
});

test('advisory feedback survives host loss and is redelivered under its original ID', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-attachment-restart-'));
  const file = join(directory, 'host.json');
  await writeFile(file, JSON.stringify(config(directory))); await writeFile(join(directory, 'report.txt'), 'original');
  let child;
  let client;
  const start = async () => { child = spawn((process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome')), ['host', file], { stdio: ['pipe', 'pipe', 'pipe'] }); child.stderr.resume(); client = await AttachmentClient.connect(child.stdout, child.stdin, 5000); };
  try {
    await start();
    await client.peer.call('attachment.open', open());
    await client.peer.call('attachment.events', { attachment_id: 'attachment', events: [event()] });
    await until(async () => (await client.peer.call('attachment.status', { attachment_id: 'attachment' })).pending_feedback === 1);
    const first = (await client.peer.call('attachment.feedback', { attachment_id: 'attachment' })).items[0];
    child.kill('SIGKILL'); await once(child, 'exit');
    await start();
    const restored = await client.peer.call('attachment.open', open());
    assert.equal(restored.state, 'active');
    const status = await client.peer.call('attachment.status', { attachment_id: 'attachment' });
    assert.equal(status.attachment.state, 'active', JSON.stringify(status));
    const second = (await client.peer.call('attachment.feedback', { attachment_id: 'attachment' }).catch(async error => {
      throw new Error(`${error.message}: ${JSON.stringify(await client.peer.call('attachment.status', { attachment_id: 'attachment' }))}`);
    })).items[0];
    assert.equal(second.id, first.id); assert.equal(second.attempts, 2);
    await client.peer.call('attachment.ack', { attachment_id: 'attachment', feedback_id: second.id, outcome: 'acknowledged', detail: 'Received after restart' });
    assert.deepEqual((await client.peer.call('attachment.feedback', { attachment_id: 'attachment' })).items, []);
    child.stdin.end(); await once(child, 'exit');
  } finally { child?.kill(); client?.peer.close(); await rm(directory, { recursive: true, force: true }); }
});

test('cooperative repair waits for a writer, applies checked output and releases queued writes', async () => {
  const f = await fixture(c => {
    c.grant.mode = 'apply'; c.grant.paths.push('independent.txt'); c.grant.writable_paths = ['report.txt'];
    c.grant.tools = ['report-check']; c.grant.required_checks = ['report-check'];
    c.attachment = { allow_coordinated_writes: true };
    c.tools = { 'report-check': { program: process.execPath, args: ['-e', "if(require('fs').readFileSync('report.txt','utf8')!=='corrected')process.exit(1)"], timeout_ms: 2000, reads: ['report.txt'], validates: ['report.txt'], writes: [] } };
  });
  try {
    const received = [];
    const a = await f.client.attach({ executionId: 'writer', coordinatedWrites: true, onFeedback: feedback => { received.push(feedback); } });
    await a.publish(event()); await a.finish();
    const coordinator = new WriteCoordinator();
    let release;
    const inFlight = coordinator.run(async () => { await new Promise(resolve => { release = resolve; }); });
    const repair = a.repair(coordinator);
    const after = coordinator.run(async () => readFile(join(f.directory, 'report.txt'), 'utf8'));
    release(); await inFlight; await repair;
    assert.equal(await after, 'corrected');
    assert.equal(await readFile(join(f.directory, 'independent.txt'), 'utf8'), 'preserve me');
    await a.finish();
    assert.ok(received.some(r => r.kind === 'proposal' && r.disposition === 'completed'));
  } finally { await f.close(); }
});

test('detach cancels pending feedback and cannot resume under the same attachment ID', async () => {
  const f = await fixture();
  try {
    await f.client.peer.call('attachment.open', open());
    await f.client.peer.call('attachment.events', { attachment_id: 'attachment', events: [event()] });
    await until(async () => (await f.client.peer.call('attachment.status', { attachment_id: 'attachment' })).pending_feedback === 1);
    await f.client.peer.call('attachment.detach', { attachment_id: 'attachment' });
    await assert.rejects(f.client.peer.call('attachment.feedback', { attachment_id: 'attachment' }), /not active/);
    await assert.rejects(f.client.peer.call('attachment.open', open()), /terminal|detached/);
  } finally { await f.close(); }
});

test('worker failure is published as failure rather than successful silent completion', async () => {
  const f = await fixture(c => { c.worker = '/absent/ribosome-worker.js'; });
  try {
    const feedback = [];
    const a = await f.client.attach({ executionId: 'missing-worker', onFeedback: value => { feedback.push(value); } });
    await a.publish(event());
    await a.finish().catch(error => assert.match(error.message, /interrupted|not active|worker|Worker|absolute/));
    const status = await a.status();
    assert.ok(feedback.some(f => f.disposition === 'failed' || f.disposition === 'interrupted') || status.attachment.state === 'interrupted');
    assert.equal(await readFile(join(f.directory, 'report.txt'), 'utf8'), 'original');
  } finally { await f.close(); }
});

for (const mode of ['unavailable-check', 'uncooperative-writer']) test(`repair preserves the live workspace on ${mode}`, async () => {
  const f = await fixture(c => {
    c.grant.mode = 'apply'; c.grant.paths.push('independent.txt'); c.grant.writable_paths = ['report.txt'];
    c.grant.tools = ['report-check']; c.grant.required_checks = ['report-check']; c.attachment = { allow_coordinated_writes: true };
    if (mode === 'uncooperative-writer') c.tools = { 'report-check': { program: process.execPath, args: ['-e', `require('fs').writeFileSync(${JSON.stringify(join(c.workspace, 'report.txt'))}, 'concurrent owner edit')`], timeout_ms: 2000, reads: ['report.txt'], validates: ['report.txt'], writes: [] } };
  });
  try {
    const a = await f.client.attach({ executionId: mode, coordinatedWrites: true, onFeedback: () => {} });
    await a.publish(event()); await a.finish();
    await a.repair(new WriteCoordinator()); await a.finish();
    assert.equal(await readFile(join(f.directory, 'report.txt'), 'utf8'), mode === 'unavailable-check' ? 'original' : 'concurrent owner edit');
    assert.equal(await readFile(join(f.directory, 'independent.txt'), 'utf8'), 'preserve me');
    const status = await a.status();
    const inspected = spawnSync((process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome')), ['inspect', join(f.directory, '.ribosome/ribosome.db'), status.attachment.handoff.work_id], { encoding: 'utf8' });
    assert.equal(inspected.status, 0, inspected.stderr);
    const effects = JSON.parse(inspected.stdout).effects;
    assert.ok(effects.some(e => e.action.kind === 'apply' && e.status !== 'succeeded'));
  } finally { await f.close(); }
});

for (const lostOutcome of [false, true]) test(`a worker lost after application keeps the writer stopped until ${lostOutcome ? 'explicit host settlement' : 'receipt reconciliation'}`, async t => {
  const f = await fixture(c => {
    c.grant.mode = 'apply'; c.grant.paths.push('independent.txt'); c.grant.writable_paths = ['report.txt'];
    c.grant.tools = ['report-check']; c.grant.required_checks = ['report-check']; c.attachment = { allow_coordinated_writes: true };
    c.request.prompt = 'crash-after-apply';
    c.tools = { 'report-check': { program: process.execPath, args: ['-e', "if(require('fs').readFileSync('report.txt','utf8')!=='corrected')process.exit(1)"], timeout_ms: 2000, reads: ['report.txt'], validates: ['report.txt'], writes: [] } };
  });
  t.after(() => f.close());
  {
    const options = { id: 'lost-repair', executionId: 'recover-writer', coordinatedWrites: true, onFeedback: () => {} };
    const a = await f.client.attach(options);
    await a.publish(event()); await a.finish();
    const coordinator = new WriteCoordinator();
    if (lostOutcome) {
      const db = new DatabaseSync(join(f.directory, '.ribosome/ribosome.db'));
      db.exec("CREATE TRIGGER lose_apply_outcome BEFORE UPDATE OF observation ON effects WHEN json_extract(NEW.body,'$.action.kind')='apply' BEGIN SELECT RAISE(FAIL,'injected application outcome loss'); END");
      db.close();
    }
    await assert.rejects(a.repair(coordinator), lostOutcome ? /injected application outcome loss/ : /active|interrupted|Worker|worker|Repair/);
    assert.equal(await readFile(join(f.directory, 'report.txt'), 'utf8'), 'corrected');
    const status = await a.status(); assert.equal(status.attachment.handoff.state, lostOutcome ? 'held' : 'unknown');
    await assert.rejects(coordinator.run(async () => 'must stay stopped'));
    const restored = await f.client.attach(options);
    if (lostOutcome) {
      const db = new DatabaseSync(join(f.directory, '.ribosome/ribosome.db'));
      db.exec('DROP TRIGGER lose_apply_outcome');
      const { id } = db.prepare("SELECT id FROM effects WHERE run_id=? AND json_extract(body,'$.action.kind')='apply'").get(status.attachment.handoff.work_id);
      db.close();
      await assert.rejects(restored.reconcileRepair(coordinator, status.attachment.handoff.generation), /unknown effects/);
      const view = await f.client.inspectEffect(id);
      await f.client.settleEffect({ operation_id: id, expected_receipt_version: view.receipt_version, executor_stopped: true, workspace_versions: view.workspace_versions, reason: 'The maintenance executor exited; its current output needs fresh validation.', source_refs: [] });
    }
    await restored.reconcileRepair(coordinator, status.attachment.handoff.generation);
    await restored.finish();
    assert.equal(await coordinator.run(async () => readFile(join(f.directory, 'report.txt'), 'utf8')), 'corrected');
    const inspected = spawnSync((process.env.RIBOSOME_TEST_CLI ?? resolve('target/debug/ribosome')), ['inspect', join(f.directory, '.ribosome/ribosome.db'), status.attachment.handoff.work_id], { encoding: 'utf8' });
    assert.equal(inspected.status, 0, inspected.stderr);
    const application = JSON.parse(inspected.stdout).effects.filter(e => e.action.kind === 'apply');
    assert.equal(application.length, 1);
    assert.equal(application[0].status, lostOutcome ? 'unknown' : 'succeeded');
    if (lostOutcome) assert.ok(application[0].settlement);
  }
});
