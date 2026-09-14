import { randomUUID } from 'node:crypto';
import type { StreamFn } from '@earendil-works/pi-agent-core';
import { createAssistantMessageEventStream, createModels, type AssistantMessage, type Model, type Api } from '@earendil-works/pi-ai';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { anthropicProvider } from '@earendil-works/pi-ai/providers/anthropic';
import { azureOpenAIResponsesProvider } from '@earendil-works/pi-ai/providers/azure-openai-responses';
import type { AgentRunRequest } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { RpcError } from '../client/validation.js';
import { providerEnvironment } from './provider.js';

export function modelTransport(request: AgentRunRequest, streamOverride?: StreamFn) {
  const models = createModels();
  if (request.provider === 'openai') models.setProvider(openaiProvider());
  else if (request.provider === 'anthropic') models.setProvider(anthropicProvider());
  else if (request.provider === 'azure-openai-responses') models.setProvider(azureOpenAIResponsesProvider());
  else throw new RpcError(-32602, 'supported providers are openai, anthropic and azure-openai-responses');
  const model = models.getModel(request.provider, request.model);
  if (!model) throw new RpcError(-32602, 'model absent from pinned Pi provider catalogue');
  const configuredOutput = Number(process.env.RIBOSOME_MODEL_MAX_OUTPUT_TOKENS ?? 4096);
  if (!Number.isInteger(configuredOutput) || configuredOutput < 1 || configuredOutput > 32768) {
    throw new RpcError(-32602, 'RIBOSOME_MODEL_MAX_OUTPUT_TOKENS must be between 1 and 32768');
  }
  const maxOutput = Math.min(configuredOutput, model.maxTokens);
  const environment = streamOverride ? {} : providerEnvironment(request.provider);

  return { model, metered(peer: RpcPeer, compactionId?: string) {
    let permitId: string | undefined;
    let completedPermitId: string | undefined;
    let failure: RpcError | undefined;
    const streamFn: StreamFn = async (currentModel, context, options) => {
      const callId = randomUUID();
      try {
        // UTF-8 bytes provide a deliberately conservative token reservation;
        // observed usage replaces it only after a completed provider response.
        const inputBound = Buffer.byteLength(JSON.stringify(context)) * 2 + 4096;
        const inputPrice = Math.max(model.cost.input, model.cost.cacheRead, model.cost.cacheWrite);
        const costBound = Math.ceil(inputBound * inputPrice + maxOutput * model.cost.output);
        const permit = await peer.call('model.permit', { call_id: callId, max_output_tokens: maxOutput, input_tokens_bound: String(inputBound), cost_microusd_bound: String(costBound), ...(compactionId ? { compaction_id: compactionId } : {}) }, options?.signal);
        permitId = permit.id;
        await peer.call('model.dispatch', { id: permit.id }, options?.signal);
        const requestOptions = { ...options, env: environment, maxTokens: permit.max_output_tokens, maxRetries: 0, timeoutMs: 30_000, maxRetryDelayMs: 1000 };
        if (streamOverride) return await streamOverride(currentModel, context, requestOptions);
        // Pi's full stream API exposes provider-native tool choice. Its
        // simple API permits only auto/none, which can omit our finish tool.
        return models.stream(currentModel, context, {
          ...requestOptions,
          toolChoice: currentModel.api === 'anthropic-messages' ? 'any' : 'required',
          ...(currentModel.reasoning && currentModel.api !== 'anthropic-messages' ? { reasoningEffort: 'medium' } : {}),
        });
      } catch (error) {
        if (!permitId) {
          // A lost reservation response can still leave a committed permit.
          // Recover that identity without requesting another allowance, even
          // when the caller's signal has already been cancelled.
          try { permitId = (await peer.call('model.permit.lookup', { id: callId })).id; }
          catch { /* No observed permit or closed transport: retain host state. */ }
        }
        if (permitId) {
          // Only the host can establish that dispatch never happened. A lost
          // dispatch response keeps its reservation, even if no usage arrives.
          try { await peer.call('model.release', { id: permitId }); permitId = undefined; }
          catch { /* Dispatched or unreachable: retain the unknown liability. */ }
        }
        failure = error instanceof RpcError ? error : new RpcError(-32603, error instanceof Error ? error.message : 'provider failed');
        return failedStream(currentModel, failure.message, options?.signal?.aborted ?? false);
      }
    };

    return {
      streamFn,
      get failure() { return failure; },
      get completedPermitId() { return completedPermitId; },
      async settle(message: AssistantMessage) {
        if (!permitId) return;
        const usage = message.usage;
        const complete = !['error', 'aborted'].includes(message.stopReason);
        await peer.call('model.usage', { permit_id: permitId, input_tokens: String(usage.input + usage.cacheRead + usage.cacheWrite), output_tokens: String(usage.output), cost_microusd: String(Math.ceil(usage.cost.total * 1_000_000)), complete });
        if (complete) completedPermitId = permitId;
        permitId = undefined;
      },
    };
  } };
}

function failedStream(model: Model<Api>, message: string, aborted: boolean) {
  const stream = createAssistantMessageEventStream();
  const result: AssistantMessage = { role: 'assistant', content: [], api: model.api, provider: model.provider, model: model.id, usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } }, stopReason: aborted ? 'aborted' : 'error', errorMessage: message, timestamp: Date.now() };
  stream.push({ type: 'error', reason: result.stopReason as 'error' | 'aborted', error: result });
  return stream;
}
