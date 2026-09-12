import { prepare, submit, extractionPrompt, json, hash, hostTool } from './prepare.mjs';
import { liveSession, providerEnvironment, metrics } from './session.mjs';
import { randomUUID } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';

export async function prepareInheritance(prepared, session) {
  const { config, configFile } = prepared;
  await session.run('curator', 'extraction@1', extractionPrompt);
  const candidates = await session.search('implementation');
  const candidate = candidates.find(r => r.body.format === 'registered_tool' && r.body.material === 'normalize-measurements');
  assert.ok(candidate, 'Curator must prepare the observed installed normalization procedure');
  assert.ok(candidate.provenance.source_refs.length, 'Curation must retain donor evidence');
  assert.ok((await session.search('occurrence')).length, 'Curation must identify a local motif occurrence');
  const [baseline] = await submit(configFile, [{
    kind: 'implementation',
    provenance: { origin: 'synthetic', source_refs: [], scenario_family: 'reference-control', split: 'development', limitations: ['Explicit host-supplied negative control'] },
    body: { name: 'sum-unconverted', version: '1', motifs: [], format: 'registered_tool', material: 'sum-unconverted', parameters: {}, required_capabilities: [], state_assumptions: ['Inputs already have a consistent length unit'], possible_effects: [], failure_behavior: 'Produces incorrect mixed-unit totals', evaluation_refs: [] },
  }]);
  await session.run('experimenter', 'experiment@1', `Evaluate candidate ${JSON.stringify({ id: candidate.id, version: candidate.body.version })} against baseline ${JSON.stringify({ id: baseline.id, version: "1" })} using policy units-policy@1 in context length-measurements. The policy fixes case IDs ["mixed-m-cm","mixed-km-mm"], 1 repetition, quality metric, checks dimensional-consistency and expected-total, evaluator units@1, aggregate feedback. Use scenario families ${JSON.stringify(config.cases.map(c => c.family))} and the transfer template. This is a two-arm comparison: baseline and candidate are built in, so variants is []. Use this budget: ${JSON.stringify(config.grant.budget)}. Freeze selection, run the experiment, submit a recommendation citing all evaluation references and request policy admission. Do not write evaluator observations yourself.`);
  assert.ok((await session.search('implementation', 'usable')).some(r => r.id === candidate.id), 'Candidate must receive contextual policy admission');
  return candidate;
}

export async function runDemo(scenario, directory) {
  providerEnvironment();
  const prepared = await prepare(directory, scenario);
  const { config, configFile } = prepared;
  const originalPrompt = config.request.prompt;
  const sourceBefore = await readFile(join(prepared.directory, 'source.json'), 'utf8');
  const session = liveSession(prepared);
  let maintenance;
  if (scenario === 'B' || scenario === 'C') {
    const candidate = await prepareInheritance(prepared, session);
    if (scenario === 'B') {
      maintenance = await session.run('caretaker', 'recombination@1', 'A later workflow needs a measurement total in metres. Retrieve a compatible implementation from the usable inventory, adapt bindings to source.json and report.json, execute the admitted registered procedure in a branch, and apply it with report-check. Submit a transplant record. Preserve independent_cost_analysis. Do not reread the donor transcript.');
      assert.ok((await session.search('transplant')).some(r => r.body.donor.id === candidate.id), 'Reuse must identify the admitted donor');
      assert.ok(!maintenance.observed.checkpoint.tool_calls.some(c => c.name === 'evidence_read'), 'Prepared reuse must not reread donor events');
    } else {
      await session.run('curator', 'memory@1', `Inspect the report workflow evidence and retain scoped failure memory about stale validation after source revision. Include supporting event references, a benign counterexample, response guidance and uncertainty. The evaluated recovery procedure is ${JSON.stringify({ id: candidate.id, version: candidate.body.version })}; reference its admission accurately. Do not invent a general law from one episode.`);
      assert.ok((await session.search('memory')).length, 'Curator must retain scoped knowledge');
      const other = { ...config, grant: { ...config.grant, id: randomUUID(), scope: { client: 'unrelated-client', project: 'measurement-report' } } };
      const otherFile = join(prepared.directory, 'unrelated-client.json');
      await writeFile(otherFile, json(other));
      assert.equal((await session.search('memory', 'evidence', otherFile)).length, 0, 'Unrelated client must not retrieve memory');
      maintenance = await session.run('caretaker', 'regeneration@1', `${originalPrompt} Consult scoped memory and the usable procedure inventory. Record which properties regained support and what remains unresolved.`);
      assert.ok((await session.search('obligation')).length, 'Regeneration must record property support');
    }
  } else {
    maintenance = await session.run('caretaker', 'proofreading@1', config.request.prompt);
  }
  const reportText = await readFile(join(prepared.directory, 'report.json'), 'utf8');
  const report = JSON.parse(reportText);
  assert.equal(report.total_m, 3, 'Report must reflect current mixed-unit source');
  assert.equal(report.independent_cost_analysis, 250, 'Independent work must remain intact');
  assert.equal(await readFile(join(prepared.directory, 'source.json'), 'utf8'), sourceBefore, 'Source inputs must remain intact');
  const effects = maintenance.observed.effects ?? [];
  assert.ok(effects.some(e => e.action.kind === 'check' && e.status === 'succeeded' && e.after.some(a => a.path === 'report.json' && a.version === hash(reportText))), 'Completion requires a check receipt covering the final report version');
  assert.ok(effects.some(e => e.action.kind === 'apply' && e.status === 'succeeded'), 'Repair must pass checked branch application');
  const independentCheck = hostTool(prepared.directory, 'check');
  const result = {
    scenario, kind: 'live-model', provider: config.request.provider, model: config.request.model, passed: true,
    metrics: { preparation_and_laboratory: metrics(session.reports.slice(0, -1)), maintenance: metrics([maintenance]), amortization: 'No amortization assumed; preparation/laboratory costs are reported separately.' },
    independent_check: independentCheck, reports: session.reports,
  };
  await writeFile(join(prepared.directory, 'live-report.json'), json(result));
  return result;
}

if (process.argv[1] === import.meta.filename) {
  const scenario = process.argv[2] ?? 'A';
  if (!['A', 'B', 'C'].includes(scenario)) throw Error('Choose demonstration A, B or C');
  const directory = resolve(process.argv[3] ?? `.ribosome/demo-${scenario}-${Date.now()}`);
  await mkdir(directory, { recursive: true });
  const result = await runDemo(scenario, directory);
  console.log(json({ scenario: result.scenario, passed: result.passed, report: join(directory, 'live-report.json') }));
}
