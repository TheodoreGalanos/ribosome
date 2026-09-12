import type { AgentMessage } from '@earendil-works/pi-agent-core';
import type { AgentActivity } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { RpcError } from '../client/validation.js';

/** Render complete visible messages for curation. Provider reasoning and raw
 * media stay outside this evidence view; the full continuation stays in Rust. */
export function activityMessage(message: AgentMessage, index: number, sourceRefs: string[] = []): AgentActivity {
  if (!['user', 'assistant', 'toolResult'].includes(message.role)) throw new RpcError(-32602, 'unsupported activity message role');
  const typed = message as { role: 'user' | 'assistant' | 'toolResult'; content: string | Array<Record<string, unknown>>; timestamp?: number };
  const content = typeof typed.content === 'string' ? [{ type: 'text', text: typed.content }] : typed.content.flatMap<Record<string, unknown>>(part => {
    if (part.type === 'thinking') return [];
    if (part.type === 'text') return [{ type: 'text', text: part.text }];
    if (part.type === 'toolCall') return [{ type: 'toolCall', id: part.id, name: part.name, arguments: part.arguments }];
    return [{ type: part.type, omitted: true, reason: 'Non-text content is retained in the checkpoint.' }];
  });
  const timestamp = typed.timestamp;
  if (typeof timestamp !== 'number' || !Number.isSafeInteger(timestamp) || timestamp < 0) throw new RpcError(-32602, 'Pi activity requires a valid message timestamp');
  return {
    sequence: String(index + 1), role: typed.role === 'toolResult' ? 'tool_result' : typed.role,
    timestamp_ms: String(timestamp),
    source_refs: sourceRefs,
    content: { parts: content },
  };
}

export async function publishActivity(peer: RpcPeer, messages: AgentMessage[], start: number): Promise<void> {
  let entries: AgentActivity[] = [];
  let bytes = 0;
  const sources = new Set<string>();
  for (let index = 0; index < messages.length; index++) {
    const message = messages[index]!;
    for (const reference of retrievedReferences(message)) sources.add(reference);
    if (index < start) continue;
    const entry = activityMessage(message, index, [...sources].sort());
    const size = Buffer.byteLength(JSON.stringify(entry));
    if (size > 240 * 1024) throw new RpcError(-32005, 'agent message exceeds evidence capacity; reference large artifacts');
    if (entries.length && (entries.length === 20 || bytes + size > 480 * 1024)) {
      await peer.call('session.events', { entries });
      entries = []; bytes = 0;
    }
    entries.push(entry); bytes += size;
  }
  if (entries.length) await peer.call('session.events', { entries });
}

function retrievedReferences(message: AgentMessage): string[] {
  if (message.role !== 'toolResult' || message.isError) return [];
  const method = message.toolName;
  if (!['record_read', 'record_submit', 'inventory_admission_request', 'search_query', 'evidence_read'].includes(method)) return [];
  const text = message.content.filter(part => part.type === 'text').map(part => part.text).join('');
  let result;
  try { result = JSON.parse(text); } catch { return []; }
  const records = method === 'search_query' ? result.records : method === 'evidence_read' ? result.events : [result];
  return Array.isArray(records) ? records.flatMap(record => typeof record?.id === 'string' ? [record.id] : []) : [];
}
