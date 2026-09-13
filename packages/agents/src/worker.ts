import { RpcPeer } from './client/rpc.js';
import { RpcError } from './client/validation.js';
import { createAgentExecution } from './pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const execution = createAgentExecution(peer);
let session: string | undefined;
peer.onClose = error => { execution.cancel(); process.stderr.write(`Ribosome worker bridge closed: ${error.message}\n`); process.exitCode = 1; process.stdin.destroy(); };
peer.handle('bridge.hello', async hello => {
  if (session) throw new RpcError(-32002, 'worker already bound to a session');
  if (!hello.capabilities.includes('context.v2')) throw new RpcError(-32002, 'host lacks context.v2 continuation authorization');
  if (!hello.capabilities.includes('context.sources/4')) throw new RpcError(-32002, 'host lacks context.sources/4 source lineage authorization');
  if (!hello.capabilities.includes('context.compaction/1')) throw new RpcError(-32002, 'host lacks context.compaction/1 semantic continuation');
  if (!hello.capabilities.includes('context.results/1')) throw new RpcError(-32002, 'host lacks context.results/1 retained tool observations');
  if (!hello.capabilities.includes('budget.allocations/1')) throw new RpcError(-32002, 'host lacks budget.allocations/1 provider accounting');
  if (!hello.capabilities.includes('context.parking/1')) throw new RpcError(-32002, 'host lacks context.parking/1 parent continuation');
  session = hello.session;
  return { protocol: 'ribosome/1', build: '0.1.0', pi: '0.85.1', session, capabilities: ['agent.run','agent.cancel','agent.steer','context.v2','context.sources/4','context.compaction/1','context.results/1','budget.allocations/1','context.parking/1'] };
});
peer.handle('agent.run', async request => {
  if (!session) throw new RpcError(-32001, 'handshake required');
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.handle('agent.steer', async message => { execution.steer(message.body); return { ok: true }; });
