import assert from 'node:assert/strict';
import { appendFileSync, existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
let settings, phase = 0, resultPath, sourceMemory, inlineBytes = 0, summaries = 0;
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (settings.mode === 'export' && method === 'tool.call' && params.method === 'training.export') {
    const product = JSON.parse(result.content);
    sourceMemory = settings.memory;
    resultPath = product.artifact.path;
    writeFileSync(join(settings.directory, 'reference.json'), JSON.stringify(result));
    writeFileSync(join(settings.directory, 'export-file.json'), JSON.stringify(product));
  }
  if (settings.mode.startsWith('revision') && method === 'tool.call' && params.method === 'record.read') {
    sourceMemory = settings.source;
    resultPath = result.artifact.path;
    writeFileSync(join(settings.directory, 'reference.json'), JSON.stringify(result));
  }
  if (settings.mode === 'exchange' && method === 'tool.call' && params.method === 'record.read') {
    inlineBytes += Number(result.total_bytes);
    sourceMemory = result.sources.find(source => source.kind === 'record').id;
    writeFileSync(join(settings.directory, 'reference.json'), JSON.stringify(result));
  }
  if ((method === 'tool.call' && params.method === 'search.query') || (method === 'tool.result' && params.id === 'result-0')) {
    sourceMemory = result.sources.find(source => source.kind === 'record').id;
    assert.ok(Number(result.total_bytes) > 400000, `Search retained ${result.total_bytes} bytes: ${result.content.slice(0, 120)}`);
    assert.ok(Buffer.byteLength(JSON.stringify(result)) < 32768);
    resultPath = result.artifact.path;
    writeFileSync(join(settings.directory, 'reference.json'), JSON.stringify(result));
    if (method === 'tool.call' && settings.mode === 'resume' && !existsSync(join(settings.directory, 'fault'))) {
      writeFileSync(join(settings.directory, 'fault'), 'result retained before Pi received it');
      process.exit(23);
    }
  }
  if (method === 'session.checkpoint') assert.ok(Buffer.byteLength(JSON.stringify(params)) < 256 * 1024);
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  if (context.tools?.some(tool => tool.name === 'commit_summary')) {
    assert.ok(Buffer.byteLength(JSON.stringify(context.messages)) < 480 * 1024);
    const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
      content: [{ type: 'toolCall', id: `result-summary-${++summaries}`, name: 'commit_summary', arguments: { text: 'Historical tool observations are retained as readable artifacts. Inspect their contents before drawing conclusions.' } }], stopReason: 'toolUse', timestamp: Date.now(),
      usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
    const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
  }
  assert.ok(Buffer.byteLength(JSON.stringify(context.messages)) < 180000);
  appendFileSync(join(settings.directory, 'calls.jsonl'), JSON.stringify(context.messages) + '\n');
  const previous = context.messages.at(-1);
  let name, args;
  if (phase === 0) {
    if (settings.mode === 'export') { name = 'training_export'; args = { record_ids: [settings.memory], product: 'synthetic' }; }
    else if (settings.mode === 'exchange' || settings.mode.startsWith('revision')) { name = 'record_read'; args = { id: settings.memory }; }
    else { name = 'search_query'; args = { query: 'bulkyneedle', inventory: 'evidence', limit: 20, offset: 0 }; }
  } else if (phase === 1) {
    assert.equal(previous.role, 'toolResult');
    assert.equal(previous.isError, false, JSON.stringify(previous.content));
    let result = JSON.parse(previous.content[0].text);
    if (settings.mode === 'export') {
      assert.match(result.artifact.path, /^ribosome-export:/);
      resultPath = result.artifact.path;
      name = 'artifact_read'; args = { path: resultPath, offset: 8192, length: 16000 };
    } else {
      if (settings.mode === 'exchange') {
        assert.ok(inlineBytes > 180000);
        const results = context.messages.filter(message => message.role === 'toolResult');
        assert.equal(results.length, 12);
        assert.equal(new Set(results.map(message => message.toolCallId)).size, 12);
        assert.match(previous.content[0].text, /BEYOND-EXCERPT-MARKER/, 'Recent observations remain readable under context pressure');
        result = results.map(message => JSON.parse(message.content[0].text)).find(value => value.retained_result);
        assert.ok(result, 'Older observations remain addressable through retained references');
      } else assert.ok(Number(result.total_bytes) > (settings.mode.startsWith('revision') ? 32768 : 400000));
      assert.equal(result.complete, false);
      assert.doesNotMatch(result.excerpt ?? '', /BEYOND-EXCERPT-MARKER/);
      resultPath = result.retained_result.path;
      name = 'artifact_read'; args = { path: resultPath, offset: result.next_offset, length: 16000 };
    }
  } else if (phase === 2) {
    assert.equal(previous.isError, false);
    const chunk = JSON.parse(previous.content[0].text);
    assert.match(chunk.content, /BEYOND-EXCERPT-MARKER/);
    assert.equal(chunk.required_freshness, 'historical');
    if (settings.mode.startsWith('revision')) {
      // Host-policy fault: another owner revision has independent evidence.
      // Apply the actual persisted revision boundary while this Pi run is live.
      const db = new DatabaseSync(join(settings.directory, 'state/ribosome.db'));
      try {
        db.exec('BEGIN IMMEDIATE');
        const record = JSON.parse(db.prepare('SELECT body FROM records WHERE id=?').get(settings.memory).body);
        record.version = '2'; record.provenance.source_refs = []; record.body.content = 'Independent current observation';
        db.prepare('UPDATE records SET version=?,body=? WHERE id=?').run(record.version, JSON.stringify(record), record.id);
        db.prepare("DELETE FROM source_edges WHERE subject_kind='record' AND subject_id=?").run(record.id);
        db.prepare('DELETE FROM record_search WHERE id=?').run(record.id);
        db.prepare('INSERT INTO record_search(id,content) VALUES(?,?)').run(record.id, JSON.stringify(record.body));
        db.exec('COMMIT');
      } finally { db.close(); }
    }
    await call('record.retire', { id: sourceMemory, expected_version: '1', delete: true });
    if (settings.mode === 'revision-resume') process.exit(23);
    name = 'artifact_read'; args = { path: resultPath, offset: 0, length: 16000 };
  } else {
    assert.equal(previous.role, 'user', 'Withdrawn result and its failed read must cause a clean segment');
    assert.doesNotMatch(JSON.stringify(context.messages), /BEYOND-EXCERPT-MARKER|EXPORT-DERIVED-MARKER|REVISION-DERIVED-MARKER|bulkyneedle/);
    name = 'finish'; args = { disposition: 'completed', summary: 'Read the retained artifact in bounded chunks; withdrawal denied the original and derived copies.' };
  }
  const round = phase++;
  const content = round === 0 && settings.mode === 'exchange'
    ? Array.from({ length: 12 }, (_, i) => ({ type: 'toolCall', id: `result-${round}-${i}`, name, arguments: args }))
    : [{ type: 'toolCall', id: `result-${round}`, name, arguments: args }];
  if (settings.mode.startsWith('revision') && round === 2) content.unshift({ type: 'text', text: 'REVISION-DERIVED-MARKER: interpretation of the original observation.' });
  if (settings.mode === 'export' && round === 2) content.unshift({ type: 'text', text: 'EXPORT-DERIVED-MARKER: interpretation of the exported evidence.' });
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content, stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt); if (request.checkpoint) phase = settings.mode === 'revision-resume' ? 3 : 1; return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
