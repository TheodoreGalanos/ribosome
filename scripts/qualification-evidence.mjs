// Export explicit fields rather than recursively copying private reports.
import { existsSync } from 'node:fs';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
const number = value => typeof value === 'number' && Number.isFinite(value) ? value : null;
const flag = value => typeof value === 'boolean' ? value : null;
const choice = (value, allowed) => allowed.includes(value) ? value : 'unknown';
const identifier = value => typeof value === 'string' && /^[0-9a-f-]{36}$/.test(value) ? value : null;
const amount = value => typeof value === 'string' && /^\d+$/.test(value) ? value : number(value);

export function summarizeStudy(study) {
  const report = study?.report;
  return { decision: choice(study?.decision, ['accepted', 'rejected', 'inconclusive']), complete: flag(study?.complete), usage_complete: flag(study?.usage_complete), planned_evaluations: number(study?.planned_evaluations),
    cases: number(report?.cases), families: number(report?.families), repetitions: number(report?.repetitions),
    arms: (report?.arms ?? []).map(a => ({ arm: choice(a.arm, ['baseline', 'candidate', 'retry', 'critique', 'care']), planned: number(a.planned), observed: number(a.observed), verified_successes: number(a.verified_successes), verified_success_rate: Number.isInteger(a.planned) && a.planned > 0 && Number.isInteger(a.verified_successes) ? a.verified_successes / a.planned : null })),
    comparisons: (report?.comparisons ?? []).map(c => ({ treatment: choice(c.treatment, ['candidate', 'retry', 'critique', 'care']), control: choice(c.control, ['baseline', 'retry', 'critique', 'care']), paired_cases: number(c.paired_cases), mean_difference: number(c.mean_difference), interval_95: c.interval_95 ? { lower: number(c.interval_95.lower), upper: number(c.interval_95.upper) } : null })),
    online_usage: report?.online_usage ? { model_calls: number(report.online_usage.model_calls), tokens: amount(report.online_usage.tokens), cost_microusd: amount(report.online_usage.cost_microusd), actions: number(report.online_usage.actions) } : null,
    learning_cost: report?.learning_cost ? { cost_microusd: amount(report.learning_cost.cost_microusd), reuse_count: number(report.learning_cost.reuse_count), complete: flag(report.learning_cost.complete) } : null,
    uncertainty_method: 'Paired case percentile bootstrap, 2000 resamples, 95%; repetitions averaged within cases; fewer than two cases has no interval.',
  };
}
export function summarizeLearning(report) {
  return { status: choice(report?.status, ['prepared', 'running', 'discovery_incomplete', 'extraction_incomplete', 'executed', 'infrastructure_error']), qualification: choice(report?.qualification, ['incomplete', 'local_function_passed', 'not_established']),
    planned_evaluations: number(report?.planned_evaluations),
    candidate: report?.candidate ? { id: identifier(report.candidate.id), version: amount(report.candidate.version) } : null,
    discovery_records: Object.fromEntries(['definitions', 'occurrences', 'investigations'].map(k => [k, report?.discovery_records?.[k]?.map(identifier).filter(Boolean) ?? []])),
    usage: report?.usage ? { calls: number(report.usage.calls), known_cost_microusd: amount(report.usage.known_cost_microusd), unknown_calls: number(report.usage.unknown_calls) } : null,
    study: summarizeStudy(report?.study),
  };
}
async function optional(file) { try { return JSON.parse(await readFile(file, 'utf8')); } catch (e) { if (e.code === 'ENOENT') return null; throw e; } }
const git = args => { const r = spawnSync('git', args, { encoding: 'utf8' }); if (r.status !== 0) throw Error('Cannot identify source checkout'); return r.stdout.trim(); };
function allocations(database) {
  const db = new DatabaseSync(database, { readOnly: true });
  try {
    const rows = db.prepare('SELECT id,body FROM budget_allocations ORDER BY id').all();
    const labels = new Map(rows.map((r,i) => [r.id, `allocation-${i+1}`]));
    return rows.map(r => { const a = JSON.parse(r.body); return { id: labels.get(r.id), parent_id: a.parent_id ? labels.get(a.parent_id) ?? 'outside-selection' : null, purpose: choice(a.purpose, ['root','curator','caretaker','experimenter','experiment','experiment-arm','evaluation-case','evaluation-subject','agent-stage-0','agent-stage-1','compaction']), disposition: a.disposition === undefined ? 'open' : choice(a.disposition, ['completed','failed','cancelled','exhausted','interrupted','abstained']), budget: { max_calls: number(a.budget.max_calls), max_tokens: amount(a.budget.max_tokens), max_cost_microusd: amount(a.budget.max_cost_microusd), max_actions: number(a.budget.max_actions) } }; });
  } finally { db.close(); }
}
function receipts(database) {
  const db = new DatabaseSync(database, { readOnly: true });
  try { return db.prepare("SELECT body FROM effects WHERE json_extract(body,'$.status') IN ('succeeded','unknown','failed') ORDER BY id LIMIT 12").all().map(row => { const e = JSON.parse(row.body); return { kind: choice(e.action.kind, ['branch', 'edit', 'check', 'apply', 'execute']), status: choice(e.status, ['succeeded', 'unknown', 'failed']), outcome_basis: choice(e.outcome_basis, ['execution_established', 'current_postcondition_observed', 'unresolved']), restored_properties: e.restored_properties?.length ?? 0 }; }); } finally { db.close(); }
}
export async function bundle(root, output) {
  root = resolve(root); output = resolve(output);
  const installed = await optional(join(root, '.ribosome/r7-installed.json'));
  const learningDirectory = installed ? join(installed.consumer, 'learning-trial') : null;
  const learning = learningDirectory ? await optional(join(learningDirectory, 'report.json')) : null;
  const mechanical = await optional(join(root, '.ribosome/r7-mechanical/report.json'));
  const followup = await optional(join(root, '.ribosome/r7-mechanical-followup/report.json'));
  if (mechanical && followup) {
    mechanical.checks = [...new Map([...mechanical.checks, ...followup.checks].map(c => [c.id, c])).values()];
    mechanical.status = mechanical.checks.every(c => c.status === 'passed') ? 'passed' : 'failed';
  }
  const r6 = await optional(join(root, '.ribosome/r6-laboratory-qualification/development-report.json'));
  const packageLock = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
  const deps = JSON.parse(await readFile(join(root, 'packages/agents/package.json'), 'utf8')).dependencies;
  const receiptDatabase = learning?.study && learningDirectory ? join(learningDirectory, 'state/ribosome.db') : join(root, '.ribosome/r6-laboratory-qualification/state/ribosome.db');
  const result = { source: { commit: git(['rev-parse', 'HEAD']), working_tree_modified: git(['status', '--porcelain']).length > 0, scope: 'Current local working tree. Uncommitted changes are not identified by the commit alone.' },
    runtime: { node: process.version, rust_toolchain: (await readFile(join(root, 'rust-toolchain.toml'), 'utf8')).match(/channel = "([^"]+)"/)?.[1], package_lock_version: packageLock.lockfileVersion, dependencies: deps, rust_lock: 'Cargo.lock', npm_lock: 'package-lock.json', schema_version: 21, protocols: ['ribosome/1', 'ribosome-host/1'] },
    configuration: { provider_settings: 'Private local environment; deliberately omitted', operators: ['discovery@1', 'extraction@1', 'execute-motif@1', 'experiment@1'], recipient_tools: 'Granted artifact reads, branches and edits; offline protected judge', starting_definitions: 0, maximum_calls: 160, maximum_cost_microusd: '1000000', matrix: { cases: 2, repetitions: 1, arms: ['baseline', 'candidate'] } },
    partitions: [{ role: 'discovery', family: 'authored-source-reconciliation', split: 'development', episodes: 4, source: 'Actual file observations from authored donor decisions, including benign and failed-global episodes' }, { role: 'recipient', family: 'recipient-reconciliation', split: 'development', cases: 2, limitation: 'Related authored mechanism; not independent protected transfer' }, { role: 'R6 recheck', split: 'development', cases: 4, limitation: 'Previously exposed protected cases, retained as development evidence' }],
    mechanical: { status: choice(mechanical?.status, ['passed', 'failed']), full_suite_run: false, checks: (mechanical?.checks ?? []).map(c => ({ id: choice(c.id, ['rust-build', 'typescript-build', 'generated-contracts', 'schema-migrations', 'upgrade-rollback-newer-schema', 'current-store-upgrade', 'managed-exports', 'aggregate-accounting', 'contextual-admission', 'protected-exposure', 'typed-invocation', 'compaction-interrupted-effect', 'bridge-errors-cancellation', 'integrated-recovery', 'qualification-reporting']), status: choice(c.status, ['passed', 'failed']), elapsed_ms: number(c.elapsed_ms), exit_code: number(c.exit_code) })) },
    installed: { status: choice(installed?.installed_checks, ['passed', 'failed']), model_calls_in_smoke_tests: number(installed?.live_model_calls), distinct_tests: number(installed?.distinct_tests), prepared_invocation: choice(installed?.prepared_invocation, ['passed','failed']) },
    model_backed: summarizeLearning(learning), whole_agent_development_comparison: summarizeStudy(r6?.study),
    budget_allocations: learning?.usage && learningDirectory ? allocations(join(learningDirectory, 'state/ribosome.db')) : [],
    selected_receipts: { source: learning?.study ? 'R7 installed learning' : 'R6 retained campaign', observations: existsSync(receiptDatabase) ? receipts(receiptDatabase) : null },
    limitations: ['Mechanical/provider fixtures are separate from live capability results.', 'No universal reliability, causal benefit or useful learned archive diversity claim.', 'Full Linux/macOS CI and package publication were not performed in this R7 run.', 'Private configurations, donor transcripts, hidden answers and model reasoning are omitted.', 'Missing reports and unknown usage remain unknown; this bundle cannot certify absent evidence.', 'R6 usage in this bundle covers the selected development recheck; earlier campaign attempts remain in the validation report.'],
  };
  await mkdir(output, { recursive: true }); await writeFile(join(output, 'evidence.json'), JSON.stringify(result, null, 2) + '\n'); return result;
}
if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const result = await bundle(process.cwd(), process.argv[2] ?? '.ribosome/r7-evidence');
  console.log(JSON.stringify({ mechanical: result.mechanical.status, installed: result.installed.status, learning: result.model_backed.qualification, comparison: result.whole_agent_development_comparison.decision }));
}
