import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp,rm,readFile,writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join,resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { prepare } from '../../examples/local-project/prepare.mjs';
import { totalMetres } from '../../examples/local-project/normalize.mjs';
import { boundedContext } from '../../packages/agents/dist/pi/context.js';

test('reference scenarios execute normalization and validation before recording those observations',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-reference-'));
  try{for(const scenario of ['A','B','C']){const prepared=await prepare(join(directory,scenario),scenario);const check=prepared.events.find(e=>e.kind==='check_completion'&&e.producer==='worker-measurements');assert.match(check.payload.output,/report-check passed/);assert.equal(JSON.parse(await readFile(join(prepared.directory,'report.json'),'utf8')).total_m,scenario==='C'?2:201);}}
  finally{await rm(directory,{recursive:true,force:true});}
});

test('reference evaluator executes installed procedures and attributes independent outcomes',()=>{
  const measurements=[{value:2,unit:'m'},{value:300,unit:'cm'}];assert.equal(totalMetres(measurements),5);assert.throws(()=>totalMetres([{value:1,unit:'kg'}]),/Unsupported/);
  for(const [material,expected]of [['normalize-measurements',true],['sum-unconverted',false]]){
    const task={implementation:{format:'registered_tool',material},case_input:{measurements,expected_m:5}};
    const result=spawnSync(process.execPath,[resolve('examples/local-project/evaluator.mjs')],{input:JSON.stringify(task),encoding:'utf8'});assert.equal(result.status,0,result.stderr);const observation=JSON.parse(result.stdout);assert.equal(observation.passed,expected);assert.equal(observation.measurements[0].value,expected?1:0);
  }
});

test('context reduction preserves the latest assistant/tool exchange',()=>{
  const user={role:'user',content:'original task',timestamp:1};const assistant={role:'assistant',content:[{type:'toolCall',id:'call',name:'evidence_read',arguments:{}}]};const result={role:'toolResult',toolCallId:'call',toolName:'evidence_read',content:[{type:'text',text:'observation'}]};
  const messages=[user,{role:'assistant',content:[{type:'text',text:'x'.repeat(10000)}]},assistant,result];const reduced=boundedContext(messages,1000);assert.equal(reduced[0],user);assert.deepEqual(reduced.slice(-2),[assistant,result]);
});

test('reference check rejects absent and nonnumeric totals and inherited unit names', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-check-'));
  try {
    await writeFile(join(directory, 'source.json'), JSON.stringify({ measurements: [{ value: 1, unit: 'm' }] }));
    for (const total of [undefined, 'invalid', null]) {
      await writeFile(join(directory, 'report.json'), JSON.stringify({ total_m: total, independent_cost_analysis: 250 }));
      const result = spawnSync(process.execPath, [resolve('examples/local-project/tool.mjs'), 'check'], { cwd: directory, encoding: 'utf8' });
      assert.notEqual(result.status, 0, `Invalid total ${total} must not pass`);
    }
    assert.throws(() => totalMetres([{ value: 1, unit: 'toString' }]), /Unsupported/);
    assert.throws(() => totalMetres([{ value: Number.MAX_VALUE, unit: 'km' }]), /finite/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('regeneration preparation revises the actual source and retains the previously checked report', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ribosome-source-revision-'));
  try {
    const prepared = await prepare(directory, 'C');
    const source = JSON.parse(await readFile(join(directory, 'source.json'), 'utf8'));
    const report = JSON.parse(await readFile(join(directory, 'report.json'), 'utf8'));
    assert.equal(totalMetres(source.measurements), 3);
    assert.equal(report.total_m, 2);
    const previous = prepared.events.find(e => e.kind === 'check_completion' && e.producer === 'worker-measurements');
    const revision = prepared.events.find(e => e.kind === 'artifact_change');
    assert.equal(previous.artifacts.find(a => a.path === 'report.json').version, revision.payload.preserved_report_version);
    assert.notEqual(previous.artifacts.find(a => a.path === 'source.json').version, revision.artifacts.find(a => a.path === 'source.json').version);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
