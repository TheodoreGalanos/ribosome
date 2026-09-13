import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { createCorpus } from './r4-corpus.mjs';
import { root, json, runCli } from '../../examples/local-project/prepare.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';

export async function prepareDiscovery(directory) {
  directory = resolve(directory);
  await mkdir(directory);
  await mkdir(join(directory, 'state'));
  const scope = { client: 'qualification', project: 'discovery' };
  const donors = join(directory, 'donors'); await mkdir(donors);
  const { episodes, corpus } = await createCorpus(donors, scope);
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath,
    worker: join(root, 'packages/agents/dist/worker.js'), tools: {}, corpora: [corpus],
    grant: { id: 'discovery-owner', scope, mode: 'observe', paths: [], tools: [], profiles: ['curator'],
      budget: { max_calls: 100, max_tokens: '3000000', max_cost_microusd: '1500000', max_actions: 0, max_work_items: 2, max_depth: 1, deadline_ms: String(Date.now() + 1200000) },
      context: 'discovery-development', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'discovery-parent', profile: 'curator', operator: 'discovery@1', provider: 'openai', model: 'configured-at-execution',
      discovery_corpus: { id: corpus.id, version: corpus.version },
      prompt: 'Investigate the assigned development executions for reusable behaviour. The starting definition inventory is empty. Retrieve evidence and compare different episodes, including unsuccessful or incomplete ones. Save grounded definitions and occurrences only where warranted; save the discovery investigation with alternatives and open questions even if no useful motif is supported. Use one bounded contrast-motif@1 child to challenge your interpretation, wait for it, and read its result before your final investigation. The child should inspect selected source evidence in a fresh context; its agreement is not proof. Do not merely describe what you would save. The donor policies and inputs were authored fixtures; their tool results were actually executed. Messages are claims, not tool receipts. Do not run effects or claim measured downstream benefit.' } };
  await writeFile(join(directory, 'config.json'), json(config));
  await writeFile(join(directory, 'report.json'), json({ status: 'prepared', episodes: episodes.length, model_calls: 0, maximum_cost_microusd: '1500000' }));
  return { episodes: episodes.length, events: corpus.source_windows.reduce((sum, window) => sum + window.event_refs.length, 0) };
}

export function inspectDiscovery(directory, status, disposition, runId) {
  const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
  try {
    const rows = db.prepare("SELECT id,kind,body FROM records WHERE json_extract(body,'$.retired')=0").all();
    const authors = new Map(db.prepare("SELECT result_run_id,result_content FROM artifact_snapshots WHERE result_method='record.submit' AND result_content IS NOT NULL").all().map(row => [JSON.parse(row.result_content).id, row.result_run_id]));
    const records = rows.map(row => ({ id: row.id, kind: row.kind, authored_by: authors.get(row.id), body: JSON.parse(row.body).body }));
    const usage = db.prepare("SELECT count(*) AS calls,coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) AS observed_cost_microusd,coalesce(sum(state NOT IN ('settled','released')),0) AS incomplete FROM permits").get();
    const children = db.prepare('SELECT id,status FROM work').all();
    const methods = db.prepare('SELECT result_method AS method,count(*) AS calls FROM artifact_snapshots WHERE result_method IS NOT NULL GROUP BY result_method').all();
    const maximumCost = db.prepare("SELECT json_extract(g.body,'$.budget.max_cost_microusd') AS value FROM grants g JOIN runs r ON r.grant_id=g.id WHERE r.id=?").get(runId)?.value;
    return { status, disposition, run_id: runId, maximum_cost_microusd: maximumCost, usage, children, methods, records,
      infrastructure: { effects: db.prepare('SELECT count(*) AS n FROM effects').get().n, all_calls_attributed: db.prepare('SELECT count(*) AS n FROM permits WHERE allocation_id IS NULL').get().n === 0,
        definitions: records.filter(r => r.kind === 'definition' && r.body.functional_contract).length, occurrences: records.filter(r => r.kind === 'occurrence' && r.body.grounding).length, investigations: records.filter(r => r.kind === 'discovery').length },
      semantic_assessment: 'pending independent review',
      limitations: ['Instrumented development donors, not autonomous source runs.', 'Saved records and completed disposition do not by themselves establish semantic correctness or downstream utility.'] };
  } finally { db.close(); }
}

export async function runDiscovery(directory, { retryAfterFix = false, complete = false } = {}) {
  directory = resolve(directory);
  const configFile = join(directory, 'config.json'), reportFile = join(directory, 'report.json');
  const previous = JSON.parse(await readFile(reportFile, 'utf8'));
  if (previous.status !== 'prepared' && !((retryAfterFix || complete) && previous.status === 'executed')) throw Error('This trial has already started; inspect its retained evidence and allowance');
  const config = JSON.parse(await readFile(configFile, 'utf8'));
  const provider = process.env.RIBOSOME_PROVIDER ?? 'openai';
  if (!process.env.RIBOSOME_MODEL?.trim()) throw Error('Configure RIBOSOME_MODEL');
  if (retryAfterFix || complete) {
    if (config.request.provider !== provider || config.request.model !== process.env.RIBOSOME_MODEL) throw Error('Keep the same provider and model for the correction trial');
    await writeFile(join(directory, `attempt-${previous.run_id ?? config.request.run_id}.json`), json(previous));
    const corpus = config.corpora[0];
    if (complete) {
      config.request.run_id = `discovery-completion-${corpus.version}-${previous.usage.calls}`;
      const candidates = previous.records.filter(record => record.kind === 'definition' && record.body.functional_contract).map(record => record.id);
      const reviews = previous.records.filter(record => record.kind === 'discovery').map(record => record.id);
      if (!candidates.length || !reviews.length) throw Error('Completion requires a saved structured candidate and completed contrast investigation');
      config.request.prompt = `Complete the prior discovery investigation under the existing corpus. Read the existing candidate records ${candidates.join(', ')} and completed contrast investigations ${reviews.join(', ')}. No further reviewer is needed. Investigate the other source windows, including episodes 03, 04, 07, 08, 11 and 12, to test applicability and alternative explanations. Reuse an existing definition when it fits; save a derived candidate only if its policy needs substantive revision. Save grounded occurrences where supported, and save the final discovery investigation including contradictions, inconclusive cases and open questions. A summary alone is not the requested result. Rust can supply omitted run_refs, frontiers, occurrence operator and annotator; select the evidence and author the interpretation. Do not infer that a source message or successful final output proves a check occurred. Do not claim causal benefit or transfer qualification.`;
    } else {
      corpus.version = String(Number(corpus.version) + 1);
      config.request.discovery_corpus.version = corpus.version;
      config.request.run_id = `discovery-parent-${corpus.version}`;
    }
  } else {
    config.request.provider = provider; config.request.model = process.env.RIBOSOME_MODEL;
    config.grant.budget.deadline_ms = String(Date.now() + 1200000);
  }
  const environment = providerEnvironment(config.request.provider);
  await writeFile(configFile, json(config));
  runCli(['ingest', join(config.state_dir, 'ribosome.db'), join(directory, 'donors/events.json')]);
  await writeFile(reportFile, json({ status: 'running', maximum_cost_microusd: config.grant.budget.max_cost_microusd }));
  const child = spawn(join(root, 'target/debug/ribosome'), ['run', configFile], { env: { PATH: process.env.PATH, ...environment }, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', chunk => stdout += chunk);
  child.stderr.on('data', chunk => stderr += chunk);
  const exitCode = await new Promise((resolveExit, reject) => { child.on('error', reject); child.on('close', resolveExit); });
  await writeFile(join(directory, 'private-process-output.json'), json({ exitCode, stdout, stderr }));
  let disposition; try { disposition = JSON.parse(stdout).disposition; } catch { disposition = 'host_error'; }
  const report = inspectDiscovery(directory, 'executed', disposition, config.request.run_id);
  await writeFile(reportFile, json(report));
  return { disposition, usage: report.usage, infrastructure: report.infrastructure, children: report.children, semantic_assessment: report.semantic_assessment };
}
if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (!directory || !['--prepare', '--run', '--retry-after-fix', '--complete'].includes(mode)) throw Error('Use --prepare DIRECTORY, --run DIRECTORY --retry-after-fix DIRECTORY or --complete DIRECTORY. A live trial has one 100-call, US$1.50 aggregate allowance.');
  console.log(json(await (mode === '--prepare' ? prepareDiscovery(directory) : runDiscovery(directory, { retryAfterFix: mode === '--retry-after-fix', complete: mode === '--complete' }))));
}
