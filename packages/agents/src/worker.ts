import { RpcPeer } from './client/rpc.js';
import { RpcError } from './client/validation.js';
import { createAgentExecution } from './pi/agent.js';

const peer = new RpcPeer(process.stdin, process.stdout);
const execution = createAgentExecution(peer);
let session: string | undefined;
peer.onClose = error => { execution.cancel(); process.stderr.write(`Ribosome worker bridge closed: ${error.message}\n`); process.exitCode = 1; process.stdin.destroy(); };
peer.handle('bridge.hello', async hello => {
  if (session) throw new RpcError(-32002, 'worker already bound to a session');
  session = hello.session;
  return { protocol: 'ribosome/1', build: '0.1.0', pi: '0.85.1', session, capabilities: ['agent.run','agent.cancel','agent.steer'] };
});
peer.handle('agent.run', async request => {
  if (!session) throw new RpcError(-32001, 'handshake required');
  return execution.run(request);
});
peer.handle('agent.cancel', async () => { execution.cancel(); return { ok: true }; });
peer.handle('agent.steer', async message => { execution.steer(message.body); return { ok: true }; });
