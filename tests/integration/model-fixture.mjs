import { createModels } from '@earendil-works/pi-ai';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { azureOpenAIResponsesProvider } from '@earendil-works/pi-ai/providers/azure-openai-responses';

const models = createModels();
models.setProvider(openaiProvider());
models.setProvider(azureOpenAIResponsesProvider());

// These tests replace the provider stream or fetch. Select catalogue metadata
// by capability and price so fixtures need no deployment-specific identifiers.
export function fixtureModel(provider, reasoning = false) {
  const model = models.getModels(provider)
    .filter(m => m.reasoning === reasoning && m.api.endsWith('responses') && m.maxTokens >= 4096 && m.cost.input > 0 && m.cost.output > 0)
    .toSorted((a, b) => a.cost.output - b.cost.output || a.cost.input - b.cost.input || a.id.localeCompare(b.id))[0];
  if (!model) throw new Error(`No ${reasoning ? 'reasoning' : 'non-reasoning'} Responses fixture for ${provider}`);
  return model;
}
