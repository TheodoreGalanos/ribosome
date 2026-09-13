import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { createCorpus } from './r4-corpus.mjs';
import { root, json, runCli } from '../../examples/local-project/prepare.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';

export async function prepareInvocation(directory) {
  directory = resolve(directory); await mkdir(directory); await mkdir(join(directory, 'state'));
  const scope = { client: 'qualification', project: 'prepared-invocation' };
  const donors = join(directory, 'donors'); await mkdir(donors);
  const { corpus } = await createCorpus(donors, scope);
  corpus.source_windows = corpus.source_windows.filter(window => ['episode-01', 'episode-04'].includes(window.execution));
  await writeFile(join(directory, 'recipient.json'), json({ a: { value: 2, unit: 'm', period: 'current' }, b: { value: 300, unit: 'cm', period: 'current' } }));
  await writeFile(join(directory, 'result.json'), json({ status: 'pending' }));
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath,
    worker: join(root, 'packages/agents/dist/worker.js'), corpora: [corpus],
    tools: { 'recipient-check': { program: process.execPath, args: [import.meta.filename, '--check'], timeout_ms: 5000, reads: ['recipient.json', 'result.json'], validates: ['result.json'] } },
    grant: { id: 'invocation-owner', scope, mode: 'sandbox', paths: ['recipient.json', 'result.json'], writable_paths: ['result.json'], tools: ['recipient-check'], profiles: ['curator', 'caretaker'],
      budget: { max_calls: 36, max_tokens: '2000000', max_cost_microusd: '350000', max_actions: 15, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 1200000) },
      context: 'prepared-development', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'curator', profile: 'curator', operator: 'extraction@1', provider: 'openai', model: 'configured-at-execution', discovery_corpus: { id: corpus.id, version: corpus.version },
      prompt: 'Read the assigned donor events and prepare one conditional instructions implementation of the supported quantity-combination behaviour. Preserve the unresolved metadata case as a condition for abstention; do not invent missing conversion evidence. Save a definition if needed and link the implementation to it. The host will execute the saved material in different recipients. Include instruction_contract with input slots named input (artifact_path, required), output (artifact_path, required), and checker (tool, required); outputs, entry_obligations, exit_obligations, limitations and discovery_refs. Bindings are supplied later. The input JSON has a and b quantities; the intended output shape is {status:"resolved", total:number, unit:string, period:string}, or an explicit unresolved outcome when inputs are insufficient. These are interface requirements, not observed recipient results. Describe a conditional policy using fresh reads, a sandbox branch, justified edits and the bound checker. The checker is independently registered and cannot repair the output. Use only instructions format, required_capabilities=["recipient-check"], empty evaluation_refs and no admission claim. Save the record using record_submit and copy the returned ID. Budget for this preparation is shared with three subsequent recipients; aim to finish within twelve calls. Do not run recipient effects or request child work.' } };
  await writeFile(join(directory, 'config.json'), json(config));
  return { donor_episodes: corpus.source_windows.length, maximum_calls: 36, maximum_cost_microusd: '350000' };
}

async function execute(config, directory, environment) {
  const file = join(directory, 'config.json'); await writeFile(file, json(config));
  const child = spawn(join(root, 'target/debug/ribosome'), ['run', file], { env: { PATH: process.env.PATH, ...environment }, stdio: ['ignore', 'pipe', 'pipe'] });
  const chunks = { stdout: [], stderr: [] }; let size = 0;
  for (const stream of ['stdout', 'stderr']) child[stream].on('data', data => { size += data.length; if (size > 16 * 1024 * 1024) child.kill('SIGTERM'); else chunks[stream].push(data); });
  const exitCode = await new Promise((resolveExit, reject) => { child.on('error', reject); child.on('close', resolveExit); });
  const stdout = Buffer.concat(chunks.stdout).toString('utf8'), stderr = Buffer.concat(chunks.stderr).toString('utf8');
  await writeFile(join(directory, `${config.request.run_id}-private-output.json`), json({ exitCode, stdout, stderr }));
  let result; try { result = JSON.parse(stdout); } catch { result = { disposition: 'host_error' }; }
  return result;
}

export async function runInvocation(directory) {
  directory = resolve(directory); const config = JSON.parse(await readFile(join(directory, 'config.json'), 'utf8'));
  const reportFile = join(directory, 'report.json');
  config.request.provider = process.env.RIBOSOME_PROVIDER ?? 'openai'; config.request.model = process.env.RIBOSOME_MODEL;
  if (!config.request.model) throw Error('Configure the local model before running the evaluation.');
  config.grant.budget.deadline_ms = String(Date.now() + 1200000);
  const environment = providerEnvironment(config.request.provider);
  runCli(['ingest', join(config.state_dir, 'ribosome.db'), join(directory, 'donors/events.json')]);
  const report = { status: 'running', runs: [], limitations: ['Authored donor policies and recipient cases; no held-out benefit claim.'] };
  await writeFile(reportFile, json(report));
  report.runs.push({ run_id: 'curator', result: await execute(config, directory, environment) });
  const db = new DatabaseSync(join(config.state_dir, 'ribosome.db'), { readOnly: true });
  let candidate;
  try { candidate = db.prepare("SELECT body FROM records WHERE kind='implementation' AND json_extract(body,'$.body.format')='instructions' AND json_type(body,'$.body.instruction_contract')='object' ORDER BY id DESC LIMIT 1").get(); }
  finally { db.close(); }
  if (candidate) {
    candidate = JSON.parse(candidate.body); report.implementation = { id: candidate.id, version: candidate.body.version };
    for (const [name, a, b] of [
      ['conversion', { value: 2, unit: 'm', period: 'current' }, { value: 300, unit: 'cm', period: 'current' }],
      ['compatible', { value: 4, unit: 'm', period: 'current' }, { value: 5, unit: 'm', period: 'current' }],
      ['unsupported', { value: 4, unit: 'm', period: 'current' }, { value: 5, unit: 'm' }],
    ]) {
      await writeFile(join(directory, 'recipient.json'), json({ a, b })); await writeFile(join(directory, 'result.json'), json({ status: 'pending' }));
      config.request = { run_id: name, profile: 'caretaker', operator: 'execute-motif@1', provider: config.request.provider, model: config.request.model,
        prompt: 'Execute the host-selected implementation for this recipient. Inspect fresh input and establish compatibility. Keep any edits in a sandbox branch and check the output using the bound checker. Report the resulting evidence or abstain with the missing inputs.',
        invocation: { implementation: report.implementation, bindings: { input: 'recipient.json', output: 'result.json', checker: 'recipient-check' }, recipient_refs: [], purpose: 'experimental' } };
      report.runs.push({ run_id: name, result: await execute(config, directory, environment) });
      await writeFile(reportFile, json(report));
    }
  }
  const observed = new DatabaseSync(join(config.state_dir, 'ribosome.db'), { readOnly: true });
  try {
    report.usage = observed.prepare("SELECT count(*) calls,coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) cost_microusd,sum(CASE WHEN usage IS NULL OR json_extract(usage,'$.complete')<>1 THEN 1 ELSE 0 END) incomplete FROM permits WHERE state <> 'released'").get();
    report.effects = observed.prepare('SELECT run_id,body FROM effects ORDER BY rowid').all().map(row => ({ run_id: row.run_id, receipt: JSON.parse(row.body) }));
    report.saved_records = observed.prepare("SELECT id,kind FROM records ORDER BY id").all();
  } finally { observed.close(); }
  report.status = 'executed'; report.assessment = 'Requires inspection of actual receipts, outputs and unresolved conditions; completed is a model-reported disposition.';
  await writeFile(reportFile, json(report));
  return { status: report.status, candidate_saved: !!candidate, runs: report.runs.map(run => ({ run_id: run.run_id, disposition: run.result.disposition })), usage: report.usage };
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (mode === '--check') {
    const { a, b } = JSON.parse(await readFile('recipient.json', 'utf8')); const result = JSON.parse(await readFile('result.json', 'utf8'));
    const scale = { m: 1, cm: 0.01 }; const supported = a.period && a.period === b.period && scale[a.unit] && scale[b.unit];
    const expected = supported ? a.value * scale[a.unit] + b.value * scale[b.unit] : undefined;
    const passed = supported ? result.status === 'resolved' && result.unit === 'm' && result.period === a.period && Math.abs(result.total - expected) < 1e-9 : result.status === 'unresolved';
    console.log(json({ passed, supported: !!supported })); if (!passed) process.exitCode = 1;
  } else {
    if (!directory || !['--prepare', '--run'].includes(mode)) throw Error('Use --prepare DIRECTORY or --run DIRECTORY. Preparation needs no credentials.');
    console.log(json(await (mode === '--prepare' ? prepareInvocation(directory) : runInvocation(directory))));
  }
}
