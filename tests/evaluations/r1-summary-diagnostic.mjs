import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { isDeepStrictEqual } from 'node:util';
import { root, json, runCliAsync } from '../../examples/local-project/prepare.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';

export const facts = {
  rules: 'Return one row per asset ordered by asset ID, with asset, status, accepted_count and total_kwh. Count only readings marked accepted. If any accepted reading lacks unit kWh, return status unresolved and total_kwh null, while retaining the accepted count. Otherwise return status ready and sum the accepted values. Archived and provisional readings do not contribute.',
  assets: [
    { asset: 'A', readings: [{ status: 'accepted', value: 4, unit: 'kWh' }, { status: 'archived', value: 900, unit: 'kWh' }, { status: 'accepted', value: 6, unit: 'kWh' }] },
    { asset: 'B', readings: [{ status: 'accepted', value: 12, unit: null }, { status: 'accepted', value: 8, unit: 'kWh' }] },
    { asset: 'C', readings: [{ status: 'accepted', value: 2.5, unit: 'kWh' }, { status: 'provisional', value: 100, unit: 'kWh' }, { status: 'accepted', value: 1.5, unit: 'kWh' }] },
  ],
};
const expected = [
  { asset: 'A', status: 'ready', accepted_count: 2, total_kwh: 10 },
  { asset: 'B', status: 'unresolved', accepted_count: 2, total_kwh: null },
  { asset: 'C', status: 'ready', accepted_count: 2, total_kwh: 4 },
];
export const checkAnswer = rows => isDeepStrictEqual(rows, expected);

export async function prepareDiagnostic(directory) {
  directory = resolve(directory);
  await mkdir(directory);
  await mkdir(join(directory, 'state'));
  await writeFile(join(directory, 'facts.json'), json(facts));
  const config = { workspace: directory, state_dir: join(directory, 'state'), node: process.execPath,
    worker: join(root, 'tests/evaluations/r1-summary-diagnostic-worker.mjs'), tools: {},
    grant: { id: 'summary-diagnostic-owner', scope: { client: 'qualification', project: 'summary-diagnostic' }, mode: 'observe', paths: ['facts.json'], tools: [], profiles: ['curator'],
      budget: { max_calls: 6, max_tokens: '1000000', max_cost_microusd: '250000', max_actions: 0, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 600000) },
      context: 'summary-diagnostic', visible_splits: ['development'], allow_export: false },
    request: { run_id: 'diagnostic', profile: 'curator', operator: 'memory@1', provider: 'openai', model: 'configured-at-execution', prompt: '' } };
  await writeFile(join(directory, 'config.json'), json(config));
  await writeFile(join(directory, 'report.json'), json({ status: 'prepared', model_calls: 0, maximum_cost_microusd: '250000' }));
}

export async function runDiagnostic(directory, { resume = false, repeatSummary = false } = {}) {
  directory = resolve(directory);
  const reportFile = join(directory, 'report.json'), configFile = join(directory, 'config.json');
  const previous = JSON.parse(await readFile(reportFile, 'utf8'));
  const canRepeatSummary = repeatSummary && previous.status === 'executed' && previous.attempts.find(a => a.mode === 'original-answer')?.answer_passed && previous.attempts.find(a => a.mode === 'summary-answer')?.answer_passed === false;
  if (previous.status !== 'prepared' && !canRepeatSummary && !(resume && previous.status === 'executed' && previous.attempts.at(-1)?.disposition === 'failed')) throw Error('Diagnostic already started; inspect the retained result');
  const config = JSON.parse(await readFile(configFile, 'utf8'));
  const provider = process.env.RIBOSOME_PROVIDER ?? 'openai';
  const environment = providerEnvironment(provider);
  if (!process.env.RIBOSOME_MODEL?.trim()) throw Error('Configure RIBOSOME_MODEL');
  if (previous.status === 'prepared') {
    config.request.provider = provider; config.request.model = process.env.RIBOSOME_MODEL;
    config.grant.budget.deadline_ms = String(Date.now() + 600000);
  } else {
    if (config.request.provider !== provider || config.request.model !== process.env.RIBOSOME_MODEL) throw Error('Resume with the same provider and model');
    await writeFile(join(directory, `prior-${previous.usage.calls}-calls-report.json`), json(previous));
  }
  await writeFile(reportFile, json({ status: 'running', maximum_cost_microusd: '250000' }));
  const attempts = repeatSummary ? previous.attempts.filter(a => a.mode === 'original-answer') : resume ? previous.attempts.filter(a => a.disposition === 'completed') : [];
  const suffix = repeatSummary ? `-after-${previous.usage.calls}-calls` : resume ? config.request.run_id.slice(`diagnostic-${previous.attempts.at(-1).mode}`.length) : '';
  let summaryId = attempts.find(a => a.mode === 'compact')?.summary_id;
  for (const mode of ['original-answer', 'compact', 'summary-answer']) {
    if (attempts.some(a => a.mode === mode)) continue;
    config.request.run_id = `diagnostic-${mode}${suffix}`;
    config.request.prompt = JSON.stringify({ mode, task: 'Produce the delivery rows required by the supplied inspection evidence. Preserve any supported unresolved result.', ...(summaryId ? { summary_id: summaryId } : {}) });
    await writeFile(configFile, json(config));
    try {
      const result = await runCliAsync(['run', configFile], environment);
      const output = JSON.parse(result.summary);
      attempts.push({ mode, disposition: result.disposition, ...(mode === 'compact' ? { summary_id: output.summary_id } : { rows: output.rows, answer_passed: checkAnswer(output.rows) }) });
      if (mode === 'compact') summaryId = output.summary_id;
    } catch (error) {
      await writeFile(join(directory, `${mode}-private-error.txt`), String(error.stack ?? error));
      attempts.push({ mode, disposition: 'failed', error: 'Inspect private diagnostic error; no successful outcome inferred.' });
      break;
    }
  }
  const db = new DatabaseSync(join(config.state_dir, 'ribosome.db'), { readOnly: true });
  try {
    const usage = db.prepare("SELECT count(*) AS calls,coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) AS observed_cost_microusd,sum(state NOT IN ('settled','released')) AS incomplete FROM permits").get();
    const infrastructure = { all_runs_completed: attempts.length === 3 && attempts.every(a => a.disposition === 'completed'),
      compaction_committed: db.prepare("SELECT count(*) AS n FROM context_summaries WHERE status='committed' AND segment_id IN (SELECT id FROM context_segments WHERE run_id=?)").get(`diagnostic-compact${suffix}`).n === 1,
      usage_complete: usage.incomplete === 0, effects: db.prepare('SELECT count(*) AS n FROM effects').get().n };
    const report = { status: 'executed', kind: 'controlled-live-content-diagnostic', attempts, infrastructure, usage, maximum_cost_microusd: '250000',
      limitation: 'One controlled input with harness-supplied evidence and one answer per arm. Separates infrastructure results from model accuracy; not a reliability estimate or full long-run qualification.' };
    await writeFile(reportFile, json(report));
    return report;
  } finally { db.close(); }
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const [mode, directory] = process.argv.slice(2);
  if (!directory || !['--prepare', '--run', '--resume', '--repeat-summary'].includes(mode)) throw Error('Use --prepare DIRECTORY, --run DIRECTORY, --resume DIRECTORY or --repeat-summary DIRECTORY. Execution makes paid calls under a shared US$0.25 cap.');
  if (mode === '--prepare') { await prepareDiagnostic(directory); console.log('Diagnostic prepared without model calls.'); }
  else console.log(json(await runDiagnostic(directory, { resume: mode === '--resume', repeatSummary: mode === '--repeat-summary' })));
}
