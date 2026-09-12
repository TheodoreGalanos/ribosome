import test from 'node:test';
import assert from 'node:assert/strict';
import { Ajv } from 'ajv';
import { createTools } from '../../packages/agents/dist/tools/index.js';

test('record tool parameters expose the matching operator body contracts', () => {
  const tools = createTools({}, 'run', 'proofreading@1', ['finding', 'obligation'], () => {});
  const parameters = tools.find(tool => tool.name === 'record_submit').parameters;
  const accepts = new Ajv({ strict: false }).compile(parameters);
  const submission = {
    kind: 'finding', provenance: { origin: 'observed', source_refs: [], scenario_family: 'fixture', split: 'development', limitations: [] },
    body: { subject: 'report', observation: 'check failed', interpretation: 'repair needed', evidence_refs: [], uncertainty: [], operator: 'proofreading@1' },
  };
  assert.equal(accepts(submission), true);
  assert.equal(accepts({ ...submission, body: { summary: 'plausible but unusable' } }), false);
  assert.equal(accepts({ ...submission, kind: 'definition' }), false);
  assert.equal(accepts({ ...submission, kind: 'obligation' }), false);
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
