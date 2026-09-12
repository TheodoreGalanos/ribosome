import { providerEnvironment } from '../packages/agents/dist/index.js';

const provider = process.env.RIBOSOME_PROVIDER ?? 'openai';
const model = process.env.RIBOSOME_MODEL;

try {
  if (!model?.trim() || model === 'your-model-id') throw new Error('RIBOSOME_MODEL must be configured');
  const environment = providerEnvironment(provider);
  console.log(JSON.stringify({
    provider, model,
    settings: Object.fromEntries(Object.keys(environment).map(name => [name, '<set>'])),
    status: 'configuration-present',
    limitation: 'No network or model call was made. This does not verify deployment access, model compatibility or Azure billing rates.',
  }, null, 2));
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
