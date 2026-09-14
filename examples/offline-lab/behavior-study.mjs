// A bounded development investigation of decisions in one imported execution.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { execute } from './lab.mjs';

const read = async path => JSON.parse(await readFile(path, 'utf8'));
const save = (path, value) => writeFile(path, JSON.stringify(value, null, 2) + '\n');

export async function prepare(directory, cohort) {
  directory = resolve(directory); cohort = resolve(cohort);
  if (existsSync(join(directory, 'report.json'))) throw Error('This investigation is already prepared; use its saved assignments.');
  const report = await read(join(cohort, 'report.json'));
  let episode;
  for (const id of report.episode_ids) {
    const value = await read(join(cohort, 'episodes', `${id}.json`));
    if (value.task === 'numpy__numpydoc-101') episode = value;
  }
  if (!episode || episode.decoded.messages.length < 108) throw Error('The cohort must contain the inspected numpy__numpydoc-101 execution.');
  await mkdir(join(directory, 'workspace'), { recursive: true });
  const window = (start, end) => ({ cohort: 'nebius', episode: episode.id, start, end });
  const manifest = { version: '1', id: `behavior-${randomUUID().slice(0, 8)}`, cohorts: { nebius: cohort },
    episodes: [{ cohort: 'nebius', episode: episode.id, split: 'development' }], assignments: [
      { id: 'nebius-discovery', visibility: 'retrospective', windows: [window(1, 2), window(36, 88)], required_capabilities: ['tool_calls', 'paired_results'] },
      { id: 'nebius-challenge', visibility: 'retrospective', windows: [window(88, 108)], required_capabilities: ['tool_calls', 'paired_results'] },
    ] };
  const budget = { max_calls: 500, max_tokens: '16000000', max_cost_microusd: '5000000', max_actions: 200, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now() + 6 * 3600_000) };
  const config = { workspace: join(directory, 'workspace'), state_dir: join(directory, 'state'), node: process.execPath,
    worker: fileURLToPath(import.meta.resolve('@ribosome/agents/worker')), tools: {},
    grant: { id: `behavior-${randomUUID()}`, scope: { client: 'local', project: 'behavior-study' }, mode: 'sandbox', paths: [], tools: [], profiles: ['caretaker', 'curator', 'experimenter'], budget,
      context: 'behavior-development', visible_splits: ['development'], allow_export: false },
    run_budget: { ...budget, max_calls: 80, max_actions: 0 },
    request: { run_id: 'owner-setup', profile: 'curator', operator: 'discovery@1', prompt: '', provider: 'openai', model: 'configured-at-execution' } };
  const manifestPath = join(directory, 'study.json'), ownerPath = join(directory, 'owner.json');
  await save(manifestPath, manifest); await save(ownerPath, config);
  const assigned = await execute(process.env.RIBOSOME_IMPORT_CLI ?? resolve('target/debug/ribosome-import'), ['assign', manifestPath, ownerPath, join(directory, 'assignments')]);
  await save(join(directory, 'private-assignment-result.json'), assigned);
  if (assigned.code !== 0) throw Error('Assignment failed; inspect private-assignment-result.json.');
  await save(join(directory, 'report.json'), { status: 'prepared', stages: [], qualification: 'not_established',
    source_task: episode.task, discovery_messages: [[1, 2], [36, 88]], challenge_messages: [[88, 108]],
    limitations: ['Owner-selected development windows from one execution.', 'The later challenge belongs to the same task; it tests a nearby boundary, not independent transfer.', 'Shared allowance: at most 500 calls and US$5 at configured catalogue rates.'] });
  return { episodes: 1, discovery_messages: 53, challenge_messages: 20, model_calls: 0 };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, directory, cohort] = process.argv.slice(2);
  if (command !== 'prepare' || !directory || !cohort) throw Error('Use behavior-study.mjs prepare DIRECTORY NEBIUS_COHORT');
  console.log(JSON.stringify(await prepare(directory, cohort)));
}
