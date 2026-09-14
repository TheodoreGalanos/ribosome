import test from 'node:test';
import assert from 'node:assert/strict';
import { Ajv } from 'ajv';
import { createTools } from '../../packages/agents/dist/tools/index.js';
import { operators } from '../../packages/agents/dist/operators/index.js';

test('provider tools use object parameters without top-level union keywords', () => {
  for (const [name, operator] of Object.entries(operators)) {
    for (const tool of createTools({}, 'run', `${name}@1`, operator.outputKinds, () => {})) {
      assert.equal(tool.parameters.type, 'object', tool.name);
      for (const keyword of ['oneOf', 'anyOf', 'allOf', 'enum', 'const', 'not']) assert.ok(!(keyword in tool.parameters), `${tool.name} contains top-level ${keyword}`);
    }
  }
});

test('record tool exposes body contracts and checks the selected kind before dispatch', async () => {
  const calls = [];
  const tools = createTools({ call: async (...args) => { calls.push(args); return { content: '{}' }; } }, 'run', 'proofreading@1', ['finding', 'obligation'], () => {});
  const submit = tools.find(tool => tool.name === 'record_submit');
  const parameters = submit.parameters;
  const accepts = new Ajv({ strict: false }).compile(parameters);
  const submission = {
    kind: 'finding', provenance: { origin: 'observed', source_refs: [], scenario_family: 'fixture', split: 'development', limitations: [] },
    body: { subject: 'report', observation: 'check failed', interpretation: 'repair needed', evidence_refs: [], uncertainty: [], operator: 'proofreading@1' },
  };
  assert.equal(accepts(submission), true);
  assert.equal(accepts({ ...submission, body: { summary: 'plausible but unusable' } }), false);
  assert.equal(accepts({ ...submission, kind: 'definition' }), false);
  await assert.rejects(submit.execute('wrong-kind', { ...submission, kind: 'obligation' }), /Obligation: invalid value/);
  await assert.rejects(submit.execute('wrong-output', { ...submission, kind: 'definition' }), /not an operator output/);
  assert.equal(calls.length, 0);
  await submit.execute('valid', submission);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0][1].arguments, submission);
});

test('prepared reuse uses inventory and recipient tools without source history', () => {
  const names = operator => createTools({}, 'run', operator, ['transplant'], () => {}).map(tool => tool.name);
  const reuse = names('recombination@1');
  assert.ok(reuse.includes('search_query'));
  assert.ok(reuse.includes('record_read'));
  assert.ok(reuse.includes('artifact_read'));
  assert.ok(reuse.includes('action_execute'));
  assert.ok(!reuse.includes('evidence_read'));
  assert.ok(names('extraction@1').includes('evidence_read'));
});

test('discovery requires structured records and exposes the bounded contrast operator', async () => {
  const tools = createTools({}, 'curator', 'discovery@1', operators.discovery.outputKinds, () => {});
  assert.ok(!tools.some(tool => tool.name === 'record_submit'));
  assert.ok(tools.find(tool => tool.name === 'definition_submit').parameters.properties.body.required.includes('functional_contract'));
  assert.ok(tools.find(tool => tool.name === 'occurrence_submit').parameters.properties.body.required.includes('grounding'));
  assert.ok(!('anyOf' in tools.find(tool => tool.name === 'discovery_submit').parameters.properties.body));
  const work = tools.find(tool => tool.name === 'work_request');
  assert.deepEqual(work.parameters.properties.operator.enum, ['contrast-motif@1']);
  await assert.rejects(work.execute('recursive', { profile: 'curator', operator: 'discovery@1' }), /contrast-motif/);
  assert.ok(!createTools({}, 'reviewer', 'contrast-motif@1', ['discovery'], () => {}).some(tool => tool.name === 'work_request'));
});

test('discovery metadata can be omitted from model arguments and is left for Rust to supply', async () => {
  const { readFile } = await import('node:fs/promises');
  const fixture = JSON.parse(await readFile(new URL('../../crates/ribosome-core/tests/fixtures/motif-records.json', import.meta.url), 'utf8'));
  const calls = [];
  const tools = createTools({ call: async (...args) => { calls.push(args); return { content: '{}' }; } }, 'curator', 'discovery@1', operators.discovery.outputKinds, () => {});
  for (const kind of ['occurrence', 'discovery']) {
    const body = fixture[kind];
    if (kind === 'occurrence') { delete body.frontier; delete body.operator; delete body.grounding.annotator; }
    else { delete body.run_refs; for (const window of body.source_windows) delete window.frontier; }
    const tool = tools.find(tool => tool.name === `${kind}_submit`);
    if (kind === 'discovery') assert.equal('work_ref' in tool.parameters.properties.body.properties, false);
    const accepts = new Ajv({ strict: false }).compile(tool.parameters);
    const request = { body, provenance: { origin: 'synthetic', source_refs: [], scenario_family: 'test', split: 'development', limitations: [] } };
    assert.equal(accepts(request), true, JSON.stringify(accepts.errors));
    await tool.execute(kind, request);
    assert.equal(calls.at(-1)[1].method, 'record.submit');
    assert.deepEqual(calls.at(-1)[1].arguments, { ...request, kind }, 'validation placeholders must not be sent as host observations');
  }
});
