import { Agent, type AgentMessage, type StreamFn } from '@earendil-works/pi-agent-core';
import { createAssistantMessageEventStream, createModels, type AssistantMessage, type Model, type Api } from '@earendil-works/pi-ai';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { anthropicProvider } from '@earendil-works/pi-ai/providers/anthropic';
import { azureOpenAIResponsesProvider } from '@earendil-works/pi-ai/providers/azure-openai-responses';
import type { AgentResult, AgentRunRequest, Checkpoint } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { RpcError, validate } from '../client/validation.js';
import { systemPrompt } from '../profiles/index.js';
import { createTools } from '../tools/index.js';
import { boundedContext } from './context.js';
import { publishActivity } from './activity.js';
import { providerEnvironment } from './provider.js';
import { operatorFor } from '../operators/index.js';

export interface AgentExecution {
  run(request: AgentRunRequest): Promise<AgentResult>;
  cancel(): void;
  steer(text: string): void;
}

/** Test streams can be injected at Pi's existing provider boundary. The worker
 * entrypoint always uses the real Pi AI provider transport. */
export function createAgentExecution(peer: RpcPeer, streamOverride?: StreamFn): AgentExecution {
  let active: Agent | undefined;
  let running = false;
  let cancelled = false;
  return {
    cancel() { cancelled = true; active?.abort(); },
    steer(text) { active?.steer({ role: 'user', content: text, timestamp: Date.now() }); },
    async run(request) {
      if (running) throw new RpcError(-32002, 'worker already has an active run');
      validate('AgentRunRequest', request);
      running = true; cancelled = false;
      let terminal: AgentResult | undefined;
      let permitId: string | undefined;
      let failure: RpcError | undefined;
      try {
        const models = createModels();
        if (request.provider === 'openai') models.setProvider(openaiProvider());
        else if (request.provider === 'anthropic') models.setProvider(anthropicProvider());
        else if (request.provider === 'azure-openai-responses') models.setProvider(azureOpenAIResponsesProvider());
        else throw new RpcError(-32602, 'supported providers are openai, anthropic and azure-openai-responses');
        const model = models.getModel(request.provider, request.model);
        if (!model) throw new RpcError(-32602, 'model absent from pinned Pi provider catalogue');
        const environment = streamOverride ? {} : providerEnvironment(request.provider);
        const grant = await peer.call('session.grant', {});
        const prompt = `${systemPrompt(request.profile, request.operator)}\n\nRun configuration: ${JSON.stringify({ run_id: request.run_id, provider: request.provider, model: request.model, profile: request.profile, operator: request.operator, started_ms: String(Date.now()) })}\n\nHost grant, supplied by the trusted adapter for this run:\n${JSON.stringify(grant)}\nThe grant is permission, not a request to use every capability. Follow the operator and requested task. For a task requesting live repair in apply mode, completion includes applying the checked branch to the live artifact. Discovery and extraction do not request repair merely because edits are permitted. In sandbox mode, keep changes in the branch. In observe mode, do not dispatch effects. The grant is an upper bound; Rust enforces current remaining budgets and deadlines.`;
        const tools = createTools(peer, request.run_id, request.operator, operatorFor(request.profile, request.operator).outputKinds, result => { terminal = result; });
        const streamFn: StreamFn = async (currentModel, context, options) => {
          try {
            const maxOutput = 4096;
            // UTF-8 bytes provide a deliberately conservative token reservation;
            // observed usage replaces it only after a completed provider response.
            const inputBound = Buffer.byteLength(JSON.stringify(context)) * 2 + 4096;
            const inputPrice = Math.max(model.cost.input, model.cost.cacheRead, model.cost.cacheWrite);
            const costBound = Math.ceil(inputBound * inputPrice + maxOutput * model.cost.output);
            const permit = await peer.call('model.permit', { max_output_tokens: maxOutput, input_tokens_bound: String(inputBound), cost_microusd_bound: String(costBound) }, options?.signal);
            permitId = permit.id;
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
            failure = error instanceof RpcError ? error : new RpcError(-32603, error instanceof Error ? error.message : 'provider failed');
            return failedStream(currentModel, failure.message, cancelled);
          }
        };
        const messages = request.checkpoint ? await restoreMessages(peer, request) : [];
        const agent = new Agent({
          initialState: { model, messages, systemPrompt: prompt, tools, thinkingLevel: 'off' },
          streamFn, toolExecution: 'sequential', maxRetryDelayMs: 1000,
          transformContext: async context => boundedContext(context),
          shouldStopAfterTurn: () => terminal !== undefined || cancelled,
        });
        active = agent;
        let eventCursor = request.checkpoint?.event_cursor ?? '0';
        let publishedMessages = 0;
        const checkpoint = async () => {
          const completed = new Set(agent.state.messages.flatMap(m => m.role === 'toolResult' ? [m.toolCallId] : []));
          const pending = agent.state.messages.filter((m): m is AssistantMessage => m.role === 'assistant').flatMap(m => m.content.flatMap(c => c.type === 'toolCall' && c.name === 'action_execute' && !completed.has(c.id) ? [`${request.run_id}/${c.id}`] : []));
          const value: Checkpoint = { format: 'pi-0.85.1/1', profile: request.profile, operator: request.operator, provider: request.provider, model: request.model, messages: agent.state.messages as unknown as Array<Record<string, unknown>>, pending_operations: pending, event_cursor: eventCursor };
          await peer.call('session.checkpoint', value);
          await publishActivity(peer, agent.state.messages, publishedMessages);
          publishedMessages = agent.state.messages.length;
        };
        agent.subscribe(async event => {
          if (event.type === 'tool_execution_end' && event.toolName === 'evidence_read' && !event.isError && typeof event.result.details?.cursor === 'string') eventCursor = event.result.details.cursor;
          if (event.type === 'message_end' && event.message.role === 'assistant') {
            if (permitId) {
              const usage = event.message.usage;
              await peer.call('model.usage', { permit_id: permitId, input_tokens: String(usage.input + usage.cacheRead + usage.cacheWrite), output_tokens: String(usage.output), cost_microusd: String(Math.ceil(usage.cost.total * 1_000_000)), complete: !['error','aborted'].includes(event.message.stopReason) });
              permitId = undefined;
            }
            await checkpoint();
          } else if (event.type === 'tool_execution_end' || event.type === 'turn_end' || event.type === 'agent_end') {
            await checkpoint();
          }
        });
        if (request.checkpoint && messages.length && messages.at(-1)?.role !== 'assistant') await agent.continue();
        else await agent.prompt(request.checkpoint ? `Continue the bounded investigation. Previous host receipts remain authoritative. ${request.prompt}` : request.prompt);
        if (cancelled) return { disposition: 'cancelled', summary: 'Agent cancelled; consult durable host receipts for any dispatched effects.' };
        if (failure) return { disposition: failure.code === -32005 ? 'exhausted' : 'failed', summary: failure.message };
        if (agent.state.errorMessage) return { disposition: 'failed', summary: agent.state.errorMessage };
        return terminal ?? { disposition: 'abstained', summary: 'Agent ended without a structured completion. No successful intervention is claimed.' };
      } catch (error) {
        return { disposition: cancelled ? 'cancelled' : 'failed', summary: error instanceof Error ? error.message : 'agent failed' };
      } finally { active = undefined; running = false; }
    },
  };
}

async function restoreMessages(peer: RpcPeer, request: AgentRunRequest): Promise<AgentMessage[]> {
  const checkpoint = request.checkpoint!;
  if (checkpoint.profile !== request.profile || checkpoint.operator !== request.operator || checkpoint.provider !== request.provider || checkpoint.model !== request.model) throw new RpcError(-32002, 'checkpoint configuration mismatch');
  const messages = structuredClone(checkpoint.messages) as unknown as AgentMessage[];
  for (const message of messages) if (!['user','assistant','toolResult'].includes(message.role)) throw new RpcError(-32602, 'unsupported Pi checkpoint message');
  const assistant = messages.findLast((m): m is AssistantMessage => m.role === 'assistant');
  if (!assistant) return messages;
  const after = messages.slice(messages.indexOf(assistant) + 1);
  for (const call of assistant.content.filter(c => c.type === 'toolCall')) {
    if (after.some(m => m.role === 'toolResult' && m.toolCallId === call.id)) continue;
    let text = 'This tool had no acknowledged result at interruption. Reinspect current state before choosing a new action.';
    let isError = true;
    if (call.name === 'action_execute') {
      try {
        const receipt = await peer.call('action.lookup', { id: `${request.run_id}/${call.id}` });
        if (receipt.status === 'unknown' || receipt.status === 'started') throw new RpcError(-32002, 'unreconciled effect prevents continuation');
        text = JSON.stringify(receipt); isError = receipt.status !== 'succeeded';
      } catch (error) {
        if (!(error instanceof RpcError) || error.code !== -32004) throw error;
        text = 'Rust has no intent for this operation: the effect was not dispatched. Inspect current state before requesting it again.';
      }
    }
    messages.push({ role: 'toolResult', toolCallId: call.id, toolName: call.name, content: [{ type: 'text', text }], isError, timestamp: Date.now() });
  }
  return messages;
}

function failedStream(model: Model<Api>, message: string, aborted: boolean) {
  const stream = createAssistantMessageEventStream();
  const result: AssistantMessage = { role: 'assistant', content: [], api: model.api, provider: model.provider, model: model.id, usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } }, stopReason: aborted ? 'aborted' : 'error', errorMessage: message, timestamp: Date.now() };
  stream.push({ type: 'error', reason: result.stopReason as 'error' | 'aborted', error: result });
  return stream;
}
