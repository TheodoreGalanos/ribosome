import assert from 'node:assert/strict';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { RpcPeer, createAgentExecution } from '@ribosome/agents';

// Only provider decisions are authored here. Pi, storage, effects, evaluation,
// admission and source withdrawal run through the ordinary library.
const peer = new RpcPeer(process.stdin, process.stdout);
const corrected = 'Read input and output. For each id compare all numeric values: if any differ, write unresolved status; otherwise keep one {id,value} row. Ignore additional fields. Preserve independent_cost. Write in a branch and run checker.';
let request, step;
const execution = createAgentExecution(peer, async (model, context) => {
  const messages = context.messages.filter(message => message.role === 'toolResult');
  assert.ok(messages.every(message => !message.isError), JSON.stringify(messages));
  const results = name => messages.filter(message => message.toolName === name).map(message => JSON.parse(message.content[0].text));
  const last = name => results(name).at(-1);
  const assignment = request.prompt.includes('\nAssignment:\n') ? JSON.parse(request.prompt.split('\nAssignment:\n').at(-1)) : {};
  const provenance = refs => ({ origin: 'synthetic', source_refs: refs, scenario_family: 'lifecycle-provider-fixture', split: 'development', limitations: ['Authored provider decisions test library mechanisms.'] });
  const submit = (kind, body, refs = []) => ['record_submit', { kind, body, provenance: provenance(refs) }];
  const finish = () => ['finish', { disposition: 'completed', summary: 'Completed the assigned fixture step; inspect stored evidence.' }];
  const search = (kind, inventory = 'evidence') => ['search_query', { query: '', kind, inventory, limit: 20, offset: 0 }];
  let call;
  if (request.invocation) {
    assert.equal(context.tools.some(tool => tool.name === 'evidence_read'), false);
    assert.ok(context.systemPrompt.includes(request.invocation.implementation.id));
    const binding = request.invocation.bindings;
    if (step < 2) call = ['artifact_read', { path: step === 0 ? binding.input : binding.output, offset: 0, length: 4096 }];
    else if (step === 2) call = ['action_execute', { kind: 'branch' }];
    else if (step === 3) {
      const [source, target] = results('artifact_read');
      const rows = JSON.parse(source.content), original = JSON.parse(target.content);
      const values = new Map(); let conflict = false;
      for (const row of rows) {
        if (values.has(row.id) && values.get(row.id) !== row.value) conflict = true;
        if (!values.has(row.id)) values.set(row.id, row.value);
      }
      const keepFirst = context.systemPrompt.includes('keep the first value and discard later rows');
      const output = conflict && !keepFirst ? { status: 'unresolved', independent_cost: original.independent_cost } : { status: 'resolved', rows: [...values].map(([id, value]) => ({ id, value })), independent_cost: original.independent_cost };
      call = ['action_execute', { kind: 'edit', branch_id: last('action_execute').output, path: binding.output, expected_version: target.artifact.version, content: JSON.stringify(output) }];
    } else if (step === 4) call = ['action_execute', { kind: 'check', tool: binding.checker, branch_id: results('action_execute')[0].output }];
    else call = finish();
  } else if (request.run_id === 'revise' || request.run_id === 'admit') {
    const references = [assignment.candidate.id, ...assignment.evaluation_refs];
    if (step < references.length) call = ['record_read', { id: references[step] }];
    else if (step === references.length && request.run_id === 'revise') {
      const donor = results('record_read')[0];
      assert.ok(results('record_read').slice(1).every(record => record.kind === 'evaluation'));
      call = submit('implementation', { ...donor.body, name: 'compare-values-reconciliation', material: corrected, instruction_contract: assignment.instruction_contract, evaluation_refs: [] }, references);
    } else if (step === references.length) {
      call = submit('recommendation', { implementation: assignment.candidate, context: assignment.context, decision: 'accepted', evaluation_refs: assignment.evaluation_refs, rationale: 'Independent checks passed for both local row cases.', restrictions: ['Two authored cases with id and numeric value; additional fields require reconsideration.'] }, references);
    } else if (step === references.length + 1 && request.run_id === 'admit') call = ['inventory_admission_request', { recommendation_id: last('record_submit').id }];
    else call = finish();
  } else if (request.run_id === 'remember') {
    if (step === 0) call = ['record_read', { id: assignment.candidate.id }];
    else if (step === 1) call = submit('memory', { kind: 'procedural', content: 'Compare values within each row id; leave contradictions unresolved.', applicability: 'Rows with id and numeric value. Additional fields require reconsideration.', evidence_refs: [assignment.candidate.id], counterexamples: ['Equal numbers can carry different units.'], responses: ['Retrieve the admitted instruction and inspect recipient inputs.'], regression_cases: ['conflicting-values'], conflicts: [], supersedes: [] }, [assignment.candidate.id]);
    else call = finish();
  } else if (request.run_id === 'select') {
    assert.equal(context.tools.some(tool => tool.name === 'evidence_read'), false);
    if (step === 0) call = search('memory');
    else if (step === 1) call = search('implementation', 'usable');
    else if (step === 2) call = ['artifact_read', { path: assignment.bindings.input, offset: 0, length: 4096 }];
    else if (step === 3) {
      assert.equal(results('search_query')[0].records.length, 1);
      const candidates = results('search_query')[1].records;
      assert.equal(candidates.length, 1);
      const donor = candidates[0];
      call = submit('transplant', { donor: { id: donor.id, version: donor.body.version }, recipient: assignment.recipient, bindings: assignment.bindings, adaptations: ['Bind current input and output paths.'], incompatibilities: [], checks: ['row-check'], fallback: 'Leave unresolved if incompatible.' }, [donor.id]);
    } else call = finish();
  } else if (request.run_id === 'withdraw') {
    if (step === 0) call = ['record_read', { id: assignment.finding }];
    else if (step === 1) call = ['record_read', { id: assignment.candidate.id }];
    else if (step === 2) call = ['record_retire', { id: assignment.candidate.id, expected_version: last('record_read').version, delete: false }];
    else if (step === 3) {
      assert.ok(!JSON.stringify(context).includes(corrected), 'withdrawn instructions returned to the provider');
      call = search('memory');
    } else { assert.equal(last('search_query').records.length, 0); call = finish(); }
  } else throw Error(`Unexpected fixture run: ${request.run_id}`);
  const [name, args] = call;
  assert.ok(context.tools.some(tool => tool.name === name), `${name} is unavailable`);
  const message = { role: 'assistant', api: model.api, provider: model.provider, model: model.id, content: [{ type: 'toolCall', id: `${request.run_id}-${step++}`, name, arguments: args }], stopReason: 'toolUse', timestamp: Date.now(), usage: { input: 10, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 12, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
  const stream = createAssistantMessageEventStream(); stream.push({ type: 'done', reason: 'toolUse', message }); return stream;
});
peer.handle('bridge.hello', async value => value);
peer.handle('agent.run', async value => { request = value; step = 0; return execution.run(value); });
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.onClose = () => { execution.cancel(); process.stdin.destroy(); };
