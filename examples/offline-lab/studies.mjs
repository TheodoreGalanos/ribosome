// Study orchestration uses the same CLI, record store and Pi evaluator as an
// application. Recipient tasks and their independent judge are owner inputs.
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { providerEnvironment } from '@ribosome/agents';
import { execute, stored } from './lab.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const read = async path => JSON.parse(await readFile(path, 'utf8'));
const save = (path, value) => writeFile(path, JSON.stringify(value, null, 2) + '\n');
const ref = record => ({ id: record.id, version: record.body.version });

export function supportedDiscovery(records, corpus) {
  const found = records.filter(r => !r.retired && r.kind === 'discovery' && r.body.corpus.id === corpus
    && r.body.decision === 'supported' && r.body.definition_refs.length && r.body.occurrence_refs.length).at(-1);
  if (!found) throw Error('No supported investigation with saved occurrences. Inspect discovery before extraction or contrast.');
  return found;
}

export function requireSupportedCandidate(records, candidate) {
  const references = candidate?.body.instruction_contract?.discovery_refs;
  if (!references?.length) throw Error('Select a saved extracted instruction linked to its investigation.');
  for (const reference of references) {
    const source = records.find(r => !r.retired && r.id === reference.id && r.version === reference.version);
    const investigation = source && records.find(r => !r.retired && r.kind === 'discovery' && r.body.decision === 'supported' && r.body.occurrence_refs.length
      && (source.kind === 'discovery' ? r.id === source.id
        : source.kind === 'definition' ? r.body.definition_refs.some(d => d.id === source.id && d.version === source.body.version)
        : source.kind === 'occurrence' && r.body.occurrence_refs.includes(source.id)));
    if (!investigation) throw Error('Candidate investigation is absent or incomplete.');
  }
}

function configured(config) {
  config.request.provider = process.env.RIBOSOME_PROVIDER ?? 'openai';
  config.request.model = process.env.RIBOSOME_MODEL;
  if (!config.request.model) throw Error('Set RIBOSOME_MODEL in your local environment.');
  return providerEnvironment(config.request.provider);
}

async function host(directory, config, command, args = [], environment = {}) {
  const file = join(directory, `private-${config.request.run_id}-config.json`);
  await save(file, config);
  const result = await execute(process.env.RIBOSOME_CLI ?? resolve('target/debug/ribosome'), [command, file, ...args], environment);
  await save(join(directory, `private-${config.request.run_id}-${command}.json`), result);
  let value; try { value = JSON.parse(result.stdout); } catch { throw Error('Host returned no structured result; inspect the private diagnostic.'); }
  if (result.code !== 0 && !(command === 'run' && value.disposition)) throw Error('Host command failed; inspect the private diagnostic.');
  return value;
}

export async function followDiscovery(directory, action, cohort, extra) {
  directory = resolve(directory);
  if (!['aec', 'nebius'].includes(cohort)) throw Error('Choose aec or nebius.');
  const config = await read(join(directory, 'assignments/owner-config.json'));
  const assignment = await read(join(directory, 'assignments', `${cohort}-discovery.json`));
  const records = stored(directory).records;
  if (action === 'retrieve' || action === 'retrieve-agent') {
    const queries = await read(extra);
    if (!Array.isArray(queries) || !queries.length || queries.some(q => typeof q !== 'string')) throw Error('Queries must be a nonempty JSON array of strings.');
    if (action === 'retrieve-agent' && queries.length > 3) throw Error('Agentic retrieval accepts at most three questions per probe.');
    const results = [];
    for (const query of queries) {
      config.request.run_id = `retrieval-${randomUUID()}`;
      if (action === 'retrieve-agent') {
        const before = new Set(stored(directory, true).records.map(r => r.id));
        const environment = configured(config);
        config.run_budget = { ...config.grant.budget, max_calls: 12, max_actions: 0, max_work_items: 0,
          max_cost_microusd: String(Math.min(Number(config.grant.budget.max_cost_microusd), 500000)) };
        config.request.profile = 'curator';
        config.request.operator = 'memory@1';
        delete config.request.discovery_corpus;
        config.request.prompt = `Find a prepared instruction for this request: ${JSON.stringify(query)}. Start with the request's wording, then reformulate at most three searches using the existing lexical search tools, any_terms and relevance ordering. Search the evidence inventory because an experimental candidate may be unadmitted. Read a promising definition and implementation before judging fit. Save a short memory with the selected implementation and definition in evidence_refs, applicability and unresolved requirements, or report no suitable candidate. Do not execute or admit it. You have at most 12 model calls and three reformulated searches.`;
        const result = await host(directory, config, 'run', [], environment);
        const selected = stored(directory, true).records.filter(r => !before.has(r.id) && r.kind === 'memory');
        results.push({ query, run_id: config.request.run_id, disposition: result.disposition, summary: result.summary,
          memories: selected.map(r => ({ id: r.id, content: r.body.content, applicability: r.body.applicability, evidence_refs: r.body.evidence_refs })) });
        continue;
      }
      const path = join(directory, 'private-search.json');
      await save(path, { query, kind: 'implementation', inventory: 'evidence', limit: 10, offset: 0, query_mode: 'any_terms', order: 'relevance', eligible: true });
      const page = await host(directory, config, 'search', [path]);
      results.push({ query, matches: page.records.map(ref) });
    }
    await save(join(directory, `retrieval-${action === 'retrieve-agent' ? 'agent-' : ''}${cohort}.json`), results);
    return results;
  }
  const discovery = supportedDiscovery(records, assignment.corpus.id);
  const environment = configured(config);
  config.request.run_id = `${cohort}-${action}-${randomUUID()}`;
  config.request.profile = 'curator';
  config.run_budget ??= { ...config.grant.budget, max_calls: 20 };
  if (action === 'contrast') {
    const challenge = await read(join(directory, 'assignments', `${cohort}-challenge.json`));
    const corpus = { ...challenge.corpus, id: `${cohort}-contrast-${randomUUID()}`, definition_refs: discovery.body.definition_refs };
    const donorEvents = new Set(discovery.body.source_windows.flatMap(w => w.event_refs));
    if (corpus.source_windows.some(w => w.event_refs.some(id => donorEvents.has(id)))) throw Error('Challenge overlaps the discovery evidence. Assign a separate evidence window first.');
    config.corpora = [corpus];
    config.request.operator = 'contrast-motif@1';
    config.request.discovery_corpus = { id: corpus.id, version: corpus.version };
    config.request.prompt = `Challenge the assigned definitions against this separate evidence window. Inspect applicability, competing explanations and disconfirming evidence. Save one discovery investigation with the actual references; retain missing evidence and disagreement. If the challenge is from the same task, describe it as a dependent development contrast. This run has up to ${config.run_budget.max_calls} calls within the shared campaign allowance.`;
  } else if (action === 'extract') {
    const contract = await read(extra);
    contract.discovery_refs = [{ id: discovery.id, version: discovery.version }];
    config.request.operator = 'extraction@1';
    config.request.discovery_corpus = { id: assignment.corpus.id, version: assignment.corpus.version };
    config.request.prompt = (await readFile(join(here, 'prompts/extraction.md'), 'utf8'))
      + `\nRead investigation ${discovery.id}, definitions ${JSON.stringify(discovery.body.definition_refs)} and occurrences ${JSON.stringify(discovery.body.occurrence_refs)}. Use this owner-specified recipient interface as instruction_contract: ${JSON.stringify(contract)}. Save at most one candidate. This run has up to ${config.run_budget.max_calls} calls within the shared campaign allowance.`;
  } else throw Error('Use contrast, extract, retrieve or retrieve-agent.');
  const result = await host(directory, config, 'run', [], environment);
  const before = new Set(records.map(r => r.id));
  const after = stored(directory);
  const report = { action, cohort, run_id: config.request.run_id, disposition: result.disposition,
    saved_records: after.records.filter(r => !before.has(r.id)).map(r => ({ id: r.id, kind: r.kind })), usage: after.usage };
  await save(join(directory, `${config.request.run_id}.json`), report);
  return report;
}

export function studyShape(plan) {
  if (!['function', 'system_benefit'].includes(plan.objective)) throw Error('Study objective must be function or system_benefit.');
  if (plan.name !== undefined && (typeof plan.name !== 'string' || !/^[a-z][a-z0-9-]{0,39}$/.test(plan.name))) throw Error('Study name must be a short lowercase filename component.');
  if (!Array.isArray(plan.cases) || plan.cases.length < 2) throw Error('Provide at least two fresh recipient cases, including an incompatible or benign case.');
  if (new Set(plan.cases.map(c => c.id)).size !== plan.cases.length) throw Error('Recipient case IDs must be unique.');
  if (plan.cases.some(c => !c.input?.subject || !c.input?.oracle)) throw Error('Separate each case into input.subject and owner-only input.oracle.');
  const families = [...new Set(plan.cases.map(c => c.family))];
  if (plan.objective === 'system_benefit' && families.length < 2) throw Error('System benefit requires two recipient families.');
  const arms = plan.objective === 'function' ? 2 : 5;
  return { arms, repetitions: 2, planned: plan.cases.length * arms * 2, families };
}

export async function runStudy(directory, planPath) {
  directory = resolve(directory); planPath = resolve(planPath);
  const plan = await read(planPath), shape = studyShape(plan);
  const reportPath = join(directory, `${plan.objective}${plan.name ? `-${plan.name}` : ''}-study.json`);
  if (existsSync(reportPath)) throw Error('This study already has an attempt. Preserve it; use its saved CLI config to inspect or resume the same study.');
  const config = await read(resolve(dirname(planPath), plan.owner_config));
  if (resolve(config.state_dir) !== join(directory, 'state') || config.grant.mode !== 'sandbox') throw Error('Use a sandbox owner configuration for this lab database.');
  const records = stored(directory).records;
  const candidate = records.find(r => r.id === plan.candidate.id && r.body.version === plan.candidate.version && r.kind === 'implementation' && !r.retired);
  requireSupportedCandidate(records, candidate);
  if (plan.objective === 'system_benefit') {
    const prior = await read(join(directory, `function${plan.name ? `-${plan.name}` : ''}-study.json`));
    if (!prior.study?.complete || prior.study.decision !== 'accepted' || prior.candidate.id !== candidate.id || prior.candidate.version !== candidate.body.version) throw Error('System comparison requires an accepted function study for the same candidate.');
  }
  const environment = configured(config);
  delete config.run_budget;
  config.corpora = [];
  config.request = { run_id: `${plan.objective}-${randomUUID()}`, profile: 'experimenter', operator: 'experiment@1', prompt: 'Run the owner-assigned study.', provider: config.request.provider, model: config.request.model };
  const provenance = { origin: 'synthetic', source_refs: [], scenario_family: 'host-control', split: 'development', limitations: ['Owner-authored experimental control.'] };
  const control = (name, material) => ({ kind: 'implementation', provenance, body: { name, version: '1', motifs: [], format: 'instructions', material, parameters: {}, required_capabilities: [], state_assumptions: [], possible_effects: [], failure_behavior: 'Report unresolved evidence.', evaluation_refs: [], instruction_contract: { ...candidate.body.instruction_contract, discovery_refs: [] } } });
  const file = join(directory, 'private-study-records.json');
  await save(file, [control('ordinary-worker', 'Complete the recipient task from current evidence and permitted tools.'), control('ordinary-critique', 'Review the prior attempt against the task. Correct a supported error and report the result.'), control('ordinary-care', 'Inspect the prior output and its support. Repair a demonstrated problem, preserve independent work, and leave supported output in place.')]);
  const [baseline, critique, care] = (await host(directory, config, 'records', [file])).map(ref);
  const stage = (implementation, prompt) => ({ implementation, prompt });
  const first = stage(baseline, 'Complete the task.');
  const stages = { baseline: [first], retry: [first, stage(baseline, 'Make another attempt using the remaining allowance.')], critique: [first, stage(critique, 'Review and revise the prior attempt.')], care: [first, stage(care, 'Maintain the prior attempt.')], candidate: [first, stage(null, 'Apply the prepared instruction if compatible with the current task.')] };
  const usage = stored(directory).usage;
  const learning_cost = { cost_microusd: String(usage.known_cost_microusd), reuse_count: 1, complete: usage.unknown_calls === 0 };
  const policy = { ...plan.policy, id: `${plan.objective}@1`, context: config.grant.context, evaluator: 'subject', evaluator_version: '1', case_ids: plan.cases.map(c => c.id), repetitions: shape.repetitions, max_evaluations: shape.planned, retain_learning_memory: false, study_objective: plan.objective, case_budget: plan.case_budget,
    ...(plan.objective === 'system_benefit' ? { learning_cost } : {}) };
  config.cases = plan.cases; config.policies = [policy];
  config.agent_evaluators = { subject: { ...plan.evaluator, provider: config.request.provider, model: config.request.model, model_version: config.request.model, tool_versions: [],
    ...(plan.objective === 'system_benefit' ? { stages } : {}) } };
  const experiment = { kind: 'experiment', provenance, body: { name: `offline-${plan.objective}`, template: 'transfer', study_objective: plan.objective, candidate: ref(candidate), baseline, variants: [], hypothesis: plan.hypothesis,
    case_ids: policy.case_ids, scenario_families: shape.families, feedback: 'aggregate', model_version: config.request.model, tool_versions: [], memory_start_refs: [], repetitions: shape.repetitions, budget: config.grant.budget, metrics: [policy.metric], policy_id: policy.id, selection_frozen: true,
    ...(plan.objective === 'system_benefit' ? { learning_cost, variants: [{ arm: 'retry', implementation: baseline }, { arm: 'critique', implementation: critique }, { arm: 'care', implementation: care }] } : {}) } };
  await save(file, [experiment]);
  const [saved] = await host(directory, config, 'records', [file]);
  const report = { objective: plan.objective, candidate: ref(candidate), experiment: saved.id, planned_evaluations: shape.planned, status: 'running' };
  await save(reportPath, report);
  try {
    report.study = await host(directory, config, 'study', [saved.id], environment);
    report.status = report.study.complete ? 'complete' : 'incomplete';
  } catch (error) { report.status = 'infrastructure_error'; report.error = error.message; }
  finally { report.usage = stored(directory).usage; await save(reportPath, report); }
  return report;
}
