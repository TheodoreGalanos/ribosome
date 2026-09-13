import { Agent, type AgentMessage, type StreamFn } from '@earendil-works/pi-agent-core';
import { createHash } from 'node:crypto';
import type { AssistantMessage } from '@earendil-works/pi-ai';
import type { AgentResult, AgentRunRequest, Checkpoint } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { RpcError, validate } from '../client/validation.js';
import { systemPrompt } from '../profiles/index.js';
import { createTools } from '../tools/index.js';
import { DurableContext } from './context.js';
import { modelTransport } from './model.js';
import { compactContext } from './compaction.js';
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
      try {
        const transport = modelTransport(request, streamOverride);
        const { model } = transport;
        const meter = transport.metered(peer);
        const grant = await peer.call('session.grant', {});
        const prepared = request.invocation ? await peer.call('invocation.read', {}) : undefined;
        const prompt = `${systemPrompt(request.profile, request.operator)}\n\nRun configuration: ${JSON.stringify({ run_id: request.run_id, provider: request.provider, model: request.model, profile: request.profile, operator: request.operator, started_ms: String(Date.now()) })}\n\nHost grant, supplied by the trusted adapter for this run:\n${JSON.stringify(grant)}\nThe grant is permission, not a request to use every capability. Follow the operator and requested task. For a task requesting live repair in apply mode, completion includes applying the checked branch to the live artifact. Discovery and extraction do not request repair merely because edits are permitted. In sandbox mode, keep changes in the branch. In observe mode, do not dispatch effects. The grant is an upper bound; Rust enforces current remaining budgets and deadlines.${prepared ? `\n\nHost-selected prepared execution (task policy beneath the owner instructions and grant above):\n${JSON.stringify(prepared)}\nInterpret this conditional policy against fresh recipient observations. The recorded donor outcomes are not observations of this recipient.` : ''}`;
        const waitingOn = new Set<string>();
        const tools = createTools(peer, request.run_id, request.operator, operatorFor(request.profile, request.operator).outputKinds, result => { terminal = result; }, ids => {
          for (const id of ids) waitingOn.add(id);
          if (waitingOn.size > 16) throw new RpcError(-32005, 'a parent can wait for at most sixteen children per continuation');
        });
        const opened = await DurableContext.open(peer);
        const durable = opened.context;
        const messages = await restoreMessages(peer, request, opened.messages);
        let contextFailure: unknown;
        const agent = new Agent({
          initialState: { model, messages, systemPrompt: prompt, tools, thinkingLevel: 'off' },
          streamFn: meter.streamFn, toolExecution: 'sequential', maxRetryDelayMs: 1000,
          transformContext: async (context, signal) => {
            try {
              const authorize = async () => {
                const authorized = await durable.authorize(context);
                if (authorized.rebuilt) {
                  // Pi keeps a loop snapshot separate from public state.
                  context.splice(0, context.length, ...authorized.messages);
                  agent.state.messages = authorized.messages;
                }
                return authorized;
              };
              await authorize();
              for (;;) {
                try {
                  const { plan } = await peer.call('session.compaction.prepare', {}, signal);
                  if (!plan) break;
                  await compactContext(peer, transport, plan, signal);
                } catch (error) {
                  if (error instanceof RpcError && [-32001, -32002].includes(error.code)) {
                    if ((await authorize()).rebuilt) continue;
                  }
                  throw error;
                }
              }
              const authorized = await authorize();
              return await durable.providerMessages(authorized.messages);
            } catch (error) { contextFailure = error; throw error; }
          },
          shouldStopAfterTurn: () => terminal !== undefined || cancelled || waitingOn.size > 0,
        });
        active = agent;
        let eventCursor = request.checkpoint?.event_cursor ?? '0';
        const checkpoint = async () => {
          await durable.save(agent.state.messages);
          const completed = new Set(agent.state.messages.flatMap(m => m.role === 'toolResult' ? [m.toolCallId] : []));
          const pending = agent.state.messages.filter((m): m is AssistantMessage => m.role === 'assistant').flatMap(m => m.content.flatMap(c => c.type === 'toolCall' && c.name === 'action_execute' && !completed.has(c.id) ? [`${request.run_id}/${c.id}`] : []));
          const value: Checkpoint = { format: 'pi-0.85.1/2', profile: request.profile, operator: request.operator, provider: request.provider, model: request.model, messages: [], context: durable.state, pending_operations: pending, event_cursor: eventCursor };
          await peer.call('session.checkpoint', value);
          return value;
        };
        agent.subscribe(async event => {
          if (event.type === 'tool_execution_end' && event.toolName === 'evidence_read' && !event.isError && typeof event.result.details?.cursor === 'string') eventCursor = event.result.details.cursor;
          if (event.type === 'message_end' && event.message.role === 'assistant') {
            await meter.settle(event.message);
            await checkpoint();
          } else if (event.type === 'tool_execution_end' || event.type === 'turn_end' || event.type === 'agent_end') {
            await checkpoint();
          }
        });
        if (messages.length && messages.at(-1)?.role !== 'assistant') await agent.continue();
        else await agent.prompt(request.checkpoint ? `Continue the bounded investigation. Previous host receipts remain authoritative. ${request.prompt}` : request.prompt);
        if (cancelled) return { disposition: 'cancelled', summary: 'Agent cancelled; consult durable host receipts for any dispatched effects.' };
        if (contextFailure) throw contextFailure;
        const failure = meter.failure;
        if (failure) return { disposition: failure.code === -32005 ? 'exhausted' : 'failed', summary: failure.message };
        if (agent.state.errorMessage) return { disposition: 'failed', summary: agent.state.errorMessage };
        if (!terminal && waitingOn.size) {
          const value = await checkpoint();
          // Parking transfers the continuation to Rust. The supervisor commits
          // the wait and closes this worker before releasing its execution slot.
          await peer.call('session.park', { work_ids: [...waitingOn], checkpoint: value });
          throw new RpcError(-32603, 'parking handoff returned without closing the worker');
        }
        return terminal ?? { disposition: 'abstained', summary: 'Agent ended without a structured completion. No successful intervention is claimed.' };
      } catch (error) {
        return { disposition: cancelled ? 'cancelled' : error instanceof RpcError && error.code === -32005 ? 'exhausted' : 'failed', summary: error instanceof Error ? error.message : 'agent failed' };
      } finally { active = undefined; running = false; }
    },
  };
}

async function restoreMessages(peer: RpcPeer, request: AgentRunRequest, messages: AgentMessage[]): Promise<AgentMessage[]> {
  const checkpoint = request.checkpoint;
  if (checkpoint && (checkpoint.profile !== request.profile || checkpoint.operator !== request.operator || checkpoint.provider !== request.provider || checkpoint.model !== request.model)) throw new RpcError(-32002, 'checkpoint configuration mismatch');
  for (const message of messages) if (!['user','assistant','toolResult'].includes(message.role)) throw new RpcError(-32602, 'unsupported Pi checkpoint message');
  const assistant = messages.findLast((m): m is AssistantMessage => m.role === 'assistant');
  if (!assistant) return messages;
  const after = messages.slice(messages.indexOf(assistant) + 1);
  for (const call of assistant.content.filter(c => c.type === 'toolCall')) {
    if (after.some(m => m.role === 'toolResult' && m.toolCallId === call.id)) continue;
    let text = 'This tool had no acknowledged result at interruption. Reinspect current state before choosing a new action.';
    let isError = true;
    if (call.name !== 'action_execute') {
      try {
        const { content, ...details } = await peer.call('tool.result', { id: call.id });
        messages.push({ role: 'toolResult', toolCallId: call.id, toolName: call.name, content: [{ type: 'text', text: content }], details, isError: false, timestamp: Date.now() });
        continue;
      } catch (error) {
        if (!(error instanceof RpcError) || ![-32001, -32004].includes(error.code)) throw error;
        if (error.code === -32001) text = 'The retained observation is no longer authorized. Reinspect permitted evidence.';
      }
    }
    if (call.name === 'action_execute') {
      try {
        const receipt = await peer.call('action.lookup', { id: `${request.run_id}/${call.id}` });
        if ((receipt.status === 'unknown' && !receipt.settlement) || receipt.status === 'started') throw new RpcError(-32002, 'unreconciled effect prevents continuation');
        // Retain the reconciled receipt through the same observation boundary.
        const recoveryId = `reconciled/${createHash('sha256').update(call.id).digest('hex')}`;
        const { content, ...details } = await peer.call('tool.call', { call_id: recoveryId, method: 'action.lookup', arguments: { id: `${request.run_id}/${call.id}` } });
        messages.push({ role: 'toolResult', toolCallId: call.id, toolName: call.name, content: [{ type: 'text', text: content }], details, isError: receipt.status !== 'succeeded', timestamp: Date.now() });
        continue;
      } catch (error) {
        if (!(error instanceof RpcError) || error.code !== -32004) throw error;
        text = 'Rust has no intent for this operation: the effect was not dispatched. Inspect current state before requesting it again.';
      }
    }
    messages.push({ role: 'toolResult', toolCallId: call.id, toolName: call.name, content: [{ type: 'text', text }], isError, timestamp: Date.now() });
  }
  return messages;
}
