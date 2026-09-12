import { runDemo } from '../../examples/local-project/demo.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';
import { runCase, caseIds } from './cases.mjs';
import { runDevelopment } from './development.mjs';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

// Refuse before preparation or a provider call when credentials are absent.
providerEnvironment();
const args = process.argv.slice(2);
const casesFlag = args.indexOf('--cases');
const selected = casesFlag === -1 ? ['A', 'B', 'C', ...caseIds] : (args[casesFlag + 1] ?? '').split(',');
if (casesFlag !== -1) args.splice(casesFlag, 2);
if (!selected.length || selected.some(id => !['A', 'B', 'C', ...caseIds, 'generated-development'].includes(id)) || new Set(selected).size !== selected.length) throw Error('Select distinct case IDs with --cases A,B,C or named semantic cases');
const directory = resolve(args[0] ?? `.ribosome/evaluations-${Date.now()}`);
await mkdir(directory, { recursive: true });
const results = [];
for (const scenario of selected) {
  console.error(`Live evaluation ${results.length + 1}/${selected.length}: ${scenario}`);
  try {
    const caseDirectory = join(directory, scenario);
    const result = await (scenario === 'generated-development' ? runDevelopment(caseDirectory) : (['A', 'B', 'C'].includes(scenario) ? runDemo : runCase)(scenario, caseDirectory));
    results.push({ scenario, passed: result.passed, metrics: result.metrics, report: join(directory, scenario, 'live-report.json') });
  } catch (error) {
    results.push({ scenario, passed: false, error: error.message, partial_runs: join(directory, scenario, 'live-runs.json') });
  }
  await writeFile(join(directory, 'results.json'), JSON.stringify({ kind: 'live-model', maximum_model_cost_usd: selected.length, results }, null, 2) + '\n');
}
console.log(JSON.stringify(results, null, 2));
if (results.some(r => !r.passed)) process.exitCode = 1;
