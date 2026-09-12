// Actual Pi loop, scripted only at the model boundary. Infrastructure evidence,
// not a live-model recognition result or a production mock mode.
import { createAgentExecution, RpcPeer } from '@ribosome/agents';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';

const peer = new RpcPeer(process.stdin, process.stdout);
let step = 0;
let request;
let events = [];
let artifact;
let finding;
let branch;
let intervention;
let failed = false;
const call = peer.call.bind(peer);
peer.call = async (method, params, signal) => {
  const result = await call(method, params, signal);
  if (method === 'action.execute' && params.kind === 'apply' && result.status === 'succeeded' && request.prompt.includes('crash-after-apply')) process.exit(42);
  return result;
};
const execution = createAgentExecution(peer, async (model, context) => {
  const last = context.messages.at(-1);
  if (last?.role === 'toolResult') {
    failed ||= last.isError;
    const value = JSON.parse(last.content.filter(p => p.type === 'text').map(p => p.text).join(''));
    if (last.toolName === 'evidence_read') events = value.events ?? [];
    if (last.toolName === 'artifact_read') artifact = value.artifact;
    if (last.toolName === 'record_submit') {
      if (value.kind === 'finding') finding = value.id;
      if (value.kind === 'intervention') intervention = value.id;
    }
    if (last.toolName === 'action_execute') {
      failed ||= value.status !== 'succeeded';
      if (value.action?.kind === 'branch') branch = value.output;
    }
  }
  const provenance = { origin: 'observed', source_refs: events.map(e => e.id), scenario_family: 'attachment-fixture', split: 'development', limitations: ['Scripted provider used only for infrastructure verification.'] };
  const findingBody = { subject: 'report', observation: 'The source reported a report requiring inspection.', interpretation: 'Inspect the actual report before changing it.', evidence_refs: events.map(e => e.id), uncertainty: ['Infrastructure fixture does not establish semantic recognition.'], operator: request.operator };
  let call;
  if (request.operator === 'excision-repair@1') {
    const calls = [
      ['evidence_read', { cursor: '0', limit: 100 }],
      ['artifact_read', { path: 'report.txt', offset: 0, length: 1000 }],
      ['record_submit', { kind: 'finding', provenance, body: findingBody }],
      ['action_execute', { kind: 'branch' }],
      ['action_execute', { kind: 'edit', branch_id: branch, path: 'report.txt', expected_version: artifact?.version, content: 'corrected' }],
      ['record_submit', { kind: 'intervention', provenance, body: {
        kind: 'repair', subject: 'report', finding_ref: finding, read_versions: [], preserve: ['independent.txt'], replace: ['report.txt'], invalidate: [], recompute: [], required_checks: ['report-check'], bindings: {}, requested_effects: ['apply'], assumptions: ['The host has stopped every writer.'], fallback: 'Leave the live report unchanged.', operator: request.operator,
      } }],
      ['action_execute', { kind: 'apply', branch_id: branch, path: 'report.txt', expected_version: artifact?.version, content: 'corrected', intervention_ref: intervention }],
    ];
    call = calls[step];
  } else {
    const calls = [['evidence_read', { cursor: '0', limit: 100 }]];
    if (request.prompt.includes('slow-check')) calls.push(['action_execute', { kind: 'check', tool: 'slow-check' }]);
    calls.push(['record_submit', { kind: 'finding', provenance, body: findingBody }]);
    call = calls[step];
  }
  call ??= ['finish', { disposition: failed ? 'failed' : 'completed', summary: request.operator === 'excision-repair@1' ? 'The fixture inspected the durable repair outcome.' : 'A finding was saved against the observed source execution.' }];
  const [name, args] = call;
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id,
    content: [{ type: 'toolCall', id: `step-${step++}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(),
    usage: { input: 10, output: 10, cacheRead: 0, cacheWrite: 0, totalTokens: 20, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.run', async value => { request = value; return execution.run(value); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
