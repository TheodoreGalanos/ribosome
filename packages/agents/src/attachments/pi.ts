import { randomUUID } from 'node:crypto';
import type { Agent, AgentEvent, AgentMessage } from '@earendil-works/pi-agent-core';
import type { ArtifactRef, HarnessEvent } from '../generated/contracts.js';
import { activityMessage } from '../pi/activity.js';
import { bounded } from './coordinator.js';
import { AttachmentClient, type AttachOptions, type HarnessAttachment } from './client.js';

export interface PiAttachOptions extends Omit<AttachOptions, 'source' | 'connector' | 'connectorVersion' | 'steer'> {
  allowSteering?: boolean;
  captureMessages?: boolean;
  captureToolContent?: boolean;
  artifacts?(event: AgentEvent): Promise<ArtifactRef[]> | ArtifactRef[];
  redact?(event: HarnessEvent): HarnessEvent | undefined;
}

/** Instrument an application-owned Agent; never starts or replaces its loop. */
export async function attachPi(client: AttachmentClient, agent: Agent, options: PiAttachOptions): Promise<HarnessAttachment> {
  const producer = `pi-${randomUUID()}`;
  const steeringMessages = new WeakSet<AgentMessage>();
  const toolStarts = new Map<string, string>();
  let sequence = 0;
  let attached: HarnessAttachment | undefined;
  let adapterFailure: Error | undefined;
  const { allowSteering, captureMessages, captureToolContent, artifacts, redact, ...base } = options;
  const source = {
    subscribe(emit: (event: HarnessEvent) => Promise<void>) {
      return agent.subscribe(async event => {
        try {
          const id = randomUUID();
          const payload: Record<string, unknown> = {};
          const parents: string[] = [];
          let kind: string;
          let correlation = options.executionId;
          switch (event.type) {
            case 'agent_start': kind = 'execution.started'; break;
            case 'agent_end': kind = agent.state.errorMessage ? 'execution.failed' : 'execution.completed'; break;
            case 'tool_execution_start':
              kind = 'tool.started'; correlation = event.toolCallId;
              toolStarts.set(event.toolCallId, id); payload.tool = event.toolName;
              break;
            case 'tool_execution_end': {
              kind = 'tool.completed'; correlation = event.toolCallId;
              const start = toolStarts.get(event.toolCallId);
              if (start) parents.push(start);
              toolStarts.delete(event.toolCallId);
              payload.tool = event.toolName; payload.is_error = event.isError;
              if (captureToolContent) payload.result = event.result;
              break;
            }
            case 'message_end':
              if (!captureMessages || steeringMessages.has(event.message)) return;
              kind = 'message.completed';
              payload.message = activityMessage(event.message, sequence).content;
              payload.role = event.message.role;
              break;
            default: return;
          }
          let normalized: HarnessEvent | undefined = { id, producer, sequence: String(sequence + 1), kind,
            timestamp_ms: String(Date.now()), parents, correlation, artifacts: artifacts ? await bounded(Promise.resolve(artifacts(event)), 1000, 'Artifact observation timed out; source coverage interrupted') : [], payload };
          if (redact) normalized = redact(normalized);
          if (!normalized) {
            if (event.type === 'tool_execution_start') toolStarts.delete(event.toolCallId);
            return;
          }
          sequence++;
          await emit(normalized);
        } catch (error) {
          adapterFailure = error instanceof Error ? error : new Error(String(error));
          attached?.connectionLost(adapterFailure);
          // Observation failure must not abort the application-owned Agent.
          // The attachment retains the error and finish() will report it.
        }
      });
    },
  };
  attached = await client.attach({ ...base, connector: 'pi-agent-core', connectorVersion: '0.85.1', source,
    ...(allowSteering ? { steer: (text: string) => {
      const message: AgentMessage = { role: 'user', content: text, timestamp: Date.now() };
      steeringMessages.add(message); agent.steer(message);
    } } : {}),
  });
  if (adapterFailure) attached.connectionLost(adapterFailure);
  return attached;
}
