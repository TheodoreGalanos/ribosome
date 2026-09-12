// Live, independently owned Pi execution plus a Ribosome maintenance host.
// The intentionally incorrect report is a demonstration input, not a model result.
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { Agent } from '@earendil-works/pi-agent-core';
import { createModels } from '@earendil-works/pi-ai';
import { azureOpenAIResponsesProvider } from '@earendil-works/pi-ai/providers/azure-openai-responses';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { anthropicProvider } from '@earendil-works/pi-ai/providers/anthropic';
import { Type } from 'typebox';
import { providerEnvironment } from '@ribosome/agents';
import { AttachmentClient, WriteCoordinator } from '@ribosome/agents/attachments';
import { attachPi } from '@ribosome/agents/attachments/pi';

const midRun = process.argv.includes('--mid-run');
const directory = resolve(process.argv.slice(2).find(a => !a.startsWith('--')) ?? `.ribosome/attached-${midRun ? 'mid-run' : 'startup'}-${Date.now()}`);
const executable = process.env.RIBOSOME_CLI ?? resolve('target/debug/ribosome');
const provider = process.env.RIBOSOME_PROVIDER ?? 'azure-openai-responses';
const modelId = process.env.RIBOSOME_MODEL;
if (!modelId?.trim() || modelId === 'your-model-id') throw new Error('RIBOSOME_MODEL must be configured');
const environment = providerEnvironment(provider);
const models = createModels();
models.setProvider(provider === 'azure-openai-responses' ? azureOpenAIResponsesProvider() : provider === 'anthropic' ? anthropicProvider() : openaiProvider());
const model = models.getModel(provider, modelId);
if (!model) throw new Error('Model absent from the pinned Pi catalogue');
const json = value => JSON.stringify(value, null, 2) + '\n';
const hash = text => `sha256:${createHash('sha256').update(text).digest('hex')}`;
await mkdir(directory, { recursive: true });
const source = json({ measurements: [{ value: 1, unit: 'm' }, { value: 200, unit: 'cm' }] });
await writeFile(join(directory, 'source.json'), source, { flag: 'wx' });
await writeFile(join(directory, 'report.json'), json({ total_m: 201, independent_cost_analysis: 250 }), { flag: 'wx' });
const checkFile = join(directory, 'check.mjs');
await writeFile(checkFile, `import assert from 'node:assert/strict'; import {readFileSync} from 'node:fs'; const source=JSON.parse(readFileSync('source.json','utf8')); const report=JSON.parse(readFileSync('report.json','utf8')); const units={m:1,cm:0.01}; const total=source.measurements.reduce((n,m)=>{assert.ok(Object.hasOwn(units,m.unit));return n+m.value*units[m.unit]},0);assert.equal(report.total_m,total);assert.equal(report.independent_cost_analysis,250);console.log('Current source total and independent cost analysis verified');\n`);
const config = { workspace: directory, state_dir: join(directory, '.ribosome'), node: process.execPath,
  worker: fileURLToPath(import.meta.resolve('@ribosome/agents/worker')),
  grant: { id: randomUUID(), scope: { client: 'reference-client', project: 'attached-report' }, mode: 'apply', paths: ['source.json', 'report.json'], writable_paths: ['report.json'], tools: ['report-check'], required_checks: ['report-check'], profiles: ['caretaker'],
    budget: { max_calls: 32, max_tokens: '3000000', max_cost_microusd: '600000', max_actions: 20, max_work_items: 6, max_depth: 1, deadline_ms: String(Date.now() + 300000) }, context: 'attached-report', visible_splits: ['development'], allow_export: false },
  request: { run_id: 'attachment-template', profile: 'caretaker', operator: 'proofreading@1', provider, model: modelId,
    prompt: 'Inspect source.json and report.json against the source tool observations. Save one supported finding identifying any incorrect measurement total. Preserve independent_cost_analysis. During observation report the defect without requesting follow-up work or changing files. During an explicit excision-repair writer handoff, repair report.json through a branch and report-check, apply it, and retain the intervention and actual outcome.' },
  tools: { 'report-check': { program: process.execPath, args: [checkFile], timeout_ms: 2000, reads: ['source.json', 'report.json'], validates: ['report.json'], writes: [] } },
  attachment: { allow_coordinated_writes: true, event_kinds: ['tool.completed'], batch_size: 1 },
};
const configFile = join(directory, 'host.json'); await writeFile(configFile, json(config));
const feedback = [], records = [], sourceUsage = [];
const coordinator = new WriteCoordinator();
let calls = 0, reserved = 0, attachment, client;
let enter, resume;
const entered = new Promise(resolve => { enter = resolve; });
const gate = new Promise(resolve => { resume = resolve; });
const agent = new Agent({ initialState: { model, tools: [{ name: 'read_report', description: 'Read the source measurements and current report.', parameters: Type.Object({}), execute: async () => {
  enter(); if (midRun && !attachment) await gate;
  const result = { source: JSON.parse(await readFile(join(directory, 'source.json'), 'utf8')), report: JSON.parse(await readFile(join(directory, 'report.json'), 'utf8')) };
  return { content: [{ type: 'text', text: json(result) }], details: {} };
} }] }, streamFn: async (currentModel, context, options) => {
  const bound = Math.ceil((Buffer.byteLength(JSON.stringify(context)) * 2 + 4096) * Math.max(model.cost.input, model.cost.cacheRead, model.cost.cacheWrite) + 2048 * model.cost.output);
  if (++calls > 6 || reserved + bound > 400000) throw new Error('External source agent budget exhausted');
  reserved += bound; // Keep uncertain calls reserved; no implicit retries.
  return models.stream(currentModel, context, { ...options, env: environment, maxTokens: 2048, maxRetries: 0, timeoutMs: 30000,
    ...(currentModel.reasoning && currentModel.api !== 'anthropic-messages' ? { reasoningEffort: 'low' } : {}) });
} });
agent.subscribe(event => { if (event.type === 'message_end' && event.message.role === 'assistant') sourceUsage.push({ cost_microusd: String(Math.ceil(event.message.usage.cost.total * 1000000)), complete: !['error', 'aborted'].includes(event.message.stopReason), tokens: event.message.usage.totalTokens }); });
const connect = async () => { attachment = await attachPi(client, agent, { executionId: randomUUID(), coordinatedWrites: true, captureToolContent: true,
  artifacts: async event => event.type === 'tool_execution_end' ? Promise.all(['source.json', 'report.json'].map(async path => ({ path, version: hash(await readFile(join(directory, path), 'utf8')) }))) : [],
  onFeedback: async value => { feedback.push(value); for (const id of value.record_refs) records.push(await attachment.readRecord(id)); },
  onError: error => console.error(`Attachment stopped: ${error.message}`),
}); };
let failure, status;
try {
  client = await AttachmentClient.start({ executable, config: configFile, environment: { PATH: process.env.PATH, ...environment } });
  const prompt = 'Use read_report once to inspect the installed source and report, then briefly describe the measurement total and independent cost analysis. Do not change any files.';
  let sourceRun;
  if (midRun) { sourceRun = agent.prompt(prompt); await entered; await connect(); resume(); }
  else { await connect(); sourceRun = agent.prompt(prompt); }
  await sourceRun;
  assert.equal(agent.state.errorMessage, undefined);
  await attachment.finish();
  assert.ok(records.some(r => {
    const diagnosis = `${r.body.observation ?? ''} ${r.body.interpretation ?? ''}`;
    return r.kind === 'finding' && /\b201\b/.test(diagnosis) && /\b3\b/.test(diagnosis);
  }), 'Expected a supported finding about the seeded total');
  assert.equal(JSON.parse(await readFile(join(directory, 'report.json'), 'utf8')).total_m, 201);
  await attachment.repair(coordinator);
  await attachment.finish();
  const check = spawnSync(process.execPath, [checkFile], { cwd: directory, encoding: 'utf8', env: {} });
  assert.equal(check.status, 0, check.stderr);
  assert.equal(await readFile(join(directory, 'source.json'), 'utf8'), source);
  // Every application-owned writer must use coordinator.run in a real harness.
  await coordinator.run(async () => { assert.equal(JSON.parse(await readFile(join(directory, 'report.json'), 'utf8')).total_m, 3); });
  await agent.prompt('Use read_report once again and describe the current report.');
  assert.equal(agent.state.errorMessage, undefined);
  await attachment.finish(); status = await attachment.status();
} catch (error) { failure = error.message; throw error; }
finally {
  resume(); agent.abort();
  const observations = feedback.map(f => {
    const inspected = spawnSync(executable, ['inspect', join(config.state_dir, 'ribosome.db'), f.run_id], { encoding: 'utf8' });
    return inspected.status === 0 ? JSON.parse(inspected.stdout) : { run_id: f.run_id, unavailable: true };
  });
  const report = { kind: 'live-model-attachment', mode: midRun ? 'mid-run' : 'startup', provider, model: modelId, directory, failure, status, sourceUsage, feedback, records, observations,
    source_reserved_cost_microusd: String(reserved), source_unknown_calls: calls - sourceUsage.filter(u => u.complete).length,
    allowance_microusd: '1000000', limitation: 'Pi catalogue cost estimates, not Azure invoices. This seeded demonstration is not a reliability or benefit study.' };
  await writeFile(join(directory, 'result.json'), json(report));
  await client?.close();
  console.log(json({ directory, passed: !failure, source_calls: calls, maintenance_calls: status?.usage.calls, source_cost_microusd: sourceUsage.reduce((n,u) => n + BigInt(u.cost_microusd), 0n).toString(), maintenance_cost_microusd: status?.usage.observed_cost_microusd }));
}
