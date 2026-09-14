import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const call = peer.call.bind(peer);
let settings, restoring, phase = 0, interrupted = false, artifactSaved = false, lastTool, captures = 0;
const metrics = { retainedBytes: 0, maxFrameBytes: 0, maxCheckpointBytes: 0 };
async function withdraw() {
  if (settings.withdrawal === 'expiry') {
    // Expire only this memory after observation. Worker startup must not consume
    // its validity window, and subsequent requests must still check expiry.
    const db = new DatabaseSync(join(settings.directory, 'state/ribosome.db'));
    try {
      db.prepare("UPDATE records SET body=json_set(body,'$.body.expires_ms',?) WHERE id=?").run(String(Date.now() - 1), settings.memory);
    } finally { db.close(); }
  }
  else if (settings.withdrawal === 'access') {
    // Host-policy fault injection: the source is no longer released to the
    // run's development-only grant, although its content still exists.
    const db = new DatabaseSync(join(settings.directory, 'state/ribosome.db'));
    try {
      db.prepare("UPDATE records SET split='holdout',body=json_set(body,'$.provenance.split','holdout') WHERE id=?").run(settings.memory);
    } finally { db.close(); }
  }
  else await call('record.retire', { id: settings.memory, expected_version: '1', delete: settings.withdrawal === 'delete' });
}
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  metrics.maxFrameBytes = Math.max(metrics.maxFrameBytes, Buffer.byteLength(JSON.stringify(params)) + 200, Buffer.byteLength(JSON.stringify(result)) + 200);
  if (method === 'tool.call') metrics.retainedBytes += Number(result.total_bytes);
  if (method === 'session.context.append') {
    lastTool = params.entries.at(-1)?.message?.toolName;
  }
  if (method === 'session.checkpoint') {
    metrics.maxCheckpointBytes = Math.max(metrics.maxCheckpointBytes, Buffer.byteLength(JSON.stringify(params)));
    if (settings.mode === 'long' && !restoring && phase === settings.reads && lastTool === 'record_read') {
      writeFileSync(join(settings.directory, 'metrics.json'), JSON.stringify(metrics));
      process.exit(23);
    }
  }
  if (method === 'session.context.append' && params.entries?.at(-1)?.message?.toolName === 'artifact_read') artifactSaved = true;
  if (method === 'session.checkpoint' && settings.mode !== 'long' && !restoring && !interrupted && phase === 2 && artifactSaved) {
    interrupted = true;
    if (settings.mode === 'active') await withdraw();
    else process.exit(23);
  }
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  let name, args, text;
  if (context.tools?.some(tool => tool.name === 'commit_summary')) {
    name = 'commit_summary'; args = { text: 'Retained observation. Continue the owner task and preserve receipt identity.' };
  } else if (settings.mode === 'long' && !restoring) {
    name = 'record_read'; args = { id: settings.memory };
  } else if (restoring || phase >= 2) {
    writeFileSync(join(settings.directory, captures === 0 ? 'after.json' : 'after-again.json'), JSON.stringify(context));
    if (captures++ === 0) { name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000 }; }
    else { name = 'finish'; args = { disposition: 'completed', summary: 'Captured two actual provider requests after continuation.' }; }
  } else if (phase === 0) {
    name = 'record_read'; args = { id: settings.memory };
  } else {
    writeFileSync(join(settings.directory, 'before.json'), JSON.stringify(context));
    text = 'DERIVED-CONTEXT-MARKER: this interpretation depends on the retrieved memory.';
    name = 'artifact_read'; args = { path: 'report.txt', offset: 0, length: 1000 };
  }
  const content = [...(text ? [{ type: 'text', text }] : []), { type: 'toolCall', id: `context-${restoring ? "resume" : "initial"}-${name === "commit_summary" ? "summary" : ++phase}`, name, arguments: args }];
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content, stopReason: 'toolUse', timestamp: Date.now(), usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => {
  settings = JSON.parse(request.prompt); restoring = Boolean(request.checkpoint);
  if (settings.mode === 'retire') {
    await withdraw();
    return { disposition: 'completed', summary: 'Memory retired by another workflow.' };
  }
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
