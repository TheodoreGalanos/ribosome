import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { prepare, submit, json } from '../../examples/local-project/prepare.mjs';
import { liveSession, metrics } from '../../examples/local-project/session.mjs';
import { totalMetres } from '../../examples/local-project/normalize.mjs';

// Explicitly selected because the original twelve-case suite retains its
// twelve-dollar maximum. This additional study has its own one-dollar root.
export async function runDevelopment(directory) {
  const prepared = await prepare(directory, 'B');
  const { config, configFile } = prepared;
  config.grant.mode = 'sandbox';
  config.policies.push({ ...config.policies[0], id: 'generated-units@1', case_ids: [], allow_generated_development_cases: true });
  await writeFile(configFile, json(config));
  const source = prepared.events.find(event => event.kind === 'tool_result');
  const [candidate, baseline] = await submit(configFile, ['normalize-measurements', 'sum-unconverted'].map((tool, index) => ({
    kind: 'implementation', provenance: { origin: index ? 'synthetic' : 'observed', source_refs: index ? [] : [source.id], scenario_family: 'length-conversion', split: 'development', limitations: ['Host-supplied material for evaluating agent scenario generation'] },
    body: { name: tool, version: '1', motifs: [], format: 'registered_tool', material: tool, parameters: {}, required_capabilities: [], state_assumptions: ['numeric length measurements'], possible_effects: [], failure_behavior: 'Reject unsupported units', evaluation_refs: [] },
  })));
  const before = await readFile(join(directory, 'report.json'), 'utf8');
  const session = liveSession(prepared);
  await session.run('experimenter', 'experiment@1', `Read the observed normalization mechanism in workflow evidence, then generate two distinct development cases using different mixed length units. Each case input has measurements [{value,unit}] and expected_m. Propose candidate_checks and retain synthetic development provenance linking the observed source mechanism. Both cases derive from one mechanism, so do not claim independent transfer. Submit a stress experiment for candidate ${JSON.stringify({ id: candidate.id, version: "1" })} and baseline ${JSON.stringify({ id: baseline.id, version: "1" })} under development policy generated-units@1, context length-measurements, evaluator units@1, one repetition, metric quality, owner checks dimensional-consistency and expected-total. The policy allows two authored cases and four evaluations. Baseline and candidate are built in; variants is []. Use this budget: ${JSON.stringify(config.grant.budget)}. Check the expected_m arithmetic, then freeze and execute the comparison. Inspect the evaluation records. If a generated expectation is wrong, correct it in a new frozen study with explicit lineage and retain the failed study. Summarize the final evidence. Development results do not admit the candidate.`);
  const experiments = await session.search('experiment');
  const evaluations = await session.search('evaluation');
  // Revisions require separate immutable studies. Keep every attempt, and
  // qualify a complete successful comparison rather than counting proposals.
  const study = experiments.findLast(experiment => {
    const measured = evaluations.filter(e => e.body.experiment_ref === experiment.id);
    return measured.length === 4 && measured.filter(e => e.body.arm === 'candidate').length === 2 && measured.filter(e => e.body.arm === 'candidate').every(e => e.body.passed === true);
  });
  assert.ok(study, 'A frozen study must execute both arms on two valid generated cases');
  assert.deepEqual(study.body.candidate, { id: candidate.id, version: '1' });
  assert.deepEqual(study.body.baseline, { id: baseline.id, version: '1' });
  assert.equal(study.body.development_cases?.length, 2);
  assert.equal(new Set(study.body.development_cases.map(c => JSON.stringify(c.input))).size, 2, 'Generated inputs must differ');
  for (const entry of study.body.development_cases) {
    assert.equal(entry.provenance.origin, 'synthetic');
    assert.equal(entry.provenance.split, 'development');
    assert.ok(entry.provenance.source_refs.includes(source.id), 'Retain the actual normalization source');
    assert.ok(entry.source_mechanism.length > 0 && entry.candidate_checks.length > 0);
    assert.equal(new Set(entry.input.measurements.map(m => m.unit)).size > 1, true);
    assert.ok(Math.abs(totalMetres(entry.input.measurements) - entry.input.expected_m) <= 1e-9, 'Independent check of generated expected output using the host evaluator tolerance');
  }
  const measured = evaluations.filter(e => e.body.experiment_ref === study.id);
  assert.equal(measured.length, 4, 'Both arms must execute on both generated cases');
  assert.ok(measured.filter(e => e.body.arm === 'baseline').every(e => e.body.passed === false), 'Mixed-unit cases must distinguish normalization from the unconverted control');
  assert.equal((await session.search('admission')).length, 0, 'Authored development cases cannot create admission');
  assert.equal(await readFile(join(directory, 'report.json'), 'utf8'), before);
  const result = { scenario: 'generated-development', kind: 'live-model', provider: config.request.provider, model: config.request.model, passed: true, metrics: { preparation_and_laboratory: metrics(session.reports), maintenance: metrics([]) }, experiments, evaluations, reports: session.reports, assessment_limit: 'Generated development cases are related examples, not independent transfer or protected acceptance evidence.' };
  await writeFile(join(directory, 'live-report.json'), json(result));
  return result;
}
