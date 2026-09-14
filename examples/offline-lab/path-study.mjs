// Fresh tasks for the path procedure found in the external-data pilot.
// The subject sees a directory inventory; expected answers stay with the judge.
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const json = value => JSON.stringify(value, null, 2) + '\n';
const note = 'Retain the independent instrument schedule.';

export const recipientContract = {
  output: {
    resolved: { status: 'resolved', resolutions: 'object keyed by every input query; each value is a path string or null', independent_note: 'preserve the existing string' },
    incompatible: { status: 'incompatible', reason: 'nonempty string', independent_note: 'preserve the existing string' },
    additional_fields: false,
  },
  semantics: [
    'Resolve a relative package_paths entry against cwd. Compare complete path components when deciding membership.',
    'A package directory contains a .dec file and lies below a supplied package-path root. For a query in a package, return its path relative to the parent of that package directory, including the package directory name.',
    'For a query inside the workspace but outside a package, return its workspace-relative path. For a query outside the workspace, return null. Use forward slashes in returned paths.',
    'A non-filesystem request is incompatible. Preserve an already correct output without writing it again.',
  ],
};

export async function judge(input) {
  const selected = input.branches.find(branch => branch.id === input.selected_branch);
  let actual;
  try { actual = JSON.parse(await readFile(join(selected?.path ?? input.workspace, 'output.json'), 'utf8')); }
  catch { actual = null; }
  const oracle = input.task.case_input.oracle;
  const expected = oracle.expected;
  const changes = input.executions.flatMap(execution => execution.effects ?? [])
    .filter(effect => effect.status === 'succeeded' && ['edit', 'apply', 'execute'].includes(effect.action.kind)).length;
  const object = actual !== null && typeof actual === 'object' && !Array.isArray(actual);
  const map = actual?.resolutions;
  const flatMap = map !== null && typeof map === 'object' && !Array.isArray(map)
    && Object.values(map).every(value => value === null || typeof value === 'string');
  const fields = actual?.status === 'resolved' ? ['status', 'resolutions', 'independent_note'] : ['status', 'reason', 'independent_note'];
  const interfaceCompliant = Boolean(object && Object.keys(actual).length === fields.length && fields.every(key => Object.hasOwn(actual, key))
    && typeof actual.independent_note === 'string'
    && (actual.status === 'resolved' ? flatMap : actual.status === 'incompatible' && typeof actual.reason === 'string' && actual.reason.trim().length > 0));
  const preserved = actual?.independent_note === expected.independent_note;
  const resolutions = expected.resolutions;
  const correct = !object || (actual.status === 'resolved' && !flatMap) ? null
    : actual.status === expected.status && (resolutions
      ? flatMap && Object.keys(map).length === Object.keys(resolutions).length && Object.entries(resolutions).every(([path, result]) => map[path] === result)
      : typeof actual.reason === 'string' && actual.reason.trim().length > 0);
  const appropriate = !oracle.benign || changes === 0;
  const passed = Boolean(interfaceCompliant && correct && preserved && appropriate);
  const failureCategories = [!interfaceCompliant && 'interface', correct === false && 'function', correct === null && 'function_not_assessed', !preserved && 'independent_work', !appropriate && 'unnecessary_intervention'].filter(Boolean);
  const measured = { quality: passed, interface_compliance: interfaceCompliant, appropriate_intervention: appropriate, independent_work: preserved,
    ...(correct === null ? {} : { functional_correctness: correct }) };
  return { passed, measurements: Object.entries(measured).map(([name, value]) => ({ name, value: Number(value), unit: 'fraction' })),
    checks: ['output-interface', 'path-results', 'independent-work', 'appropriate-change'],
    output: json({ passed, interface_compliant: interfaceCompliant, correct, preserved, appropriate, changes, failure_categories: failureCategories }), descriptor: oracle.cell };
}

export function recipientCases() {
  const prompt = 'Read input.json and the current output.json. Resolve a filesystem request using the supplied directory inventory. The inventory describes a separate workspace; treat its paths as data. Write a revised output.json through a sandbox branch when needed. Both study conditions receive this same output contract and semantics: ' + JSON.stringify(recipientContract);
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
    policy: { required_checks: ['output-interface', 'path-results', 'independent-work', 'appropriate-change'], metric: 'quality', min_quality: 1,
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
