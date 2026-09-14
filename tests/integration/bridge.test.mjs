import { fixtureModel } from './model-fixture.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { RpcPeer } from '../../packages/agents/dist/client/rpc.js';
import { DatabaseSync } from 'node:sqlite';
import { AttachmentClient } from '@ribosome/agents/attachments';

const root=resolve('.');
async function command(args){return new Promise((resolve,reject)=>{const p=spawn(join(root,'target/debug/ribosome'),args,{env:{PATH:process.env.PATH}});let stdout='',stderr='';p.stdout.on('data',b=>stdout+=b);p.stderr.on('data',b=>stderr+=b);p.on('error',reject);p.on('exit',code=>resolve({code,stdout,stderr}));});}

function faultConfig(directory,worker,deadline=30000){return {workspace:directory,state_dir:join(directory,'state'),node:process.execPath,worker:join(root,'tests/integration',worker),grant:{id:'grant-fault',scope:{client:'test',project:'fault'},mode:'apply',paths:['report.txt'],tools:[],profiles:['caretaker'],budget:{max_calls:8,max_tokens:'2000000',max_cost_microusd:'1000000',max_actions:5,max_work_items:1,max_depth:1,deadline_ms:String(Date.now()+deadline)},context:'fault',visible_splits:['development'],allow_export:false},request:{run_id:'run-fault',profile:'caretaker',operator:'proofreading@1',prompt:join(directory,'report.txt'),provider:'openai',model:fixtureModel('openai').id},tools:{}};}

async function deadlineConfig(directory, worker) {
  // Database migrations are setup, not part of the worker deadline under test.
  await mkdir(join(directory, 'state'));
  const events = join(directory, 'events.json');
  await writeFile(events, '[]');
  const prepared = await command(['ingest', join(directory, 'state/ribosome.db'), events]);
  assert.equal(prepared.code, 0, prepared.stderr);
  return faultConfig(directory, worker, 3000);
}

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
    await writeFile(join(directory,'report.txt'),'original');const config=await deadlineConfig(directory,'idle-worker-fixture.mjs');const file=join(directory,'config.json');await writeFile(file,JSON.stringify(config));
    const result=await command(['run',file]);assert.equal(result.code,2,result.stderr);assert.equal(JSON.parse(result.stdout).disposition,'exhausted');
  }finally{await rm(directory,{recursive:true,force:true});}
});

test('host settlement resumes real Pi after a lost adapter outcome without rewriting it as success', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-settlement-'));
  let client;
  try {
    const report = join(directory, 'report.txt');
    await writeFile(report, 'original');
    await writeFile(`${report}.settlement-test`, 'test assertion selector');
    const config = faultConfig(directory, 'crash-worker-fixture.mjs', 60000);
    const file = join(directory, 'config.json'), records = join(directory, 'records.json');
    await writeFile(file, JSON.stringify(config)); await writeFile(records, '[]');
    const seeded = await command(['records', file, records]); assert.equal(seeded.code, 0, seeded.stderr);
    const database = join(directory, 'state/ribosome.db');
    const db = new DatabaseSync(database);
    db.exec("CREATE TRIGGER lose_outcome BEFORE UPDATE OF observation ON effects WHEN NEW.id='run-fault/crash-edit' BEGIN SELECT RAISE(FAIL,'injected outcome loss'); END");
    db.close();
    const crashed = await command(['run', file]); assert.equal(crashed.code, 2, crashed.stderr);
    assert.equal(await readFile(report, 'utf8'), 'crash edit completed');
    const reopened = new DatabaseSync(database); reopened.exec('DROP TRIGGER lose_outcome'); reopened.close();
    const blocked = await command(['run', file]); assert.equal(blocked.code, 2, blocked.stderr);
    assert.match(JSON.parse(blocked.stdout).summary, /owner reconciliation/);
    await writeFile(report, 'subsequent owner edit');
    const inspected = await command(['effect', file, 'run-fault/crash-edit']); assert.equal(inspected.code, 0, inspected.stderr);
    const view = JSON.parse(inspected.stdout);
    assert.equal(view.receipt.status, 'unknown');
    const decision = { operation_id: view.receipt.operation_id, expected_receipt_version: view.receipt_version, executor_stopped: true, workspace_versions: view.workspace_versions, reason: 'The test executor exited and the owner inspected the resulting workspace.', source_refs: [] };
    client = await AttachmentClient.start({ executable: join(root, 'target/debug/ribosome'), config: file, environment: { PATH: process.env.PATH } });
    assert.deepEqual(await client.inspectEffect(decision.operation_id), view);
    const settled = await client.settleEffect(decision);
    assert.equal(settled.status, 'unknown'); assert.ok(settled.settlement);
    await client.close(); client = undefined;
    const settlementFile = join(directory, 'settlement.json'); await writeFile(settlementFile, JSON.stringify(decision));
    const retried = await command(['settle', file, settlementFile]); assert.equal(retried.code, 0, retried.stderr);
    assert.deepEqual(JSON.parse(retried.stdout), settled);
    const resumed = await command(['run', file]); assert.equal(resumed.code, 0, resumed.stderr + resumed.stdout);
    assert.equal(await readFile(report, 'utf8'), 'subsequent owner edit');
    const stored = new DatabaseSync(database, { readOnly: true });
    try {
      const rows = stored.prepare('SELECT body,settlement FROM effects').all();
      assert.equal(rows.length, 1);
      assert.equal(JSON.parse(rows[0].body).status, 'unknown');
      assert.equal(stored.prepare("SELECT count(*) AS n FROM events WHERE producer='ribosome-host-settlement'").get().n, 1);
    } finally { stored.close(); }
  } finally { if (client) await client.close(); await rm(directory, { recursive: true, force: true }); }
});

test('root deadline also bounds an unresponsive worker handshake', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-handshake-deadline-'));
  try {
    await writeFile(join(directory, 'report.txt'), 'original');
    const config = await deadlineConfig(directory, 'stall-handshake-fixture.mjs');
    const file = join(directory, 'config.json');
    await writeFile(file, JSON.stringify(config));
    const started = performance.now();
    const result = await command(['run', file]);
    assert.equal(result.code, 2, result.stderr);
    const outcome = JSON.parse(result.stdout);
    assert.equal(outcome.disposition, 'exhausted');
    assert.match(outcome.summary, /Root deadline reached during worker handshake/);
    assert.ok(performance.now() - started < 4500, 'Handshake must not extend the root deadline to its five-second timeout');
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
