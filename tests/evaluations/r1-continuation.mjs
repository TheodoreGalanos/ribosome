import { mkdir, writeFile, readFile, stat } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { runCli, root, json } from '../../examples/local-project/prepare.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';
import { checkDelivery } from './r1-continuation-check.mjs';

const maximumCost = '2000000';
const marker = 'WITHDRAWN-R1-LIVE-BRIEF';

export async function prepareContinuation(directory) {
  directory = resolve(directory);
  // Refuse to overwrite an earlier qualification or its aggregate allowance.
  await mkdir(directory);
  const workspace = join(directory, 'workspace'), state = join(directory, 'state');
  await mkdir(workspace); await mkdir(state);
  const paths = [];
  for (let asset = 1; asset <= 18; asset++) {
    const name = `inspection-${String(asset).padStart(2, '0')}.json`;
    paths.push(name);
    const acceptedRows = [7 + asset % 5, 40 + asset % 7, 85 + asset % 9];
    const readings = Array.from({ length: 96 }, (_, row) => ({
      reading_id: `${asset}-${row}`, status: acceptedRows.includes(row) ? 'accepted' : row % 2 ? 'archived' : 'provisional',
      value: (asset + row) % 19 + 1, unit: asset === 8 && row === acceptedRows[2] ? null : 'kWh',
      note: row % 2 ? 'Inspection register retains the earlier observation for comparison. Its explicit acceptance status determines whether it contributes to delivery.' : 'Field observation awaits the recorded acceptance decision. Do not infer acceptance from the presence of a numerical reading or a unit.',
    }));
    await writeFile(join(workspace, name), json({ asset: `ASSET-${String(asset).padStart(2, '0')}`, readings }));
  }
  await writeFile(join(workspace, 'delivery.json'), '[]\n');
  const config = {
    workspace, state_dir: state, node: process.execPath, worker: join(root, 'tests/evaluations/r1-continuation-worker.mjs'),
    grant: { id: 'r1-live-owner', scope: { client: 'qualification', project: 'r1-continuation' }, mode: 'apply',
      paths: [...paths, 'delivery.json'], writable_paths: ['delivery.json'], tools: ['delivery-check'], required_checks: [], profiles: ['caretaker'],
      budget: { max_calls: 100, max_tokens: '6000000', max_cost_microusd: maximumCost, max_actions: 20, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 1800000) },
      context: 'inspection-delivery', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'r1-live-run', profile: 'caretaker', operator: 'proofreading@1', provider: 'openai', model: 'configured-at-execution', prompt: '' },
    tools: { 'delivery-check': { program: process.execPath, args: [join(root, 'tests/evaluations/r1-continuation-check.mjs')], timeout_ms: 5000, reads: [...paths, 'delivery.json'], validates: ['delivery.json'] } },
  };
  const configFile = join(directory, 'config.json'), recordsFile = join(directory, 'records.json');
  await writeFile(configFile, json(config));
  const provenance = { origin: 'synthetic', source_refs: [], scenario_family: 'inspection-delivery', split: 'development', limitations: ['Authored R1 qualification inputs; not evidence of production task performance.'] };
  await writeFile(recordsFile, json([
    { kind: 'obligation', provenance, body: { description: 'Deliver a JSON array ordered by asset ID. Each row must have asset, status, accepted_count and total_kwh. Include only readings explicitly marked accepted in the count and total. If any accepted reading has a missing or non-kWh unit, status is unresolved and total_kwh is null; otherwise status is ready. Include all 18 assets. Obtain a passed delivery-check receipt for the final live file.', owner: 'host', subject: 'delivery.json', state: 'open', affected_outputs: ['delivery.json'], created_ms: '1', consequence_boundary: 'Before delivery', evidence_refs: [] } },
    { kind: 'memory', provenance, body: { kind: 'episodic', content: `${marker}: This optional planning brief contains no acceptance rule. Use the independent obligation and inspection records.`, applicability: 'initial orientation', evidence_refs: [], counterexamples: [], responses: [], regression_cases: [], conflicts: [], supersedes: [] } },
  ]));
  config.request.prompt = `Prepare delivery.json from the eighteen inspection files. First read obligation __OBLIGATION_ID__ for the acceptance rules and optional planning brief __BRIEF_ID__. Inspect every reading in every file; accepted readings occur at different positions. The inspection files are about 26 KiB each; artifact_read with length 32768 can read a whole file. Use bounded artifact reads when needed. A planning brief can become unavailable during this task; it is not required to establish delivery. Preserve the independent obligation, inspect existing receipts after interruption and finish the actual output rather than merely describing a plan. Source inspection files are read-only. Use the registered delivery-check on the final live output.\nQualification settings: ${JSON.stringify({ directory, brief: "__BRIEF_ID__" })}`;
  await writeFile(configFile, json(config));
  await writeFile(join(directory, 'qualification.json'), json({ status: 'prepared', provider_calls: 0, maximum_model_cost_microusd: maximumCost, inputs: 18, scenario: 'Read acceptance rules, inspect a long register, compact, withdraw an optional source, interrupt, reconstruct context and finish independently checked delivery.' }));
  return { directory, configFile };
}

async function runProcess(configFile, environment, diagnosticsPath) {
  return new Promise((resolveResult, reject) => {
    const child = spawn(join(root, 'target/debug/ribosome'), ['run', configFile], { env: { PATH: process.env.PATH, ...environment }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout = (stdout + chunk).slice(-1048576); });
    child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-1048576); });
    child.on('error', reject);
    child.on('close', async code => {
      try {
        // Raw provider errors may include deployment settings. Keep bounded
        // diagnostic tails in the private case directory, outside the report.
        await writeFile(diagnosticsPath, json({ code, stdout_tail: stdout, stderr_tail: stderr }));
        resolveResult({ code, disposition: JSON.parse(stdout).disposition });
      } catch { reject(Error('Qualification host exited without a valid result; inspect its private state before retrying')); }
    });
  });
}

export async function runContinuation(directory, { retryRejectedRequest = false } = {}) {
  directory = resolve(directory);
  const configFile = join(directory, 'config.json');
  const previous = JSON.parse(await readFile(join(directory, 'qualification.json'), 'utf8'));
  if (previous.status !== 'prepared' && !retryRejectedRequest) throw Error('This qualification has already started; inspect its recorded result and private state before another trial');
  const config = JSON.parse(await readFile(configFile, 'utf8'));
  let priorAttempts = [];
  const provider = process.env.RIBOSOME_PROVIDER ?? 'openai', model = process.env.RIBOSOME_MODEL;
  if (!model?.trim()) throw Error('Configure RIBOSOME_MODEL before live qualification');
  const environment = providerEnvironment(provider);
  if (config.grant.budget.max_cost_microusd !== maximumCost) throw Error('Qualification requires the fixed shared $2 accounting limit');
  if (config.request.model === 'configured-at-execution') {
    config.request.provider = provider; config.request.model = model;
    config.grant.budget.deadline_ms = String(Date.now() + 1800000);
    await writeFile(configFile, json(config));
  } else if (provider !== config.request.provider || model !== config.request.model) {
    throw Error('Resume with the same configured provider and model; the root allowance is not reset');
  }
  if (retryRejectedRequest) {
    // A failed run is terminal. A rejected-request retry needs a new run identity,
    // but keeps the existing grant, deadline, database and unknown liabilities.
    if (previous.status !== 'executed' || previous.attempts?.length !== 1 || previous.attempts[0].disposition !== 'failed') throw Error('Only an inspected initial provider rejection can be retried');
    const diagnostics = JSON.parse(await readFile(join(directory, 'attempt-1-diagnostics.json'), 'utf8'));
    const summary = JSON.parse(diagnostics.stdout_tail).summary;
    if (typeof summary !== 'string' || (summary !== 'Connection error.' && !(summary.includes('API error (400)') && summary.includes('invalid_function_parameters')))) throw Error('The initial failure was not a connection or tool-schema rejection');
    const state = new DatabaseSync(join(config.state_dir, 'ribosome.db'), { readOnly: true });
    try {
      if (state.prepare("SELECT count(*) AS n FROM permits WHERE state='settled'").get().n || state.prepare('SELECT count(*) AS n FROM effects').get().n) throw Error('Provider-rejection retry requires no completed provider calls or effects');
      if (state.prepare('SELECT status FROM runs WHERE id=?').get(config.request.run_id)?.status !== 'failed') throw Error('Initial run is not terminal');
    } finally { state.close(); }
    priorAttempts = [...previous.prior_attempts ?? [], ...previous.attempts];
    await writeFile(join(directory, `rejected-${priorAttempts.length}-report.json`), json(previous));
    await writeFile(join(directory, `rejected-${priorAttempts.length}-diagnostics.json`), json(diagnostics));
    config.request.run_id += '-retry';
    await writeFile(configFile, json(config));
  }
  if (config.request.prompt.includes('__OBLIGATION_ID__')) {
    const [obligation, brief] = runCli(['records', configFile, join(directory, 'records.json')]);
    config.request.prompt = config.request.prompt.replaceAll('__OBLIGATION_ID__', obligation.id).replaceAll('__BRIEF_ID__', brief.id);
    await writeFile(configFile, json(config));
  }
  await writeFile(join(directory, 'qualification.json'), json({ status: 'running', prior_attempts: priorAttempts, maximum_model_cost_microusd: maximumCost, provider_calls: null }));
  const attempts = [];
  for (let attempt = 0; attempt < 2; attempt++) {
    try { attempts.push(await runProcess(configFile, environment, join(directory, `attempt-${attempt + 1}-diagnostics.json`))); }
    catch { attempts.push({ code: null, disposition: 'failed', error: 'Host exited without a valid result; private state retained' }); }
    if (attempts.at(-1).disposition !== 'interrupted') break;
  }
  let db;
  try {
    db = new DatabaseSync(join(config.state_dir, 'ribosome.db'), { readOnly: true });
    const lines = async name => {
      try { return (await readFile(join(directory, name), 'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse); }
      catch (error) { if (error.code === 'ENOENT') return []; throw error; }
    };
    const boundaries = await lines('provider-boundaries.jsonl');
    const checkpoints = await lines('checkpoints.jsonl');
    const reads = await lines('inspection-reads.jsonl');
    const inspected = await Promise.all(config.grant.paths.filter(path => path.startsWith('inspection-')).map(async path => {
      let through = 0;
      for (const window of reads.filter(read => read.path === path).sort((a, b) => a.offset - b.offset)) {
        if (window.offset > through) break;
        through = Math.max(through, window.offset + window.bytes);
      }
      return through >= (await stat(join(config.workspace, path))).size;
    }));
    const usage = db.prepare("SELECT count(*) AS calls,coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) AS observed_cost_microusd,sum(state NOT IN ('settled','released')) AS incomplete,coalesce(sum(CASE WHEN state NOT IN ('settled','released') THEN reserved_cost ELSE 0 END),0) AS unknown_reserved_cost_microusd FROM permits").get();
    const runIncomplete = db.prepare("SELECT count(*) AS n FROM permits WHERE run_id=? AND state NOT IN ('settled','released')").get(config.request.run_id).n;
    const compactions = db.prepare("SELECT count(*) AS n FROM permits WHERE compaction_id IS NOT NULL AND state='settled'").get().n;
    const retainedBytes = db.prepare("SELECT coalesce(sum(length(CAST(result_content AS BLOB))),0) AS n FROM artifact_snapshots WHERE result_run_id=?").get(config.request.run_id).n;
    const rebuilt = db.prepare('SELECT count(*) AS n FROM context_segments WHERE predecessor IS NOT NULL').get().n;
    const checks = db.prepare("SELECT count(*) AS n FROM effects WHERE json_extract(body,'$.action.kind')='check' AND json_extract(body,'$.status')='succeeded'").get().n;
    let delivery, deliveryPassed = false;
    try { delivery = checkDelivery(config.workspace); deliveryPassed = true; } catch { delivery = { error: 'delivery failed independent inspection checks' }; }
    const after = boundaries.filter(call => call.after_withdrawal);
    const conditions = {
      completed: attempts.at(-1).disposition === 'completed', interrupted_then_resumed: attempts.length === 2 && attempts[0].disposition === 'interrupted',
      compacted: compactions > 0, context_rebuilt: rebuilt > 0, long_evidence: retainedBytes > 180000,
      all_sources_inspected: inspected.every(Boolean),
      marker_observed_before_withdrawal: boundaries.some(call => !call.after_withdrawal && call.contains_withdrawn_marker),
      marker_absent_after_withdrawal: after.length > 0 && after.every(call => !call.contains_withdrawn_marker),
      bounded_checkpoints: checkpoints.length > 0 && checkpoints.every(item => item.bytes < 256 * 1024),
      independent_delivery_passed: deliveryPassed, successful_check_receipt: checks > 0, run_usage_complete: runIncomplete === 0,
    };
    const report = { status: 'executed', kind: 'live-model', passed: Object.values(conditions).every(Boolean), conditions, attempts, prior_attempts: priorAttempts, usage, compactions, retained_bytes: retainedBytes, rebuilt_segments: rebuilt, delivery, maximum_model_cost_microusd: maximumCost };
    await writeFile(join(directory, 'qualification.json'), json(report));
    return report;
  } catch (error) {
    const report = { status: 'incomplete', kind: 'live-model-qualification', passed: false, attempts, prior_attempts: priorAttempts, usage: null,
      maximum_model_cost_microusd: maximumCost, error: 'Evidence collection failed; inspect the private case diagnostics. Usage is unknown until recovered from the host state.' };
    await writeFile(join(directory, 'report-error.txt'), String(error.stack ?? error));
    await writeFile(join(directory, 'qualification.json'), json(report));
    return report;
  } finally { db?.close(); }
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (!['--prepare', '--run', '--retry-rejected'].includes(mode) || !directory) throw Error('Use --prepare DIRECTORY, --run DIRECTORY or --retry-rejected DIRECTORY. Execution makes paid provider calls under the shared $2 accounting limit.');
  const result = await (mode === '--prepare' ? prepareContinuation(directory) : runContinuation(directory, { retryRejectedRequest: mode === '--retry-rejected' }));
  console.log(json(result));
  if (mode !== '--prepare' && !result.passed) process.exitCode = 1;
}
