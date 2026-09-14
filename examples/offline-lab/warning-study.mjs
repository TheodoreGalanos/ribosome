// Fresh configuration-repair tasks for the observed warning investigation strategy.
// The fixed checker interprets JSON; it never executes recipient-authored code.
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const read = async path => JSON.parse(await readFile(path, 'utf8'));
const json = value => JSON.stringify(value, null, 2) + '\n';
const note = 'Preserve the independent calibration schedule.';
const script = fileURLToPath(import.meta.url);

export const instructionContract = {
  inputs: [{ name: 'input', kind: 'artifact_path', required: true }, { name: 'output', kind: 'artifact_path', required: true }, { name: 'checker', kind: 'tool', required: true }],
  outputs: ['Warning configuration satisfying the recipient contract, or a report of unresolved evidence.'],
  entry_obligations: ['Inspect the warning routes, actual wrapper metadata and current diagnostic output before deciding whether a repair is needed.'],
  exit_obligations: ['Check contextual and context-free cases after a repair; preserve an already correct configuration and independent_note.'],
  limitations: ['A local configuration repair tests application of the investigation strategy, not source-code repair across arbitrary projects.'],
  discovery_refs: [],
};

export function validConfiguration(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).sort().join(',') === 'context_field,independent_note,parser_warning,setter_warning'
    && (value.context_field === null || typeof value.context_field === 'string')
    && typeof value.parser_warning === 'boolean' && typeof value.setter_warning === 'boolean'
    && typeof value.independent_note === 'string';
}

// The two routes model a parser that calls a generic setter. Only the parser
// receives a wrapper, so moving context lookup to the setter cannot add context.
export function warningOutput(input, configuration) {
  if (!validConfiguration(configuration)) return null;
  return input.probes.map(probe => {
    const warnings = [];
    const generic = `Unknown section ${probe.section}`;
    const context = configuration.context_field === null ? undefined : probe.wrapper?.[configuration.context_field];
    if (probe.route === 'parse' && configuration.parser_warning) {
      warnings.push(context?.name && context?.file ? `${generic} in ${context.name} (${context.file})` : generic);
    }
    if ((probe.route === 'assign' || probe.route === 'parse') && configuration.setter_warning) warnings.push(generic);
    return { id: probe.id, warnings };
  });
}

export function recipientCases() {
  const prompt = 'Read input.json and output.json. Repair the warning configuration if its diagnostics do not meet the stated contract. Run the bound checker to observe current diagnostics. Inspect the supplied routes and wrapper fields, then decide whether a change is needed. After a change, check again. Use a sandbox branch for edits and diagnostic checks; leave an already correct configuration in place. output.json must contain exactly context_field (string or null), parser_warning (boolean), setter_warning (boolean), and independent_note (the existing string). context_field names a property to read from each probe.wrapper; it is a lookup key, not formatted warning text. The selected property supplies name and file for the parser warning. A null or absent property supplies no context. parser_warning enables emission in parse; setter_warning enables emission in assign, including when parse calls assign. A parse probe must produce exactly one warning. Include the object name and file when its wrapper supplies them, and keep a generic warning when context is absent. These probes concern parsing; preserve independent_note.';
  const contextual = { id: 'wrapped', route: 'parse', section: 'NotesX', wrapper: { _origin: { name: 'readGauge', file: 'gauge.js' } } };
  const plain = { id: 'plain', route: 'parse', section: 'NotesX', wrapper: {} };
  const make = (id, probes, initial, expected, benign) => ({ id, family: 'warning-configuration', split: 'development', input: {
    subject: { prompt, bindings: { input: 'input.json', output: 'output.json', checker: 'warning-check' }, files: {
      'input.json': json({ routes: ['parse(wrapper) optionally warns, then calls assign(section).', 'assign(section) optionally emits a generic warning and receives no wrapper.'], probes }),
      'output.json': json({ ...initial, independent_note: note }),
    } }, oracle: { expected, benign, independent_note: note },
  } });
  return [
    make('context-and-duplicate', [contextual, plain], { context_field: '_object', parser_warning: true, setter_warning: true },
      [{ id: 'wrapped', warnings: ['Unknown section NotesX in readGauge (gauge.js)'] }, { id: 'plain', warnings: ['Unknown section NotesX'] }], false),
    make('already-correct-plain', [plain], { context_field: null, parser_warning: false, setter_warning: true },
      [{ id: 'plain', warnings: ['Unknown section NotesX'] }], true),
  ];
}

export async function judge(input) {
  const workspace = input.branches.find(branch => branch.id === input.selected_branch)?.path ?? input.workspace;
  let output; try { output = await read(join(workspace, 'output.json')); } catch { output = null; }
  const oracle = input.task.case_input.oracle;
  const observed = warningOutput(JSON.parse(input.task.case_input.subject.files['input.json']), output);
  const interfaceCompliant = validConfiguration(output);
  const correct = observed === null ? null : observed.length === oracle.expected.length
    && observed.every((probe, i) => probe.id === oracle.expected[i].id && probe.warnings.length === oracle.expected[i].warnings.length && probe.warnings.every((warning, j) => warning === oracle.expected[i].warnings[j]));
  const effects = input.executions.flatMap(execution => execution.effects ?? []).filter(effect => effect.status === 'succeeded');
  const edits = effects.filter(effect => ['edit', 'apply'].includes(effect.action.kind)).length;
  const checks = effects.filter(effect => ['check', 'execute'].includes(effect.action.kind) && effect.action.tool === 'warning-check').length;
  const appropriate = !oracle.benign || edits === 0;
  const preserved = output?.independent_note === oracle.independent_note;
  const passed = Boolean(interfaceCompliant && correct && appropriate && preserved);
  const measured = { quality: passed, interface_compliance: interfaceCompliant, appropriate_intervention: appropriate, independent_work: preserved,
    ...(correct === null ? {} : { functional_correctness: correct }) };
  return { passed, measurements: Object.entries(measured).map(([name, value]) => ({ name, value: Number(value), unit: 'fraction' })),
    checks: ['output-interface', 'warning-results', 'independent-work', 'appropriate-change'], descriptor: input.task.case_id,
    output: json({ passed, interface_compliant: interfaceCompliant, correct, appropriate, preserved, edits, checks, observed,
      failure_categories: [!interfaceCompliant && 'interface', correct === false && 'function', correct === null && 'function_not_assessed', !appropriate && 'unnecessary_intervention', !preserved && 'independent_work'].filter(Boolean) }) };
}

export async function prepare(directory, candidate) {
  directory = resolve(directory);
  if (existsSync(join(directory, 'warning-function-plan.json'))) throw Error('This function study is already prepared; use its saved plan.');
  const config = await read(join(directory, 'assignments/owner-config.json'));
  const checker = { program: process.execPath, args: [script, 'check'], timeout_ms: 5000, reads: ['input.json', 'output.json'], validates: ['output.json'] };
  const db = new DatabaseSync(join(directory, 'state/ribosome.db'), { readOnly: true });
  let spent;
  try {
    if (db.prepare("SELECT 1 FROM runs WHERE status IN ('running','queued') LIMIT 1").get()) throw Error('Finish the current investigation before preparing the function study.');
    spent = db.prepare("SELECT count(*) calls, coalesce(sum(reserved_tokens),0) tokens, coalesce(sum(reserved_cost),0) cost FROM permits WHERE state!='released'").get();
  } finally { db.close(); }
  // Finish discovery first. This new tool-bearing grant receives only the unspent
  // campaign allowance, including any outstanding provider reservations.
  config.grant.id += '-function';
  config.grant.tools = ['warning-check'];
  config.tools = { 'warning-check': checker };
  config.grant.budget.max_calls -= spent.calls;
  config.grant.budget.max_tokens = String(Number(config.grant.budget.max_tokens) - spent.tokens);
  config.grant.budget.max_cost_microusd = String(Number(config.grant.budget.max_cost_microusd) - spent.cost);
  if (config.grant.budget.max_calls <= 0 || Number(config.grant.budget.max_tokens) <= 0 || Number(config.grant.budget.max_cost_microusd) <= 0) throw Error('The campaign has no remaining function-study allowance.');
  await writeFile(join(directory, 'warning-owner.json'), json(config));
  const plan = { owner_config: 'warning-owner.json', objective: 'function', candidate: { id: candidate, version: '1' },
    hypothesis: 'The extracted investigation strategy repairs contextual and duplicate warnings and preserves an already correct context-free configuration.', cases: recipientCases(),
    evaluator: { paths: ['input.json', 'output.json'], writable_paths: ['output.json'], tools: { 'warning-check': checker },
      judge: { program: process.execPath, args: [script, 'judge'], timeout_ms: 5000 } },
    policy: { required_checks: ['output-interface', 'warning-results', 'independent-work', 'appropriate-change'], metric: 'quality', min_quality: 1, min_improvement: 0,
      allowed_cells: ['context-and-duplicate', 'already-correct-plain'] },
    case_budget: { ...config.grant.budget, max_calls: 40, max_tokens: '2000000', max_cost_microusd: '400000', max_actions: 12 } };
  await writeFile(join(directory, 'warning-function-plan.json'), json(plan));
  return { planned_executions: 8 };
}

if (process.argv[1] && resolve(process.argv[1]) === script) {
  const [command, directory, candidate] = process.argv.slice(2);
  if (command === 'check') {
    const output = warningOutput(await read('input.json'), await read('output.json'));
    console.log(json({ interface_compliant: output !== null, observed: output }));
  } else if (command === 'judge') {
    let input = ''; for await (const chunk of process.stdin) input += chunk;
    console.log(json(await judge(JSON.parse(input))));
  } else if (command === 'contract' && directory) await writeFile(directory, json(instructionContract));
  else if (command === 'prepare' && directory && candidate) console.log(json(await prepare(directory, candidate)));
  else throw Error('Use warning-study.mjs contract FILE, prepare DIRECTORY CANDIDATE_ID, check, or judge.');
}
