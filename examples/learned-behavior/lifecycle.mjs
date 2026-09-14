import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync, realpathSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { providerEnvironment } from '@ribosome/agents';
import { judge } from './demo.mjs';

const json = value => JSON.stringify(value, null, 2) + '\n';
const read = async path => JSON.parse(await readFile(path, 'utf8'));
const save = (path, value) => writeFile(path, json(value));
const ref = record => ({ id: record.id, version: record.body.version });
const provenance = { origin: 'synthetic', source_refs: [], scenario_family: 'row-reconciliation-lifecycle', split: 'development', limitations: ['Authored local cases; results describe these cases only.'] };
const bindings = { input: 'input.json', output: 'output.json', checker: 'row-check' };
const task = 'Reconcile rows by id. Collapse equal values; conflicting values require unresolved status. Preserve independent_cost from the current output. Use a branch, write output.json as {status:"resolved",rows:[{id,value}],independent_cost}, or {status:"unresolved",independent_cost}, then run the bound checker. Leave an already correct output in place.';
const instructionContract = {
  inputs: [{ name: 'input', kind: 'artifact_path', required: true }, { name: 'output', kind: 'artifact_path', required: true }, { name: 'checker', kind: 'tool', required: true }],
  outputs: ['Reconciled rows or unresolved status'], entry_obligations: ['Read current rows and independent_cost'],
  exit_obligations: ['Check the result and preserve independent_cost'], limitations: ['Tested with id and numeric value fields'], discovery_refs: [],
};
const implementation = (name, material) => ({ kind: 'implementation', provenance, body: {
  name, version: '1', motifs: [], format: 'instructions', material, parameters: {}, required_capabilities: ['row-check'],
  state_assumptions: ['Rows contain id and value'], possible_effects: ['Update the bound output'], failure_behavior: 'Report a failed check', evaluation_refs: [], instruction_contract: instructionContract,
} });
const exampleCase = (id, rows, expected) => ({ id, family: 'row-reconciliation', split: 'development', input: {
  subject: { prompt: task, files: { 'input.json': json(rows), 'output.json': json({ status: 'pending', independent_cost: 250 }) }, bindings },
  oracle: { expected, benign: false },
} });

export async function prepareLifecycle(directory) {
  directory = resolve(directory); await mkdir(directory); await mkdir(join(directory, 'state'));
  const cases = [
    exampleCase('equal-values', [{ id: 'a', value: 2 }, { id: 'a', value: 2 }], { status: 'resolved', rows: [{ id: 'a', value: 2 }], independent_cost: 250 }),
    exampleCase('conflicting-values', [{ id: 'b', value: 3 }, { id: 'b', value: 4 }], { status: 'unresolved', independent_cost: 250 }),
  ];
  const budget = { max_calls: 400, max_tokens: '12000000', max_cost_microusd: '3000000', max_actions: 120, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 3600000) };
  const tools = { 'row-check': { program: process.execPath, args: [import.meta.filename, '--check'], timeout_ms: 5000, reads: ['input.json', 'output.json'], validates: ['output.json'] } };
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: fileURLToPath(import.meta.resolve('@ribosome/agents/worker')), tools, cases,
    grant: { id: 'lifecycle-owner', scope: { client: 'example', project: 'row-reconciliation' }, mode: 'sandbox', paths: ['input.json', 'output.json'], writable_paths: ['output.json'], tools: ['row-check'], profiles: ['caretaker', 'curator', 'experimenter'], budget, context: 'row-reconciliation', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'initial-study', profile: 'experimenter', operator: 'experiment@1', provider: process.env.RIBOSOME_PROVIDER ?? 'openai', model: process.env.RIBOSOME_MODEL ?? 'configured-at-execution', prompt: 'Run the local function study.' },
    policies: [{ id: 'rows@1', context: 'row-reconciliation', evaluator: 'subject', evaluator_version: '1', case_ids: cases.map(c => c.id), required_checks: ['recipient-output', 'independent-work'], metric: 'quality', min_quality: 1, min_improvement: 0, repetitions: 1, allowed_cells: ['supported-reconciliation', 'honest-unresolved'], retain_learning_memory: false, max_evaluations: 4, study_objective: 'function', case_budget: { ...budget, max_calls: 20, max_actions: 8, max_cost_microusd: '150000' } }],
  };
  await save(join(directory, 'config.json'), config);
  await save(join(directory, 'report.json'), { status: 'prepared', planned_evaluations: 8, limitations: ['An authored faulty instruction starts this example. Autonomous discovery is a separate experiment.'] });
  return { planned_evaluations: 8, maximum_calls: budget.max_calls, maximum_cost_microusd: budget.max_cost_microusd };
}

// The checker reads recipient files and reports correctness. It cannot repair them.
export async function checkRows(directory) {
  const rows = await read(join(directory, 'input.json'));
  const actual = await read(join(directory, 'output.json')).catch(() => null);
  const values = new Map(); let unresolved = false;
  for (const row of rows) {
    if (Object.keys(row).some(key => !['id', 'value'].includes(key)) || (values.has(row.id) && values.get(row.id) !== row.value)) unresolved = true;
    values.set(row.id, row.value);
  }
  const expected = unresolved ? { status: 'unresolved', independent_cost: 250 } : { status: 'resolved', rows: [...values].map(([id, value]) => ({ id, value })), independent_cost: 250 };
  const result = await judge({ workspace: directory, branches: [], executions: [], task: { case_input: { oracle: { expected, benign: false } } } });
  return { ...result, actual };
}

export async function runLifecycle(directory, options = {}) {
  directory = resolve(directory);
  const configFile = join(directory, 'config.json'), reportFile = join(directory, 'report.json');
  const config = await read(configFile), report = await read(reportFile);
  if (report.status !== 'prepared') throw Error('This trial already started. Inspect its saved report and configuration before continuing it.');
  const environment = options.environment ?? providerEnvironment(config.request.provider);
  if (config.request.model === 'configured-at-execution') throw Error('Set RIBOSOME_MODEL before preparation or set request.model in config.json.');
  const executable = process.env.RIBOSOME_CLI ?? resolve('target/debug/ribosome');
  const database = join(config.state_dir, 'ribosome.db');
  config.agent_evaluators = { subject: { provider: config.request.provider, model: config.request.model, model_version: config.request.model, tool_versions: [], tools: config.tools, paths: config.grant.paths, writable_paths: config.grant.writable_paths,
    judge: { program: process.execPath, args: [fileURLToPath(new URL('./demo.mjs', import.meta.url)), '--judge'], timeout_ms: 5000 } } };
  report.status = 'running'; report.stages = [];
  let commandNumber = 0;
  const command = async (name, args = [], configured = true) => {
    await save(configFile, config);
    const result = spawnSync(executable, [name, ...(configured ? [configFile] : []), ...args], { env: { PATH: process.env.PATH, ...environment }, encoding: 'utf8', timeout: 3600000, maxBuffer: 16 * 1024 * 1024 });
    await save(join(directory, `private-${commandNumber++}-${name}.json`), { code: result.status, stdout: result.stdout, stderr: result.stderr, error: result.error?.message });
    if (result.error) throw result.error;
    let value; try { value = JSON.parse(result.stdout); } catch { throw Error(`${name} did not return JSON; inspect private diagnostics.`); }
    if (result.status !== 0 && !(name === 'run' && value.disposition)) throw Error(`${name} failed; inspect private diagnostics.`);
    return value;
  };
  const search = async (kind, inventory = 'evidence') => {
    const file = join(directory, 'query.json'); await save(file, { query: '', kind, inventory, limit: 100, offset: 0 });
    return (await command('search', [file])).records;
  };
  const submit = async records => { const file = join(directory, 'records.json'); await save(file, records); return command('records', [file]); };
  const stage = async (id, profile, operator, prompt, assignment = {}, invocation) => {
    config.request = { run_id: id, profile, operator, provider: config.request.provider, model: config.request.model, prompt: `${prompt}\n\nAssignment:\n${json(assignment)}`, ...(invocation ? { invocation } : {}) };
    const result = await command('run');
    const observed = await command('inspect', [database, id], false);
    report.stages.push({ id, result, effects: observed.effects, model_usage: observed.model_usage });
    await save(reportFile, report);
    return observed;
  };
  const study = async (name, candidate, baseline) => {
    config.request = { run_id: name, profile: 'experimenter', operator: 'experiment@1', provider: config.request.provider, model: config.request.model, prompt: 'Execute the owner-configured function study.' };
    const [experiment] = await submit([{ kind: 'experiment', provenance, body: { name, template: 'transfer', study_objective: 'function', candidate: ref(candidate), baseline: ref(baseline), variants: [], hypothesis: 'The instruction reconciles supported rows and leaves conflicting evidence unresolved.', case_ids: config.cases.map(c => c.id), scenario_families: ['row-reconciliation'], feedback: 'aggregate', model_version: config.request.model, tool_versions: [], memory_start_refs: [], repetitions: 1, budget: config.grant.budget, metrics: ['quality'], policy_id: 'rows@1', selection_frozen: true } }]);
    const result = await command('study', [experiment.id]);
    report.stages.push({ id: name, experiment: experiment.id });
    return result;
  };
  const recipient = async (id, rows, candidate, prompt, recipientBindings) => {
    await save(join(directory, 'input.json'), rows); await save(join(directory, 'output.json'), { status: 'pending', independent_cost: 250 });
    const observed = await stage(id, 'caretaker', 'execute-motif@1', prompt, {}, { implementation: ref(candidate), bindings: recipientBindings, recipient_refs: [], purpose: 'production' });
    const branch = [...observed.effects].reverse().filter(effect => effect.status === 'succeeded')
      .map(effect => effect.action.branch_id ?? (effect.action.kind === 'branch' ? effect.output : undefined)).find(Boolean);
    if (!branch) return { passed: false, limitation: 'No recipient branch was produced.', disposition: report.stages.at(-1).result.disposition };
    const branchPath = join(config.state_dir, 'branches', branch);
    const checked = await checkRows(branchPath);
    return { ...checked, branch: branchPath, fresh_check: observed.effects.some(effect => effect.action.kind === 'check' && effect.action.branch_id === branch && effect.status === 'succeeded' && effect.evidence_ref), evidence_refs: observed.effects.filter(effect => effect.evidence_ref).map(effect => effect.evidence_ref) };
  };
  await save(reportFile, report);
  try {
    const [baseline, faulty] = await submit([
      implementation('ordinary-worker', 'Complete the recipient task using current evidence and the permitted tools.'),
      implementation('first-value-reconciliation', 'Read input and output. For each id, keep the first value and discard later rows, even if their values differ. Preserve independent_cost. Write a resolved output in a branch and run checker.'),
    ]);
    report.faulty = ref(faulty); report.initial_study = await study('initial-study', faulty, baseline);
    if (!report.initial_study.complete || !report.initial_study.usage_complete) { report.status = 'initial_study_incomplete'; return report; }
    if (report.initial_study.decision === 'accepted') { report.status = 'fault_not_reproduced'; return report; }
    const before = new Set((await search('implementation')).map(record => record.id));
    await stage('revise', 'experimenter', 'experiment@1', 'Inspect the faulty instruction and its completed development evaluation records. Save one revised instructions implementation that fixes the demonstrated decision error, keeping the input/output/checker interface and independent work. Spell out the conditional procedure. Retain source references. Do not change checks, author evaluation results or request a new study; the host will test the candidate. Use a new record ID. Finish within 20 calls.', { candidate: ref(faulty), evaluation_refs: report.initial_study.evaluation_refs, instruction_contract: instructionContract });
    const revisions = (await search('implementation')).filter(record => !before.has(record.id) && record.body.format === 'instructions' && record.body.instruction_contract);
    if (revisions.length !== 1) { report.status = 'revision_incomplete'; return report; }
    const revised = revisions[0]; report.revised = ref(revised);
    report.revised_study = await study('revised-study', revised, baseline);
    if (!report.revised_study.complete || !report.revised_study.usage_complete) { report.status = 'revised_study_incomplete'; return report; }
    if (report.revised_study.decision !== 'accepted') { report.status = 'revision_not_accepted'; return report; }
    await stage('admit', 'experimenter', 'experiment@1', 'Read the candidate and every supplied evaluation. Submit a recommendation for these local row cases, with their limitations, and request policy admission through inventory_admission_request. Finish within 16 calls.', { candidate: ref(revised), evaluation_refs: report.revised_study.evaluation_refs, context: config.grant.context });
    if (!(await search('implementation', 'usable')).some(record => record.id === revised.id)) { report.status = 'admission_incomplete'; return report; }
    await stage('remember', 'curator', 'memory@1', 'Read this implementation and save one procedural memory for a later agent: what it does, where it was tested, its applicability and conditions for reconsideration. Include the implementation ID in provenance.source_refs so withdrawal reaches the memory. Finish within 8 calls.', { candidate: ref(revised) });
    report.memories = (await search('memory')).filter(record => record.provenance.source_refs.includes(revised.id)).map(record => record.id);
    if (!report.memories.length) { report.status = 'memory_incomplete'; return report; }
    await save(join(directory, 'input.json'), [{ id: 'later', value: 9 }, { id: 'later', value: 9 }]);
    await save(join(directory, 'output.json'), { status: 'pending', independent_cost: 250 });
    await stage('select', 'caretaker', 'recombination@1', 'A later agent needs to reconcile the current rows. Search scoped memory and the usable inventory, inspect the recipient, and save an advisory transplant with a compatible donor and bindings. The host will invoke your selected instructions next. Do not execute effects or read donor history. Finish within 10 calls.', { bindings, recipient: 'later-rows' });
    const transplant = (await search('transplant')).find(record => record.body.recipient === 'later-rows');
    if (transplant?.body.donor.id !== revised.id || transplant.body.donor.version !== revised.body.version || Object.entries(bindings).some(([name, value]) => transplant.body.bindings[name] !== value)) { report.status = 'retrieval_incomplete'; return report; }
    report.transplant = transplant.id;
    report.reuse = await recipient('reuse', [{ id: 'later', value: 9 }, { id: 'later', value: 9 }], revised, `${task} Execute the selected admitted instruction.`, transplant.body.bindings);
    if (!report.reuse.passed || !report.reuse.fresh_check) { report.status = 'reuse_not_verified'; return report; }
    report.challenge = await recipient('challenge', [{ id: 'distance', value: 1, unit: 'm' }, { id: 'distance', value: 1, unit: 'cm' }], revised, 'Reconcile the current rows using the selected instruction if compatible. These rows carry units. Equal numbers with different units must remain unresolved; do not drop units and claim equivalence. Preserve independent_cost and check any result.', transplant.body.bindings);
    if (report.challenge.passed || report.challenge.disposition === 'abstained') { report.status = 'challenge_handled'; return report; }
    if (!report.challenge.evidence_refs?.length) { report.status = 'challenge_incomplete'; return report; }
    const [finding] = await submit([{ kind: 'finding', provenance: { ...provenance, origin: 'observed', source_refs: report.challenge.evidence_refs }, body: { subject: 'row-reconciliation', observation: 'Independent recipient check rejected the selected instruction on rows with different units.', interpretation: 'Its demonstrated applicability does not cover these recipient rows.', evidence_refs: report.challenge.evidence_refs, uncertainty: ['A revised instruction needs new evaluation.'], operator: 'proofreading@1' } }]);
    await stage('withdraw', 'curator', 'memory@1', 'Read the counterexample finding and current implementation. Retire the implementation with record_retire (delete=false) because it is being selected for rows beyond its tested scope. Inspect whether its source-linked procedural memory is still available. Finish within 8 calls. This withdraws local reuse while preserving the experiment records for review.', { candidate: ref(revised), finding: finding.id });
    report.available_after_withdrawal = (await search('implementation', 'usable')).some(record => record.id === revised.id);
    report.memories_after_withdrawal = (await search('memory')).filter(record => report.memories.includes(record.id)).map(record => record.id);
    report.status = !report.available_after_withdrawal && !report.memories_after_withdrawal.length ? 'withdrawn_after_counterexample' : 'withdrawal_incomplete';
    return report;
  } catch (error) { report.status = 'infrastructure_error'; report.error = error.message; return report; }
  finally {
    if (existsSync(database)) {
      const db = new DatabaseSync(database, { readOnly: true });
      try { report.usage = db.prepare("SELECT count(*) calls, coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) known_cost_microusd, coalesce(sum(CASE WHEN state!='settled' OR usage IS NULL OR coalesce(json_extract(usage,'$.complete'),0)<>1 THEN 1 ELSE 0 END),0) unknown_calls FROM permits WHERE state!='released'").get(); }
      finally { db.close(); }
    }
    await save(reportFile, report);
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (mode === '--check') { const result = await checkRows(process.cwd()); console.log(json(result)); if (!result.passed) process.exitCode = 1; }
  else if (directory && mode === '--prepare') console.log(json(await prepareLifecycle(directory)));
  else if (directory && mode === '--run') { const result = await runLifecycle(directory); console.log(json({ status: result.status, report: join(resolve(directory), 'report.json') })); if (result.status === 'infrastructure_error') process.exitCode = 1; }
  else throw Error('Use --prepare DIRECTORY or --run DIRECTORY. A live trial has a shared ceiling of 400 calls and US$3.');
}
