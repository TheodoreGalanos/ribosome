// Infrastructure fixture: the actual Pi loop with a scripted provider boundary.
// This is not a live-model behavioral evaluation and is never a production mode.
import { createAgentExecution } from '../../packages/agents/dist/pi/agent.js';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
const peer=new RpcPeer(process.stdin,process.stdout);
let step=0;
let version;
const execution=createAgentExecution(peer,async(model,context)=>{
  const previous=context.messages.at(-1);
  if(previous?.role==='toolResult'&&previous.toolName==='artifact_read')version=JSON.parse(previous.content[0].text).artifact.version;
  const calls=[
    ['evidence_read',{cursor:'0',limit:10}],
    ['artifact_read',{path:'report.txt',offset:0,length:1000}],
    ['action_execute',{kind:'edit',path:'report.txt',expected_version:version,content:'corrected'}],
    ['action_execute',{kind:'check',tool:'report-check'}],
    ['finish',{disposition:'completed',summary:'Fixture consumed evidence, edited through Rust and inspected a check receipt.'}],
  ];
  const [name,args]=calls[Math.min(step,calls.length-1)];
  const content=[{type:'toolCall',id:`call-${step++}`,name,arguments:args}];
  const message={role:'assistant',api:model.api,provider:model.provider,model:model.id,content,stopReason:'toolUse',timestamp:Date.now(),usage:{input:10,output:10,cacheRead:0,cacheWrite:0,totalTokens:20,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}}};
  const stream=createAssistantMessageEventStream();stream.push({type:'done',reason:'toolUse',message});return stream;
});
peer.handle('bridge.hello',async h=>h);
peer.handle('agent.run',async request=>execution.run(request));
peer.handle('agent.cancel',async()=>{execution.cancel();return{ok:true};});
peer.onClose=()=>{execution.cancel();process.stdin.destroy();};
