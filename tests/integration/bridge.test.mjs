import { fixtureModel } from './model-fixture.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';

const root=resolve('.');
async function command(args){return new Promise((resolve,reject)=>{const p=spawn(join(root,'target/debug/ribosome'),args,{env:{PATH:process.env.PATH}});let stdout='',stderr='';p.stdout.on('data',b=>stdout+=b);p.stderr.on('data',b=>stderr+=b);p.on('error',reject);p.on('exit',code=>resolve({code,stdout,stderr}));});}

function faultConfig(directory,worker,deadline=30000){return {workspace:directory,state_dir:join(directory,'state'),node:process.execPath,worker:join(root,'tests/integration',worker),grant:{id:'grant-fault',scope:{client:'test',project:'fault'},mode:'apply',paths:['report.txt'],tools:[],profiles:['caretaker'],budget:{max_calls:8,max_tokens:'2000000',max_cost_microusd:'1000000',max_actions:5,max_work_items:1,max_depth:1,deadline_ms:String(Date.now()+deadline)},context:'fault',visible_splits:['development'],allow_export:false},request:{run_id:'run-fault',profile:'caretaker',operator:'proofreading@1',prompt:join(directory,'report.txt'),provider:'openai',model:fixtureModel('openai').id},tools:{}};}

test('duplex RPC handles tools while the run request is pending',async()=>{
  const a=new PassThrough(),b=new PassThrough();const host=new RpcPeer(a,b),worker=new RpcPeer(b,a);
  host.handle('message.ack',async()=>({ok:true}));
  worker.handle('agent.cancel',async()=>hostResponse());
  async function hostResponse(){return worker.call('message.ack',{id:'one'});}
  assert.deepEqual(await host.call('agent.cancel',{}),{ok:true});host.close();worker.close();a.destroy();b.destroy();
});

test('RPC rejects oversized and malformed envelopes without unbounded buffering',async()=>{
  const input=new PassThrough(),output=new PassThrough();output.resume();const peer=new RpcPeer(input,output);let closed;
  peer.onClose=e=>closed=e;
  input.write(Buffer.alloc(1_048_577,65));assert.match(closed.message,/too large/);input.destroy();output.destroy();
});

test('worker death after an effect and before reading its response resumes without repeating it',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-crash-'));
  try{
    await writeFile(join(directory,'report.txt'),'original');const config=faultConfig(directory,'crash-worker-fixture.mjs');const file=join(directory,'config.json');await writeFile(file,JSON.stringify(config));
    const crashed=await command(['run',file]);assert.equal(crashed.code,2,crashed.stderr);assert.equal(JSON.parse(crashed.stdout).disposition,'interrupted');assert.equal(await readFile(join(directory,'report.txt'),'utf8'),'crash edit completed',crashed.stdout+'\n'+crashed.stderr);
    await writeFile(join(directory,'report.txt'),'subsequent owner edit');
    const resumed=await command(['run',file]);assert.equal(resumed.code,0,resumed.stderr+'\n'+resumed.stdout);assert.equal(JSON.parse(resumed.stdout).disposition,'completed');assert.equal(await readFile(join(directory,'report.txt'),'utf8'),'subsequent owner edit');
  }finally{await rm(directory,{recursive:true,force:true});}
});

test('host deadline cancels a pending worker and records exhausted status',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-cancel-'));
  try{
    await writeFile(join(directory,'report.txt'),'original');const config=faultConfig(directory,'idle-worker-fixture.mjs',1500);const file=join(directory,'config.json');await writeFile(file,JSON.stringify(config));
    const result=await command(['run',file]);assert.equal(result.code,2,result.stderr);assert.equal(JSON.parse(result.stdout).disposition,'exhausted');
  }finally{await rm(directory,{recursive:true,force:true});}
});

test('root deadline also bounds an unresponsive worker handshake', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-handshake-deadline-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'original');
    const config = faultConfig(directory, 'stall-handshake-fixture.mjs', 1500);
    const file = join(directory, 'config.json');
    await writeFile(file, JSON.stringify(config));
    const started = performance.now();
    const result = await command(['run', file]);
    assert.equal(result.code, 2, result.stderr);
    assert.equal(JSON.parse(result.stdout).disposition, 'exhausted');
    assert.ok(performance.now() - started < 3500, 'Handshake must not extend the root deadline to its five-second timeout');
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('production Pi worker fails explicitly when its provider is unconfigured',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-provider-'));
  try{await writeFile(join(directory,'report.txt'),'original');const config=faultConfig(directory,'idle-worker-fixture.mjs');config.worker=join(root,'packages/agents/dist/worker.js');const file=join(directory,'config.json');await writeFile(file,JSON.stringify(config));const result=await command(['run',file]);assert.equal(result.code,2,result.stderr);const outcome=JSON.parse(result.stdout);assert.equal(outcome.disposition,'failed');assert.match(outcome.summary,/OPENAI_API_KEY is not configured/);}
  finally{await rm(directory,{recursive:true,force:true});}
});

test('real Pi loop crosses Rust evidence, edit, check and checkpoint boundaries',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-pi-'));
  try{
    await writeFile(join(directory,'report.txt'),'original');
    const config={workspace:directory,state_dir:join(directory,'state'),node:process.execPath,worker:join(root,'tests/integration/pi-worker-fixture.mjs'),grant:{id:'grant-pi',scope:{client:'test',project:'reference'},mode:'apply',paths:['report.txt'],tools:['report-check'],profiles:['caretaker'],budget:{max_calls:8,max_tokens:'2000000',max_cost_microusd:'1000000',max_actions:5,max_work_items:1,max_depth:1,deadline_ms:String(Date.now()+30000)},context:'reference',visible_splits:['development'],allow_export:false},request:{run_id:'run-pi',profile:'caretaker',operator:'proofreading@1',prompt:'Infrastructure fixture',provider:'openai',model:fixtureModel('openai').id},tools:{'report-check':{program:process.execPath,args:['-e',"const fs=require('node:fs');if(fs.readFileSync('report.txt','utf8')!=='corrected')process.exit(1);console.log('report-check passed')"],timeout_ms:2000,reads:['report.txt']}}};
    const file=join(directory,'config.json');await writeFile(file,JSON.stringify(config));
    await mkdir(config.state_dir);
    const eventsFile=join(directory,'events.json');await writeFile(eventsFile,JSON.stringify([{id:'host-change',scope:config.grant.scope,run_id:'host-run',producer:'worker',sequence:'1',kind:'artifact_change',timestamp_ms:String(Date.now()),parents:[],correlation:'report',artifacts:[],payload:{observation:'report.txt contains the original version and requires investigation'},provenance:{origin:'observed',source_refs:[],scenario_family:'pi-integration',split:'development',limitations:[]}}]));
    const ingestion=await command(['ingest',join(config.state_dir,'ribosome.db'),eventsFile]);assert.equal(ingestion.code,0,ingestion.stderr);
    const result=await command(['run',file]);assert.equal(result.code,0,result.stderr+'\n'+result.stdout);
    assert.equal(JSON.parse(result.stdout).disposition,'completed');assert.equal(await readFile(join(directory,'report.txt'),'utf8'),'corrected');
    const inspection=await command(['inspect',join(directory,'state/ribosome.db'),'run-pi']);assert.equal(inspection.code,0,inspection.stderr);const stored=JSON.parse(inspection.stdout);assert.equal(stored.status,'completed');assert.deepEqual(stored.checkpoint.pending_operations,[]);assert.equal(stored.checkpoint.event_cursor,'1');assert.equal(stored.agent_activity_count,stored.checkpoint.message_count);assert.ok(stored.agent_activity_count>5);
  }finally{await rm(directory,{recursive:true,force:true});}
});
