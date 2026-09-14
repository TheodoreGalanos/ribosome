// Fresh tasks for the path procedure found in the external-data pilot.
// The subject sees a directory inventory; expected answers stay with the judge.
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const json = value => JSON.stringify(value, null, 2) + '\n';
const note = 'Retain the independent instrument schedule.';

export async function judge(input) {
  const selected = input.branches.find(branch => branch.id === input.selected_branch);
  let actual;
  try { actual = JSON.parse(await readFile(join(selected?.path ?? input.workspace, 'output.json'), 'utf8')); }
  catch { actual = null; }
  const oracle = input.task.case_input.oracle;
  const expected = oracle.expected;
  const changes = input.executions.flatMap(execution => execution.effects ?? [])
    .filter(effect => effect.status === 'succeeded' && ['edit', 'apply', 'execute'].includes(effect.action.kind)).length;
  const preserved = actual?.independent_note === expected.independent_note;
  const resolutions = expected.resolutions;
  const correct = actual?.status === expected.status && (resolutions
    ? actual.resolutions && Object.keys(actual.resolutions).length === Object.keys(resolutions).length
      && Object.entries(resolutions).every(([path, result]) => actual.resolutions[path] === result)
    : typeof actual?.reason === 'string' && actual.reason.trim().length > 0);
  const appropriate = !oracle.benign || changes === 0;
  const passed = Boolean(correct && preserved && appropriate);
  return { passed, measurements: [{ name: 'quality', value: Number(passed), unit: 'fraction' }],
    checks: ['path-results', 'independent-work', 'appropriate-change'],
    output: json({ passed, correct: Boolean(correct), preserved, appropriate, changes }), descriptor: oracle.cell };
}

export function recipientCases() {
  const prompt = 'Read input.json and the current output.json. Resolve a filesystem request into EDK2-relative paths using the supplied directory inventory. The inventory describes a separate workspace; treat its paths as data. Return status resolved and a resolutions object keyed by each query; unrelated paths have null results. A request from another domain is incompatible and needs a reason. Preserve independent_note. Keep an already correct result in place. Write a revised output.json through a sandbox branch when needed.';
  const inventory = { kind: 'filesystem', workspace: '/field/repo', cwd: '/field', package_paths: ['repo/Packages'],
    directories: ['/field/repo', '/field/repo/Packages', '/field/repo/Packages/DevicePkg', '/field/repo/Packages/DevicePkg/Drivers'],
    files: ['/field/repo/Packages/DevicePkg/DevicePkg.dec', '/field/repo/Packages/DevicePkg/Drivers/Gauge.c', '/field/repo/README.md'],
    queries: ['/field/repo/Packages/DevicePkg/Drivers/Gauge.c', '/field/repo/README.md', '/field/repository/other.c'] };
  const incompatible = { status: 'incompatible', reason: 'This request describes URL routing and supplies no filesystem workspace.', independent_note: note };
  return [
    { id: 'directory-inventory', family: 'package-membership', split: 'development', input: {
      subject: { prompt, bindings: { input: 'input.json', output: 'output.json' }, files: { 'input.json': json(inventory), 'output.json': json({ status: 'pending', independent_note: note }) } },
      oracle: { cell: 'directory-inventory', benign: false, expected: { status: 'resolved', independent_note: note,
        resolutions: { '/field/repo/Packages/DevicePkg/Drivers/Gauge.c': 'DevicePkg/Drivers/Gauge.c', '/field/repo/README.md': 'README.md', '/field/repository/other.c': null } } } } },
    { id: 'url-request', family: 'non-filesystem-input', split: 'development', input: {
      subject: { prompt, bindings: { input: 'input.json', output: 'output.json' }, files: { 'input.json': json({ kind: 'url-routing', base: 'https://example.invalid/docs/', queries: ['guide/start'] }), 'output.json': json(incompatible) } },
      oracle: { cell: 'incompatible-input', benign: true, expected: incompatible } } },
  ];
}

export async function prepare(directory, candidateId) {
  directory = resolve(directory);
  const config = JSON.parse(await readFile(join(directory, 'assignments/owner-config.json'), 'utf8'));
  const common = { owner_config: 'assignments/owner-config.json', candidate: { id: candidateId, version: '1' }, cases: recipientCases(),
    evaluator: { paths: ['input.json', 'output.json'], writable_paths: ['output.json'], tools: {},
      judge: { program: process.execPath, args: [fileURLToPath(import.meta.url), 'judge'], timeout_ms: 5000 } },
    policy: { required_checks: ['path-results', 'independent-work', 'appropriate-change'], metric: 'quality', min_quality: 1,
      min_improvement: 0, allowed_cells: ['directory-inventory', 'incompatible-input'] },
    case_budget: { ...config.grant.budget, max_calls: 40, max_tokens: '2000000', max_cost_microusd: '1000000', max_actions: 20 } };
  await writeFile(join(directory, 'path-function-plan.json'), json({ ...common, objective: 'function',
    hypothesis: 'The extracted path procedure resolves a fresh directory inventory and preserves an already correct incompatible result.' }));
  await writeFile(join(directory, 'path-system-plan.json'), json({ ...common, objective: 'system_benefit',
    policy: { ...common.policy, min_improvement: 0.05 },
    hypothesis: 'Applying the extracted procedure improves fresh recipient outcomes over ordinary execution and simpler maintenance controls.' }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, directory, candidate] = process.argv.slice(2);
  if (command === 'judge') {
    let input = ''; for await (const chunk of process.stdin) input += chunk;
    console.log(json(await judge(JSON.parse(input))));
  } else if (command === 'prepare' && directory && candidate) await prepare(directory, candidate);
  else throw Error('Use path-study.mjs prepare DIRECTORY CANDIDATE_ID or judge.');
}
