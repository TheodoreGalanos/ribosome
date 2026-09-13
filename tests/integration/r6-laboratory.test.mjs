import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { prepareLaboratory, judge } from '../evaluations/r6-laboratory.mjs';

test('R6 fixes the reduced matrix and rewards benign nonintervention and honest unresolved output', async () => {
  const parent=await mkdtemp(join(tmpdir(),'ribosome-r6-'));
  try {
    const directory=join(parent,'study');
    const planned=await prepareLaboratory(directory);
    assert.equal(planned.planned_evaluations,20);assert.equal(planned.maximum_calls,280);
    const config=JSON.parse(await readFile(join(directory,'config.json'),'utf8'));
    assert.equal(new Set(config.cases.map(c=>c.family)).size,2);
    assert.equal(config.request.profile,'curator');
    assert.ok(!config.request.prompt.includes('recipient-contribution-join'));
    for(const task of config.cases) {
      assert.ok(!Object.hasOwn(task.input.subject.files,'oracle.json'));
      await writeFile(join(parent,'output.json'),JSON.stringify(task.input.oracle.expected));
      const input={task:{case_input:task.input},workspace:parent,branches:[],executions:[{effects:[]}]};
      const observed=await judge(input);
      assert.equal(observed.passed,true);
      if(task.input.oracle.benign) {
        input.executions[0].effects.push({action:{kind:'edit'},status:'succeeded'});
        assert.equal((await judge(input)).passed,false,'needless edit must not improve a benign score');
      }
    }
    await assert.rejects(prepareLaboratory(directory),/EEXIST/);
  } finally {await rm(parent,{recursive:true,force:true});}
});
