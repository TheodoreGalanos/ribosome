import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

export const root = resolve(import.meta.dirname, '../..');
export const hash = text => 'sha256:' + createHash('sha256').update(text).digest('hex');
export const json = value => JSON.stringify(value, null, 2) + '\n';

export function runCli(args, environment = {}) {
  const result = spawnSync(join(root, 'target/debug/ribosome'), args, {
    encoding: 'utf8', env: { PATH: process.env.PATH, ...environment }, maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) throw Error(`ribosome ${args[0]} exited ${result.status}: ${result.stderr}\n${result.stdout}`);
  return JSON.parse(result.stdout);
}

export function hostTool(directory, command) {
  const result = spawnSync(process.execPath, [join(root, 'examples/local-project/tool.mjs'), command], {
    cwd: directory, encoding: 'utf8', env: {},
  });
  if (result.status !== 0) throw Error(result.stderr);
  return result.stdout;
}

export function runCliAsync(args, environment = {}) {
  return new Promise((resolveResult, reject) => {
    const child = spawn(join(root, 'target/debug/ribosome'), args, { env: { PATH: process.env.PATH, ...environment }, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = { stdout: [], stderr: [] };
    let size = 0;
    for (const stream of ['stdout', 'stderr']) child[stream].on('data', chunk => {
      size += chunk.length;
      if (size > 16 * 1024 * 1024) { child.kill('SIGTERM'); reject(Error('CLI output exceeded 16 MiB')); }
      else chunks[stream].push(chunk);
    });
    child.on('error', reject);
    child.on('close', code => {
      const stdout = Buffer.concat(chunks.stdout).toString('utf8');
      const stderr = Buffer.concat(chunks.stderr).toString('utf8');
      if (code !== 0) return reject(Error(`ribosome ${args[0]} exited ${code}: ${stderr}\n${stdout}`));
      try { resolveResult(JSON.parse(stdout)); } catch (error) { reject(error); }
    });
  });
}

export async function prepare(directory, scenario = 'A', { variant = 'relevant-edit' } = {}) {
  if (!['A', 'B', 'C'].includes(scenario)) throw Error('Choose demonstration A, B or C');
  directory = resolve(directory);
  await mkdir(directory, { recursive: true });
  const state = join(directory, '.ribosome');
  await mkdir(state, { recursive: true });
  const initialSource = json({ measurements: [{ value: 1, unit: 'm' }, { value: scenario === 'C' ? 100 : 200, unit: 'cm' }] });
  const currentSource = json({ measurements: [{ value: 1, unit: 'm' }, { value: 200, unit: 'cm' }] });
  await writeFile(join(directory, 'source.json'), initialSource);
  await writeFile(join(directory, 'report.json'), json({ total_m: 201, independent_cost_analysis: 250 }));
  const normalizeOutput = hostTool(directory, 'normalize');
  const checkedReport = await readFile(join(directory, 'report.json'), 'utf8');
  const checkOutput = hostTool(directory, 'check');
  assert.equal(JSON.parse(checkedReport).independent_cost_analysis, 250);

  let currentReport = checkedReport;
  if (scenario === 'C') await writeFile(join(directory, 'source.json'), currentSource);
  else if (variant === 'relevant-edit') {
    currentReport = json({ total_m: 201, independent_cost_analysis: 250 });
    await writeFile(join(directory, 'report.json'), currentReport);
  } else if (variant === 'benign-edit') {
    await writeFile(join(directory, 'notes.txt'), 'Planning note edited after report validation.\n');
  } else if (variant !== 'incomplete') throw Error(`Unknown preparation variant: ${variant}`);

  const config = {
    workspace: directory, state_dir: state, node: process.execPath, worker: join(root, 'packages/agents/dist/worker.js'),
    grant: {
      id: randomUUID(), scope: { client: 'reference-client', project: 'measurement-report' }, mode: 'apply',
      paths: ['source.json', 'report.json'], writable_paths: ['report.json'],
      tools: ['report-check', 'normalize-measurements'], required_checks: ['report-check'], profiles: ['caretaker', 'curator', 'experimenter'],
      budget: { max_calls: 60, max_tokens: '6000000', max_cost_microusd: '1000000', max_actions: 30, max_work_items: 4, max_depth: 2, deadline_ms: String(Date.now() + 600000) },
      context: 'length-measurements', visible_splits: ['development'], allow_export: true,
    },
    request: {
      run_id: randomUUID(), profile: scenario === 'B' ? 'curator' : 'caretaker',
      operator: scenario === 'B' ? 'extraction@1' : scenario === 'C' ? 'regeneration@1' : 'proofreading@1',
      prompt: scenario === 'B' ? extractionPrompt : 'Inspect the worker and planner evidence. Restore current report validity if justified, preserve independent_cost_analysis, and report fresh evidence. Source inputs are read-only. Direct branch edits and report-check are available. The registered normalizer can execute only through an admitted implementation; without one, derive any justified repair from the source and report artifacts. The host requires a checked branch before application.',
      provider: process.env.RIBOSOME_PROVIDER ?? 'openai', model: process.env.RIBOSOME_MODEL ?? 'your-model-id',
    },
    tools: {
      'report-check': { program: process.execPath, args: [join(root, 'examples/local-project/tool.mjs'), 'check'], timeout_ms: 5000, reads: ['source.json', 'report.json'], validates: ['report.json'] },
      'normalize-measurements': { program: process.execPath, args: [join(root, 'examples/local-project/tool.mjs'), 'normalize'], timeout_ms: 5000, reads: ['source.json'], writes: ['report.json'] },
    },
    evaluators: { units: { program: process.execPath, args: [join(root, 'examples/local-project/evaluator.mjs')], timeout_ms: 5000 } },
    cases: [
      { id: 'mixed-m-cm', family: 'length-conversion-m-cm', split: 'holdout', input: { measurements: [{ value: 2, unit: 'm' }, { value: 300, unit: 'cm' }], expected_m: 5 } },
      { id: 'mixed-km-mm', family: 'length-conversion-km-mm', split: 'holdout', input: { measurements: [{ value: 0.001, unit: 'km' }, { value: 500, unit: 'mm' }], expected_m: 1.5 } },
    ],
    policies: [{ id: 'units-policy@1', context: 'length-measurements', evaluator: 'units', evaluator_version: '1', case_ids: ['mixed-m-cm', 'mixed-km-mm'], required_checks: ['dimensional-consistency', 'expected-total'], metric: 'quality', min_quality: 1, min_improvement: 0.5, repetitions: 1, allowed_cells: ['mixed-length-units'], retain_learning_memory: false, max_evaluations: 4 }],
  };
  if (variant === 'benign-edit') config.grant.paths.push('notes.txt');
  const configFile = join(directory, 'ribosome.json');
  await writeFile(configFile, json(config));
  const provenance = { origin: 'observed', source_refs: [], scenario_family: 'reference-handoff', split: 'development', limitations: ['Independent local reference project; this is not production operational evidence.'] };
  const events = [];
  const event = (producer, kind, payload, artifacts = [], parents = []) => {
    const value = { id: randomUUID(), scope: config.grant.scope, run_id: 'source-workflow', producer, sequence: String(events.filter(e => e.producer === producer).length + 1), kind, timestamp_ms: String(Date.now()), parents, correlation: 'measurement-report', artifacts, payload, provenance };
    events.push(value);
    return value;
  };
  const start = event('worker-measurements', 'task_start', { task: 'Prepare a measurement total' });
  if (variant === 'incomplete') {
    event('worker-measurements', 'progress', { status: 'working', claim: 'Validation has started; no completion evidence is available in this window.' }, [], [start.id]);
  } else {
    const normalization = event('worker-measurements', 'tool_result', {
      tool: 'normalize-measurements', input: JSON.parse(initialSource), output: JSON.parse(checkedReport), stdout: normalizeOutput,
      procedure: 'Convert length measurements to metres, then sum. m=1, cm=0.01, mm=0.001, km=1000. Reject unknown units. Preserve independent report fields.',
    }, [{ path: 'source.json', version: hash(initialSource) }, { path: 'report.json', version: hash(checkedReport) }], [start.id]);
    const checked = event('worker-measurements', 'check_completion', { status: 'succeeded', tool: 'report-check', total_m: JSON.parse(checkedReport).total_m, output: checkOutput }, normalization.artifacts, [normalization.id]);
    const costs = event('worker-costs', 'check_completion', { status: 'succeeded', check: 'host assertion independent_cost_analysis === 250', independent_cost_analysis: 250 }, [{ path: 'report.json', version: hash(checkedReport) }]);
    let changed;
    if (scenario === 'C') changed = event('planner', 'artifact_change', { reason: 'Source revision changed the measurement input; the prior report remains unmodified', preserved_report_version: hash(checkedReport), desired_properties: ['Current total in metres', 'Independent cost analysis retained'] }, [{ path: 'source.json', version: hash(currentSource) }], [checked.id]);
    else if (variant === 'relevant-edit') changed = event('worker-measurements', 'artifact_change', { note: 'Edited measurement total after validation' }, [{ path: 'report.json', version: hash(currentReport) }, { path: 'source.json', version: hash(currentSource) }], [checked.id]);
    else changed = event('planner', 'artifact_change', { note: 'Edited planning notes only' }, [{ path: 'notes.txt', version: hash(await readFile(join(directory, 'notes.txt'), 'utf8')) }], [checked.id]);
    event('planner', 'handoff', { claim: 'Report ready; earlier check passed', required_check: 'report-check' }, [{ path: 'report.json', version: hash(currentReport) }, { path: 'source.json', version: hash(currentSource) }], [changed.id, costs.id]);
    if (scenario === 'B') event('worker-measurements', 'task_completion', { status: 'failed', unrelated_failure: 'Injected host publication failure after normalization succeeded', injected: true, registered_tool: 'normalize-measurements' }, [], [normalization.id]);
  }
  const eventFile = join(directory, 'events.json');
  await writeFile(eventFile, json(events));
  runCli(['ingest', join(state, 'ribosome.db'), eventFile]);
  return { directory, configFile, config, events };
}

export const extractionPrompt = 'Inspect the observed normalization procedure, including useful local work if its enclosing execution failed. Extract it as a candidate implementation. The host offers normalize-measurements, which reads source.json and updates report.json using metre conversions. Retain provenance and motif occurrence evidence; do not admit it yourself.';

export async function submit(configFile, records) {
  const file = join(dirname(resolve(configFile)), 'record-input.json');
  await writeFile(file, json(records));
  return runCli(['records', configFile, file]);
}

if (process.argv[1] === import.meta.filename) {
  const prepared = await prepare(process.argv[2] ?? '.ribosome/reference', process.argv[3] ?? 'A');
  console.log(prepared.configFile);
}
