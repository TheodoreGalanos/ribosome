// Installed-consumer demonstration. Donor actions are authored examples;
// discovery and recipient decisions use the ordinary live Pi worker.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync, realpathSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { providerEnvironment } from '@ribosome/agents';

const json = value => JSON.stringify(value, null, 2) + '\n';
const contract = { inputs: [{ name: 'input', kind: 'artifact_path', required: true }, { name: 'output', kind: 'artifact_path', required: true }], outputs: ['Recipient output or explicit unresolved state'], entry_obligations: ['Read current recipient input'], exit_obligations: ['Preserve independent fields and report unsupported inputs'], limitations: ['Requires independent recipient evaluation'], discovery_refs: [] };

export async function prepare(directory) {
  directory = resolve(directory); await mkdir(directory); await mkdir(join(directory, 'state'));
  const scope = { client: 'qualification', project: 'installed-learning' };
  const events = [], windows = [];
  const donors = [
    { rows: [{ id: 'a', value: 2 }, { id: 'a', value: 2 }], result: { status: 'resolved', rows: [{ id: 'a', value: 2 }] }, global: 'completed' },
    { rows: [{ id: 'b', value: 3 }, { id: 'b', value: 4 }], result: { status: 'unresolved' }, global: 'completed' },
    { rows: [{ id: 'c', value: 8 }], result: { status: 'resolved', rows: [{ id: 'c', value: 8 }] }, global: 'completed' },
    { rows: [{ id: 'd', value: 6 }, { id: 'd', value: 6 }], result: { status: 'resolved', rows: [{ id: 'd', value: 6 }] }, global: 'failed' },
  ];
  for (const [index, donor] of donors.entries()) {
    const execution = `source-${index + 1}`, folder = join(directory, execution); await mkdir(folder);
    const refs = [];
    const emit = (kind, payload) => {
      const id = `${execution}:${refs.length + 1}`;
      events.push({ id, scope, run_id: execution, producer: 'source', sequence: String(refs.length + 1), kind, timestamp_ms: String(Date.now()), parents: refs.slice(-1), correlation: execution, artifacts: [], payload,
        provenance: { origin: 'observed', source_refs: [], scenario_family: 'authored-source-reconciliation', split: 'development', limitations: ['Authored donor decisions; actual file operations and checks. Not autonomous source reasoning.'] } }); refs.push(id);
    };
    await writeFile(join(folder, 'input.json'), json(donor.rows));
    emit('tool_result', { tool: 'read_rows', content: JSON.parse(await readFile(join(folder, 'input.json'), 'utf8')) });
    if (index === 2) {
      await writeFile(join(folder, 'output.json'), json(donor.result));
      emit('tool_result', { tool: 'read_existing_result', content: JSON.parse(await readFile(join(folder, 'output.json'), 'utf8')) });
    } else {
      await writeFile(join(folder, 'output.json'), json(donor.result));
      emit('tool_result', { tool: 'write_result', content: JSON.parse(await readFile(join(folder, 'output.json'), 'utf8')) });
    }
    const actual = JSON.parse(await readFile(join(folder, 'output.json'), 'utf8'));
    emit('tool_result', { tool: 'check_result', passed: JSON.stringify(actual) === JSON.stringify(donor.result), observed: actual });
    emit('execution_result', { disposition: donor.global, ...(donor.global === 'failed' ? { reason: 'A subsequent unrelated export destination was unavailable; checked local result remains present.' } : {}) });
    windows.push({ execution, event_refs: refs, frontier: { [`${execution}/source`]: String(refs.length) } });
  }
  const corpus = { id: 'installed-source', version: '1', visibility: 'retrospective', source_windows: windows, definition_refs: [], artifacts: [], dependencies: [], limitations: ['Four authored source episodes. Empty starting definition inventory.'] };
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath, worker: fileURLToPath(import.meta.resolve('@ribosome/agents/worker')), tools: {}, corpora: [corpus],
    grant: { id: 'installed-learning-owner', scope, mode: 'sandbox', paths: [], tools: [], profiles: ['curator', 'experimenter'], budget: { max_calls: 160, max_tokens: '10000000', max_cost_microusd: '1000000', max_actions: 80, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 1200000) }, context: 'row-reconciliation', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'discover', provider: 'openai', model: 'configured-at-execution', profile: 'curator', operator: 'discovery@1', discovery_corpus: { id: corpus.id, version: corpus.version }, prompt: 'Investigate the four assigned source episodes. The definition inventory is empty. Infer a reusable conditional behavior from observed inputs, actions and checks; compare the benign episode and the locally checked result within a globally failed execution. Save one functional definition, grounded occurrences and an investigation, or an evidence-backed no_motif conclusion. Do not invent missing observations or claim causal benefit. No effects or child work. Aim to finish within 24 calls; the shared allowance must also cover extraction and recipient tests.' } };
  const task = 'Reconcile input rows by id. Equivalent duplicate rows may collapse to one; conflicting values for an id require unresolved status, without choosing a value. Preserve independent_cost. Output {status:"resolved",rows:[...],independent_cost:250}, or {status:"unresolved",independent_cost:250}. Do not change an already correct result. Use current recipient files and a sandbox branch if edits are needed.';
  const cases = [
    { id: 'recipient-supported', family: 'recipient-reconciliation', split: 'development', input: { subject: { prompt: task, files: { 'input.json': json([{ id: 'x', value: 9 }, { id: 'x', value: 9 }, { id: 'y', value: 4 }]), 'output.json': json({ status: 'pending', independent_cost: 250 }) }, bindings: { input: 'input.json', output: 'output.json' } }, oracle: { expected: { status: 'resolved', rows: [{ id: 'x', value: 9 }, { id: 'y', value: 4 }], independent_cost: 250 }, benign: false } } },
    { id: 'recipient-conflict', family: 'recipient-reconciliation', split: 'development', input: { subject: { prompt: task, files: { 'input.json': json([{ id: 'z', value: 7 }, { id: 'z', value: 8 }]), 'output.json': json({ status: 'unresolved', independent_cost: 250 }) }, bindings: { input: 'input.json', output: 'output.json' } }, oracle: { expected: { status: 'unresolved', independent_cost: 250 }, benign: true } } },
  ];
  await writeFile(join(directory, 'events.json'), json(events));
  await writeFile(join(directory, 'recipients.json'), json(cases));
  await writeFile(join(directory, 'config.json'), json(config));
  await writeFile(join(directory, 'report.json'), json({ status: 'prepared', model_calls: 0, planned_evaluations: 4, limitations: ['Related authored development cases, not independent protected transfer qualification.'] }));
  return { episodes: 4, events: events.length, cases: 2, repetitions: 1, planned_evaluations: 4, maximum_calls: 160, maximum_cost_microusd: '1000000' };
}

export async function judge(input) {
  const branch = input.branches.find(b => b.id === input.selected_branch);
  let actual; try { actual = JSON.parse(await readFile(join(branch?.path ?? input.workspace, 'output.json'), 'utf8')); } catch { actual = null; }
  const expected = input.task.case_input.oracle.expected;
  const sameRows = expected.status === 'unresolved' || (Array.isArray(actual?.rows) && actual.rows.length === expected.rows.length && expected.rows.every(row => actual.rows.some(r => r.id === row.id && r.value === row.value)));
  const changes = input.executions.flatMap(e => e.effects ?? []).filter(e => e.status === 'succeeded' && ['edit', 'apply', 'execute'].includes(e.action.kind)).length;
  const preserved = actual?.independent_cost === 250;
  const passed = actual?.status === expected.status && sameRows && preserved && (!input.task.case_input.oracle.benign || changes === 0);
  return { passed, measurements: [{ name: 'quality', value: Number(passed), unit: 'fraction' }, { name: 'preserved_work', value: Number(preserved), unit: 'fraction' }], checks: ['recipient-output', 'independent-work'], output: json({ passed, preserved, changes }), descriptor: expected.status === 'unresolved' ? 'honest-unresolved' : 'supported-reconciliation' };
}

export async function run(directory) {
  directory = resolve(directory);
  const file = join(directory, 'config.json'), config = JSON.parse(await readFile(file, 'utf8'));
  if (JSON.parse(await readFile(join(directory, 'report.json'), 'utf8')).status !== 'prepared') throw Error('This trial already started. Retain its outcome; do not silently renew its budget.');
  config.request.provider = process.env.RIBOSOME_PROVIDER ?? 'openai'; config.request.model = process.env.RIBOSOME_MODEL;
  if (!config.request.model) throw Error('Configure RIBOSOME_MODEL before a live trial.');
  const environment = providerEnvironment(config.request.provider);
  const executable = process.env.RIBOSOME_CLI ?? resolve('target/debug/ribosome');
  const database = join(config.state_dir, 'ribosome.db');
  const report = { status: 'running', qualification: 'incomplete', planned_evaluations: 4, attempts: [], limitations: ['Authored donor episodes; live discovery and recipient decisions.', 'Two related development recipients, not broad independent transfer.', 'A saved candidate or completed model disposition is not verified task success.'] };
  const execute = async (command, args = [], configured = true) => {
    await writeFile(file, json(config));
    const result = spawnSync(executable, [command, ...(configured ? [file] : []), ...args], { env: { PATH: process.env.PATH, ...environment }, encoding: 'utf8', timeout: 1200000, maxBuffer: 8 * 1024 * 1024 });
    await writeFile(join(directory, `private-${report.attempts.length}.json`), json({ code: result.status, stdout: result.stdout, stderr: result.stderr, error: result.error?.message }));
    report.attempts.push({ command, run_id: config.request.run_id, exit_code: result.status });
    if (result.error) throw result.error;
    let value; try { value = JSON.parse(result.stdout); } catch { throw Error('Host output unavailable; see private diagnostics.'); }
    if (result.status !== 0 && !(command === 'run' && value.disposition)) throw Error('Host command failed; see private diagnostics.');
    return value;
  };
  const records = kind => { const db = new DatabaseSync(database, { readOnly: true }); try { return db.prepare('SELECT body FROM records WHERE kind=? ORDER BY id').all(kind).map(r => JSON.parse(r.body)); } finally { db.close(); } };
  await writeFile(join(directory, 'report.json'), json(report));
  try {
    await execute('ingest', [database, join(directory, 'events.json')], false);
    report.discovery = await execute('run');
    const definitions = records('definition').filter(r => r.body.functional_contract), occurrences = records('occurrence').filter(r => r.body.grounding), investigations = records('discovery');
    report.discovery_records = { definitions: definitions.map(r => r.id), occurrences: occurrences.map(r => r.id), investigations: investigations.map(r => r.id) };
    if (!definitions.length || !occurrences.length || !investigations.length) { report.status = 'discovery_incomplete'; return report; }
    config.request = { ...config.request, run_id: 'extract', operator: 'extraction@1', prompt: `Read the saved discovery records ${[...definitions, ...occurrences, ...investigations].map(r => r.id).join(', ')} and their supporting evidence. Produce one instructions implementation of the supported conditional behavior. Do not change the function to guarantee a result. Use body.instruction_contract with this interface: ${JSON.stringify(contract)}. Link actual discovered motif versions, include limitations and source evidence, and use input/output binding names. No effects. Save the implementation and finish within 16 calls.` };
    report.extraction = await execute('run');
    const candidate = records('implementation').find(r => r.body.format === 'instructions' && r.body.instruction_contract);
    if (!candidate) { report.status = 'extraction_incomplete'; return report; }
    report.candidate = { id: candidate.id, version: candidate.body.version };
    const provenance = { origin: 'synthetic', source_refs: [], scenario_family: 'host-control', split: 'development', limitations: ['Ordinary host-authored control, not discovered material.'] };
    await writeFile(join(directory, 'records.json'), json([{ kind: 'implementation', provenance, body: { name: 'ordinary-worker', version: '1', motifs: [], format: 'instructions', material: 'Perform the recipient task using current observations and permitted tools. Report unsupported input honestly.', parameters: {}, required_capabilities: [], state_assumptions: [], possible_effects: [], failure_behavior: 'Report unresolved state', evaluation_refs: [], instruction_contract: contract } }]));
    const [baseline] = await execute('records', [join(directory, 'records.json')]);
    config.cases = JSON.parse(await readFile(join(directory, 'recipients.json'), 'utf8')); config.corpora = [];
    config.agent_evaluators = { subject: { provider: config.request.provider, model: config.request.model, model_version: config.request.model, tool_versions: [], tools: {}, paths: ['input.json', 'output.json'], writable_paths: ['output.json'], judge: { program: process.execPath, args: [import.meta.filename, '--judge'], timeout_ms: 5000 } } };
    config.policies = [{ id: 'installed-function@1', context: config.grant.context, evaluator: 'subject', evaluator_version: '1', case_ids: config.cases.map(c => c.id), required_checks: ['recipient-output', 'independent-work'], metric: 'quality', min_quality: 1, min_improvement: 0, repetitions: 1, allowed_cells: ['supported-reconciliation', 'honest-unresolved'], retain_learning_memory: false, max_evaluations: 4, study_objective: 'function', case_budget: { ...config.grant.budget, max_calls: 20, max_actions: 10, max_cost_microusd: '150000' } }];
    config.request = { run_id: 'functional-study', profile: 'experimenter', operator: 'experiment@1', provider: config.request.provider, model: config.request.model, prompt: 'Execute the frozen host functional study.' };
    await writeFile(join(directory, 'records.json'), json([{ kind: 'experiment', provenance, body: { name: 'installed-functional-reuse', template: 'transfer', study_objective: 'function', candidate: report.candidate, baseline: { id: baseline.id, version: baseline.body.version }, variants: [], hypothesis: 'The discovered policy fulfills the recipient contract including unresolved input', case_ids: config.policies[0].case_ids, scenario_families: ['recipient-reconciliation'], feedback: 'aggregate', model_version: config.request.model, tool_versions: [], memory_start_refs: [], repetitions: 1, budget: config.grant.budget, metrics: ['quality'], policy_id: config.policies[0].id, selection_frozen: true } }]));
    const [study] = await execute('records', [join(directory, 'records.json')]);
    report.study = await execute('study', [study.id]); report.status = 'executed';
    report.qualification = report.study.complete && report.study.decision === 'accepted' ? 'local_function_passed' : 'not_established';
    return report;
  } catch { report.status = 'infrastructure_error'; return report; }
  finally {
    if (existsSync(database)) {
      const db = new DatabaseSync(database, { readOnly: true });
      try { report.usage = db.prepare("SELECT count(*) calls, coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) known_cost_microusd, coalesce(sum(CASE WHEN state!='settled' OR usage IS NULL OR coalesce(json_extract(usage,'$.complete'),0)<>1 THEN 1 ELSE 0 END),0) unknown_calls FROM permits WHERE state!='released'").get(); } finally { db.close(); }
    }
    await writeFile(join(directory, 'report.json'), json(report));
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (mode === '--judge') { let input = ''; for await (const chunk of process.stdin) input += chunk; console.log(json(await judge(JSON.parse(input)))); }
  else if (directory && mode === '--prepare') console.log(json(await prepare(directory)));
  else if (directory && mode === '--run') { const report = await run(directory); console.log(json({ status: report.status, qualification: report.qualification, usage: report.usage, study_complete: report.study?.complete })); if (report.status === 'infrastructure_error') process.exitCode = 1; }
  else throw Error('Use --prepare DIRECTORY or --run DIRECTORY. Live run: at most 160 calls and US$1 under one shared grant.');
}
