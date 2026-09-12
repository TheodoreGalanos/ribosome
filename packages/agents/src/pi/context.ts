import type { AgentMessage } from '@earendil-works/pi-agent-core';

/** Keep complete assistant/tool exchanges. Durable checkpoints keep the full
 * transcript in Rust; context reduction only changes the next model request. */
export function boundedContext(messages: AgentMessage[], byteBudget = 180_000): AgentMessage[] {
  if (Buffer.byteLength(JSON.stringify(messages)) <= byteBudget) return messages;
  const groups: AgentMessage[][] = [];
  for (const message of messages) {
    if (message.role !== 'toolResult' || groups.length === 0) groups.push([message]);
    else groups.at(-1)!.push(message);
  }
  const selected: AgentMessage[][] = [];
  let size = 0;
  for (const group of groups.toReversed()) {
    const bytes = Buffer.byteLength(JSON.stringify(group));
    if (selected.length && size + bytes > byteBudget) break;
    selected.unshift(group); size += bytes;
  }
  const retained = selected.flat();
  const firstUser = messages.find(m => m.role === 'user');
  if (firstUser && !retained.includes(firstUser)) retained.unshift(firstUser);
  return retained;
}
