import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { runCli, runCliAsync, json } from './prepare.mjs';
import { providerEnvironment as selectedEnvironment } from '../../packages/agents/dist/index.js';

export function providerEnvironment(provider = process.env.RIBOSOME_PROVIDER ?? 'openai', env = process.env) {
  try { return selectedEnvironment(provider, env); }
  catch (error) { throw Error(`${error.message}. Required for a live Pi evaluation. No scripted fallback is used.`); }
}

export function liveSession(prepared) {
  const { config, configFile } = prepared;
  const environment = providerEnvironment(config.request.provider);
  if (!config.request.model?.trim() || config.request.model === 'your-model-id') throw new Error('Configure RIBOSOME_MODEL before preparation, or set request.model in the run configuration');
  const reports = [];
  return {
    reports,
    async run(profile, operator, prompt) {
      config.request = { ...config.request, run_id: randomUUID(), profile, operator, prompt };
      await writeFile(configFile, json(config));
      const start = performance.now();
      let result, failure;
      try { result = await runCliAsync(['run', configFile], environment); }
      catch (error) { failure = error; }
      const observed = runCli(['inspect', join(config.state_dir, 'ribosome.db'), config.request.run_id]);
      const report = { profile, operator, result, observed, elapsed_ms: performance.now() - start };
      reports.push(report);
      await writeFile(join(prepared.directory, 'live-runs.json'), json({ kind: 'live-model', reports }));
      if (failure) throw failure;
      return report;
    },
    async search(kind, inventory = 'evidence', configPath = configFile) {
      const file = join(prepared.directory, 'query.json');
      await writeFile(file, json({ query: '', kind, inventory, limit: 100, offset: 0 }));
      return runCli(['search', configPath, file]).records;
    },
  };
}

export function metrics(reports) {
  const effects = reports.flatMap(r => r.observed.effects ?? []);
  return {
    elapsed_ms: reports.reduce((sum, r) => sum + r.elapsed_ms, 0),
    model_calls: reports.reduce((sum, r) => sum + r.observed.model_usage.calls, 0),
    observed_model_cost_microusd: reports.reduce((sum, r) => sum + BigInt(r.observed.model_usage.observed_cost_microusd), 0n).toString(),
    usage_complete: reports.every(r => r.observed.model_usage.complete),
    checks: effects.filter(e => e.action.kind === 'check').length,
    writes: effects.filter(e => ['edit', 'execute', 'apply'].includes(e.action.kind)).length,
    rejected_effects: effects.filter(e => ['denied', 'stale', 'failed', 'unknown'].includes(e.status)).length,
  };
}
