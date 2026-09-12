import { spawnSync,spawn } from 'node:child_process';
import { PassThrough } from 'node:stream';
import { RpcPeer } from '../packages/agents/dist/client/rpc.js';
import { performance } from 'node:perf_hooks';

const startups=[];
for(let i=0;i<5;i++){
  const start=performance.now();const run=spawnSync(process.execPath,['--input-type=module','-e',"await import('./packages/agents/dist/index.js')"],{encoding:'utf8'});
  if(run.status!==0)throw Error(run.stderr);startups.push(performance.now()-start);
}
const a=new PassThrough(),b=new PassThrough(),host=new RpcPeer(a,b),worker=new RpcPeer(b,a);
host.handle('message.ack',async()=>({ok:true}));const start=performance.now();for(let i=0;i<1000;i++)await worker.call('message.ack',{id:String(i)});const roundtrips=performance.now()-start;host.close();worker.close();a.destroy();b.destroy();
const pipeStart=performance.now();const child=spawn(process.execPath,['packages/agents/dist/worker.js'],{stdio:['pipe','pipe','ignore']});const peer=new RpcPeer(child.stdout,child.stdin);
await peer.call('bridge.hello',{protocol:'ribosome/1',build:'0.1.0',pi:'0.85.1',session:'timing',capabilities:['agent.run','agent.cancel','agent.steer']});const ready=performance.now()-pipeStart;
const rpcStart=performance.now();for(let i=0;i<1000;i++)await peer.call('agent.cancel',{});const pipeRoundtrips=performance.now()-rpcStart;peer.close();child.kill();await new Promise(resolve=>child.once('exit',resolve));
const storage=spawnSync('cargo',['run','--quiet','--locked','-p','ribosome-core','--example','measure'],{encoding:'utf8'});if(storage.status!==0)throw Error(storage.stderr);
console.log(JSON.stringify({kind:'infrastructure-only',node_startup_and_package_import_ms:startups,bridge:{transport:'in-memory Node streams',round_trips:1000,total_ms:roundtrips,mean_ms:roundtrips/1000},worker_pipe:{startup_and_handshake_ms:ready,round_trips:1000,total_ms:pipeRoundtrips,mean_ms:pipeRoundtrips/1000},sqlite:JSON.parse(storage.stdout)},null,2));
