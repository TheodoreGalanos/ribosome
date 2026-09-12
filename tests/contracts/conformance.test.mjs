import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join,resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { validate } from '../../packages/agents/dist/client/validation.js';

const fixtures=JSON.parse(await readFile('contracts/fixtures/conformance.json','utf8'));
test('Rust and TypeScript agree on canonical contract fixtures',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'ribosome-contract-'));
  try{for(const [name,value,expected]of fixtures){let accepted=true;try{validate(name,value);}catch{accepted=false;}assert.equal(accepted,expected,`${name}: TS`);const file=join(directory,'fixture.json');await writeFile(file,JSON.stringify(value));const result=spawnSync(resolve('target/debug/ribosome'),['validate',name,file],{encoding:'utf8'});assert.equal(result.status===0,expected,`${name}: Rust ${result.stderr}`);}}
  finally{await rm(directory,{recursive:true,force:true});}
});
test('agent package has no independent database, search or durable queue dependency',async()=>{
  const manifest=JSON.parse(await readFile('packages/agents/package.json','utf8'));
  assert.deepEqual(Object.keys(manifest.dependencies).sort(),['@earendil-works/pi-agent-core','@earendil-works/pi-ai','ajv','typebox']);
});
