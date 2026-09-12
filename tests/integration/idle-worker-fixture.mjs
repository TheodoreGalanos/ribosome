import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
const peer=new RpcPeer(process.stdin,process.stdout);let settle;
peer.handle('bridge.hello',async h=>h);
peer.handle('agent.run',async()=>new Promise(resolve=>settle=resolve));
peer.handle('agent.cancel',async()=>{settle?.({disposition:'cancelled',summary:'Cancellation acknowledged.'});return{ok:true};});
peer.onClose=()=>process.stdin.destroy();
