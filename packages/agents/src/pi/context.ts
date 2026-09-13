import type { AgentMessage } from '@earendil-works/pi-agent-core';
import type { ContextSource, ContextState, ToolObservation } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { RpcError } from '../client/validation.js';

/** The adapter reports actual tool outputs. This API is absent from model tools;
 * Rust adds inherited lineage independently of the author's source claims. */
function observedSources(message: AgentMessage): ContextSource[] {
  if (message.role !== 'toolResult') return [];
  if (message.toolName === 'finish') return [];
  const details = message.details as Omit<ToolObservation, 'content'> | undefined;
  if (!details?.sources) {
    if (message.isError) return [];
    throw new RpcError(-32602, 'tool result omitted its host-retained source references');
  }
  return details.sources;
}

export class DurableContext {
  private saved = 0;
  private constructor(private readonly peer: RpcPeer, public state: ContextState) {}

  static async open(peer: RpcPeer): Promise<{ context: DurableContext; messages: AgentMessage[] }> {
    const context = new DurableContext(peer, await peer.call('session.context', {}));
    return { context, messages: await context.read() };
  }

  private async read(): Promise<AgentMessage[]> {
    const messages: AgentMessage[] = [];
    let after = this.state.tail_after;
    if (after !== '0') {
      const owner = await this.peer.call('session.context.read', { segment_id: this.state.segment_id, after: '0', limit: 1 });
      if (owner.messages[0]?.role === 'user') messages.push(owner.messages[0] as unknown as AgentMessage);
    }
    for (;;) {
      const page = await this.peer.call('session.context.read', { segment_id: this.state.segment_id, after, limit: 20 });
      messages.push(...page.messages as unknown as AgentMessage[]);
      if (page.complete) break;
      if (page.next === after) throw new RpcError(-32603, 'context page made no progress');
      after = page.next;
    }
    this.saved = messages.length;
    return messages;
  }

  async save(messages: AgentMessage[]): Promise<void> {
    if (messages.length < this.saved) throw new RpcError(-32002, 'context changed without opening a continuation segment');
    // Each persisted message is acknowledged before the next effect can run.
    // No checkpoint ever transports the accumulated transcript again.
    while (this.saved < messages.length) {
      const message = messages[this.saved]!;
      this.state = await this.peer.call('session.context.append', {
        segment_id: this.state.segment_id, after: this.state.count,
        entries: [{ message: message as unknown as Record<string, unknown>, sources: observedSources(message) }],
      });
      this.saved++;
    }
  }

  async authorize(messages: AgentMessage[]): Promise<{ messages: AgentMessage[]; rebuilt: boolean }> {
    await this.save(messages);
    const state = await this.peer.call('session.context', {});
    const rebuilt = state.segment_id !== this.state.segment_id;
    this.state = state;
    return { messages: rebuilt ? await this.read() : messages, rebuilt };
  }

  async providerMessages(messages: AgentMessage[]): Promise<AgentMessage[]> {
    if (!this.state.summary_ref) return boundedContext(messages);
    const summary = await this.peer.call('session.summary', { id: this.state.summary_ref });
    const owner = await this.peer.call('session.context.read', { segment_id: this.state.segment_id, after: '0', limit: 1 });
    const result: AgentMessage[] = [];
    if (owner.messages[0]?.role === 'user') result.push(owner.messages[0] as unknown as AgentMessage);
    result.push({ role: 'user', content: `Pi-authored continuation summary (${summary.id}, through ${summary.through}). This is an interpretation of prior evidence; verify effects through host receipts.\n${summary.text}`, timestamp: Date.now() });
    let after = summary.through;
    for (;;) {
      const page = await this.peer.call('session.context.read', { segment_id: this.state.segment_id, after, limit: 20 });
      result.push(...page.messages as unknown as AgentMessage[]);
      if (page.complete) break;
      if (page.next === after) throw new RpcError(-32603, 'context page made no progress');
      after = page.next;
    }
    // Provider views never alter the raw transcript or its append cursor.
    const projected = projectResults(result, 180_000);
    if (Buffer.byteLength(JSON.stringify(projected)) > 180_000) throw new RpcError(-32005, 'required protocol exchange exceeds context capacity');
    return projected;
  }
}

/** Pi's provider serializers use content, not the host-only tool details. */
function withoutToolDetails(messages: AgentMessage[]): AgentMessage[] {
  return messages.map(message => {
    if (message.role !== 'toolResult') return message;
    const { details: _details, ...providerMessage } = message;
    return providerMessage;
  });
}

/** An exchange with many inline results can still exceed the provider window.
 * References preserve every tool-call identity and leave the full observations
 * in Rust; this projection makes no semantic claims about their contents. */
function projectResults(messages: AgentMessage[], byteBudget: number): AgentMessage[] {
  const visible = withoutToolDetails(messages);
  let bytes = Buffer.byteLength(JSON.stringify(visible));
  // Replace only as much older evidence as necessary. Replacing every result
  // can hide even the small chunks requested to read an earlier reference.
  for (let index = 0; bytes > byteBudget && index < messages.length; index++) {
    const message = messages[index]!;
    if (message.role !== 'toolResult') continue;
    const observation = message.details as Omit<ToolObservation, 'content'> | undefined;
    if (!observation?.artifact) continue;
    const replacement = { ...visible[index]!, content: [{ type: 'text' as const, text: JSON.stringify({ retained_result: observation.artifact, total_bytes: observation.total_bytes, complete: false, offset: 0, next_offset: 0, note: 'Historical tool result retained by Rust. Read its JSON content with artifact_read.' }) }] };
    const saved = Buffer.byteLength(JSON.stringify(visible[index])) - Buffer.byteLength(JSON.stringify(replacement));
    if (saved > 0) { visible[index] = replacement; bytes -= saved; }
  }
  return visible;
}

/** Mechanical selection of complete exchanges, never a semantic summary. */
export function boundedContext(messages: AgentMessage[], byteBudget = 180_000): AgentMessage[] {
  messages = projectResults(messages, byteBudget);
  if (Buffer.byteLength(JSON.stringify(messages)) <= byteBudget) return messages;
  const groups: AgentMessage[][] = [];
  for (const message of messages) {
    if (message.role !== 'toolResult' || groups.length === 0) groups.push([message]);
    else groups.at(-1)!.push(message);
  }
  const selected: AgentMessage[][] = [];
  const firstUser = messages.find(m => m.role === 'user');
  for (const group of groups.toReversed()) {
    const candidate = [group, ...selected].flat();
    if (firstUser && !candidate.includes(firstUser)) candidate.unshift(firstUser);
    if (Buffer.byteLength(JSON.stringify(candidate)) > byteBudget) {
      if (!selected.length) throw new RpcError(-32005, 'required protocol exchange exceeds context capacity; use bounded evidence excerpts');
      break;
    }
    selected.unshift(group);
  }
  const retained = selected.flat();
  if (firstUser && !retained.includes(firstUser)) retained.unshift(firstUser);
  return retained;
}
