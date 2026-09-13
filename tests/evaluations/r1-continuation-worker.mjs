import { appendFileSync, existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout), call = peer.call.bind(peer);
const marker = 'WITHDRAWN-R1-LIVE-BRIEF';
let settings;
// Capture only boundary measurements. URLs, credentials, model settings and
// provider response bodies are never written to this qualification report.
const fetchProvider = globalThis.fetch;
globalThis.fetch = async (input, init) => {
  const body = typeof init?.body === 'string' ? init.body : input instanceof Request ? await input.clone().text() : '';
  appendFileSync(join(settings.directory, 'provider-boundaries.jsonl'), JSON.stringify({
    after_withdrawal: existsSync(join(settings.directory, 'withdrawal-confirmed')),
    request_bytes: Buffer.byteLength(body), contains_withdrawn_marker: body.includes(marker),
  }) + '\n');
  return fetchProvider(input, init);
};
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (method === 'tool.call' && params.method === 'artifact.read' && /^inspection-\d+\.json$/.test(params.arguments.path)) {
    const chunk = JSON.parse(result.content);
    if (typeof chunk.content === 'string') appendFileSync(join(settings.directory, 'inspection-reads.jsonl'), JSON.stringify({ path: params.arguments.path, offset: chunk.offset, bytes: Buffer.byteLength(chunk.content) }) + '\n');
  }
  if (method === 'session.checkpoint') appendFileSync(join(settings.directory, 'checkpoints.jsonl'), JSON.stringify({ bytes: Buffer.byteLength(JSON.stringify(params)), pending_operations: params.pending_operations.length }) + '\n');
  if (method === 'session.compaction.commit' && !existsSync(join(settings.directory, 'withdrawal-confirmed'))) {
    await call('record.retire', { id: settings.brief, expected_version: '1', delete: true });
    writeFileSync(join(settings.directory, 'withdrawal-confirmed'), 'source deletion acknowledged after summary commit');
    process.exit(23);
  }
  return result;
};
// No stream override: the installed Pi provider performs every reasoning and
// compaction call, under the ordinary Rust permit and usage protocol.
const execution = createAgentExecution(peer);
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async request => { settings = JSON.parse(request.prompt.match(/\nQualification settings: (.+)$/s)[1]); return execution.run(request); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
