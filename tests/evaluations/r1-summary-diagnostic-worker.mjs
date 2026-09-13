import { Agent } from '@earendil-works/pi-agent-core';
import { Type } from 'typebox';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { modelTransport } from '../../packages/agents/dist/pi/model.js';
import { compactContext } from '../../packages/agents/dist/pi/compaction.js';

const peer = new RpcPeer(process.stdin, process.stdout);
let active;
peer.handle('bridge.hello', async hello => hello);
peer.handle('agent.cancel', async () => { active?.abort(); return { ok: true }; });
peer.onClose = () => { active?.abort(); process.stdin.destroy(); };
peer.handle('agent.run', async request => {
  const settings = JSON.parse(request.prompt);
  const transport = modelTransport(request);
  const observation = await peer.call('tool.call', { call_id: 'diagnostic-input', method: settings.mode === 'summary-answer' ? 'record.read' : 'artifact.read',
    arguments: settings.mode === 'summary-answer' ? { id: settings.summary_id } : { path: 'facts.json', offset: 0, length: 8192 } });
  const result = JSON.parse(observation.content);
  const evidence = settings.mode === 'summary-answer' ? result.body.content : result.content;
  let state = await peer.call('session.context', {});
  const append = async (content, sources = []) => {
    state = await peer.call('session.context.append', { segment_id: state.segment_id, after: state.count,
      entries: [{ message: { role: 'user', content, timestamp: Date.now() }, sources }] });
  };
  // Match the normal Pi adapter: the first context item is the owner request,
  // which compaction retrieves separately from the pinned run request.
  if (state.count === '0') await append(request.prompt);
  // This is a controlled content diagnostic, not an autonomous investigation.
  // The harness supplies known evidence; every synthesis/answer is a real model call.
  await append(`Host-authored evidence for the requested task:\n${evidence}`, observation.sources);
  if (settings.mode === 'compact') {
    // Cross the ordinary compaction threshold without adding more task facts.
    await append('Irrelevant historical progress note. '.repeat(5000));
    await append('Continue the owner task from the evidence already supplied.');
    const { plan } = await peer.call('session.compaction.prepare', {});
    if (!plan) throw Error('Diagnostic did not reach the compaction boundary');
    const input = await peer.call('session.compaction.read', { id: plan.id });
    const inputText = input.messages.flatMap(message => typeof message.content === 'string' ? [message.content] : (message.content ?? []).filter(part => part.type === 'text').map(part => part.text));
    if (!inputText.some(text => text.startsWith('Host-authored evidence for the requested task:'))) throw Error('Diagnostic source evidence is absent from compaction input');
    await compactContext(peer, transport, plan);
    const summary = await peer.call('session.summary', { id: plan.id });
    const record = await peer.call('record.submit', { kind: 'memory',
      provenance: { origin: 'synthetic', source_refs: [summary.id], scenario_family: 'r1-summary-diagnostic', split: 'development', limitations: ['Controlled content diagnostic; not autonomous task qualification.'] },
      body: { kind: 'working', content: summary.text, applicability: 'This diagnostic only', evidence_refs: [summary.id], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } });
    return { disposition: 'completed', summary: JSON.stringify({ summary_id: record.id }) };
  }
  await peer.call('session.context', {});
  const meter = transport.metered(peer);
  let answer;
  active = new Agent({ initialState: { model: transport.model, thinkingLevel: 'off',
    systemPrompt: 'Answer the owner task using only the supplied evidence. Call submit_answer once. Missing units can be a supported unresolved outcome when the rules allow it. Do not invent missing facts.',
    tools: [{ name: 'submit_answer', label: 'Submit answer', description: 'Return the requested delivery rows for independent checking.',
      parameters: Type.Object({ rows: Type.Array(Type.Object({ asset: Type.String(), status: Type.Union([Type.Literal('ready'), Type.Literal('unresolved')]), accepted_count: Type.Integer(), total_kwh: Type.Union([Type.Number(), Type.Null()]) })) }),
      executionMode: 'sequential', replay: 'safe',
      async execute(_id, args) { answer = args.rows; return { content: [{ type: 'text', text: 'Answer captured for independent checking.' }], details: {}, terminate: true }; } }],
  }, streamFn: meter.streamFn, shouldStopAfterTurn: () => true });
  active.subscribe(async event => { if (event.type === 'message_end' && event.message.role === 'assistant') await meter.settle(event.message); });
  await active.prompt(`${settings.task}\n\nSupplied evidence:\n${evidence}`);
  if (meter.failure) throw meter.failure;
  if (active.state.errorMessage) throw Error(active.state.errorMessage);
  if (!answer || !meter.completedPermitId) throw Error('Model did not return a metered answer');
  return { disposition: 'completed', summary: JSON.stringify({ rows: answer }) };
});
