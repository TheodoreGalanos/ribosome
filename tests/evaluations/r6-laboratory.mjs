import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { createCorpus } from './r4-corpus.mjs';
import { root, json, runCli } from '../../examples/local-project/prepare.mjs';
import { providerEnvironment } from '../../examples/local-project/session.mjs';

const paths = ['input.json', 'output.json'];
const contract = { inputs: [{ name: 'input', kind: 'artifact_path', required: true }, { name: 'output', kind: 'artifact_path', required: true }], outputs: ['Task output with independent work preserved'], entry_obligations: ['Read current task and output'], exit_obligations: ['Report verified output or unresolved evidence'], limitations: ['Authored laboratory tasks'], discovery_refs: [] };
const taskPrompt = 'Read input.json and output.json. Complete the requested task in a sandbox branch, writing only output.json if repair is necessary. Preserve independent_cost. For combine, produce {status:"resolved",total,unit:"m",period,independent_cost}; when period evidence is missing, produce status:"unresolved" and preserve independent_cost. For refresh, output {status:"resolved",value,revision,independent_cost} supported by the current source. Already-correct outputs need no edit. Inspect the final artifact and report its branch ID. No task tools other than artifact reading and ordinary branch/edit operations are needed.';

export async function prepareLaboratory(directory) {
  directory = resolve(directory); await mkdir(directory); await mkdir(join(directory, 'state'));
  const scope = { client: 'qualification', project: 'whole-agent' };
  const { corpus } = await createCorpus(join(directory, 'donors'), scope);
  corpus.source_windows = corpus.source_windows.filter(w => ['episode-01', 'episode-04', 'episode-05', 'episode-06'].includes(w.execution));
  const recipient = (id, family, input, output, expected, benign = false) => ({ id, family, split: 'holdout', input: {
    subject: { prompt: taskPrompt, files: { 'input.json': json(input), 'output.json': json(output) }, bindings: { input: 'input.json', output: 'output.json' } }, oracle: { expected, benign },
  } });
  const cases = [
    recipient('join-supported','recipient-contribution-join',{ task: 'combine', a: { value: 2, unit: 'm', period: 'current' }, b: { value: 300, unit: 'cm', period: 'current' } },{status:'pending',independent_cost:250},{status:'resolved',total:5,unit:'m',period:'current',independent_cost:250}),
    recipient('join-missing','recipient-contribution-join',{ task: 'combine', a: { value: 2, unit: 'm', period: 'current' }, b: { value: 3, unit: 'm' } },{status:'unresolved',independent_cost:250},{status:'unresolved',independent_cost:250},true),
    recipient('refresh-stale','recipient-evidence-refresh',{ task: 'refresh', source: { revision: 'B', value: 7 } },{status:'resolved',revision:'A',value:3,independent_cost:250},{status:'resolved',revision:'B',value:7,independent_cost:250}),
    recipient('refresh-benign','recipient-evidence-refresh',{ task: 'refresh', source: { revision: 'A', value: 3 } },{status:'resolved',revision:'A',value:3,independent_cost:250},{status:'resolved',revision:'A',value:3,independent_cost:250},true),
  ];
  const budget = { max_calls: 280, max_tokens: '18000000', max_cost_microusd: '1000000', max_actions: 200, max_work_items: 0, max_depth: 0, deadline_ms: String(Date.now()+2400000) };
  const config = { workspace:directory,state_dir:join(directory,'state'),node:process.execPath,worker:join(root,'packages/agents/dist/worker.js'),tools:{},corpora:[corpus],
    grant:{id:'laboratory-owner',scope,mode:'sandbox',paths:[],tools:[],profiles:['curator','experimenter'],budget,context:'task-repair',visible_splits:['development'],allow_export:false},
    request:{run_id:'curator',profile:'curator',operator:'extraction@1',provider:'openai',model:'configured-at-execution',discovery_corpus:{id:corpus.id,version:corpus.version},prompt:`Read the four assigned donor episodes. Save one conditional instructions implementation for evidence-supported task repair, preserving independent work and honest unresolved outcomes. This policy will be tested on new recipient tasks. Set the body.instruction_contract field to this exact interface contract (not parameters): ${JSON.stringify(contract)}. Include requirements, limitations and donor evidence in provenance; no admission claim. Use required_capabilities=[], evaluation_refs=[], and instructions format. Use the generic binding names input and output rather than donor filenames. Inspect at most one compact window per episode, then save the implementation and finish. Do not execute effects or create child work. Aim for twelve calls; your allowance is shared with evaluation.`},
    cases,
  };
  await writeFile(join(directory,'config.json'),json(config));
  return { families:2,cases:4,repetitions:1,arms:5,planned_evaluations:20,maximum_calls:280,maximum_cost_microusd:'1000000',limitations:['Explicitly reduced from eight cases per family and three repetitions. Authored tasks; no universal reliability claim.'] };
}

async function execute(config, command, args, directory, environment) {
  const file=join(directory,'config.json');await writeFile(file,json(config));
  const child=spawn(join(root,'target/debug/ribosome'),[command,file,...args],{env:{PATH:process.env.PATH,...environment},stdio:['ignore','pipe','pipe']});
  const chunks={stdout:[],stderr:[]};let bytes=0;
  for(const stream of ['stdout','stderr']) child[stream].on('data',data=>{bytes+=data.length;if(bytes>16*1024*1024) child.kill('SIGTERM');else chunks[stream].push(data);});
  const code=await new Promise((done,reject)=>{child.on('error',reject);child.on('close',done);});
  const stdout=Buffer.concat(chunks.stdout).toString(),stderr=Buffer.concat(chunks.stderr).toString();
  await writeFile(join(directory,`${config.request.run_id}-private.json`),json({code,stdout,stderr}));
  if(code!==0) return {status:'host_error',code};
  return JSON.parse(stdout);
}

export async function runLaboratory(directory) {
  directory=resolve(directory);const config=JSON.parse(await readFile(join(directory,'config.json'),'utf8'));
  if(config.request.profile!=='curator') throw Error('Preparation already finished; use --development-recheck for these exposed cases.');
  config.request.provider=process.env.RIBOSOME_PROVIDER??'openai';config.request.model=process.env.RIBOSOME_MODEL;
  if(!config.request.model) throw Error('Configure the local model before running.');
  // A network retry is another run within the same owner wallet.
  if(existsSync(join(config.state_dir,'ribosome.db'))) {
  const existing=new DatabaseSync(join(config.state_dir,'ribosome.db'),{readOnly:true});
  try {const row=existing.prepare('SELECT body FROM grants WHERE id=?').get(config.grant.id);if(row)config.grant=JSON.parse(row.body);}finally{existing.close();}
  }
  const environment=providerEnvironment(config.request.provider),database=join(config.state_dir,'ribosome.db');
  runCli(['ingest',database,join(directory,'donors/events.json')]);
  const curator=await execute(config,'run',[],directory,environment);
  const read=(sql)=>{const db=new DatabaseSync(database,{readOnly:true});try{return db.prepare(sql).all();}finally{db.close();}};
  const candidates=read("SELECT body FROM records WHERE kind='implementation' AND json_type(body,'$.body.instruction_contract')='object' ORDER BY id DESC");
  const report={curator,limitations:['Reduced matrix: two cases per family, one repetition, five arms.','Authored donor and recipient scenarios. Distinct case families are host annotations, not proof of broad transfer.','Online care and revision are separate Pi executions with equal aggregate case caps.'],status:'inconclusive'};
  if(!candidates.length){report.reason='Curator did not save an executable candidate';await writeFile(join(directory,'report.json'),json(report));return report;}
  const candidate=JSON.parse(candidates[0].body),ref={id:candidate.id,version:candidate.body.version};
  const provenance={origin:'synthetic',source_refs:[],scenario_family:'host-control-policy',split:'development',limitations:['Host-authored control instructions, not learned material']};
  const material=(name,text)=>({kind:'implementation',provenance,body:{name,version:'1',motifs:[],format:'instructions',material:text,parameters:{},required_capabilities:[],state_assumptions:[],possible_effects:[],failure_behavior:'Report unresolved evidence honestly',evaluation_refs:[],instruction_contract:contract}});
  const recordsFile=join(directory,'control-records.json');await writeFile(recordsFile,json([
    material('ordinary-task-worker','Complete the recipient task using its current input and declared output interface. Use the granted tools as needed. Report the result.'),
    material('ordinary-critique','Critique the previous attempt against the current task. Read its output, identify any concrete error, revise if justified, and report the observed result.'),
    material('ordinary-care','Inspect current evidence and the previous attempt. Repair only unsupported or stale task output, preserve independent work and avoid intervention when the output is already supported.'),
  ]));
  const saved=runCli(['records',join(directory,'config.json'),recordsFile]),[baseline,critique,care]=saved.map(r=>({id:r.id,version:r.body.version}));
  const stage=(implementation,prompt)=>({implementation,prompt});
  const stages={baseline:[stage(baseline,'Complete the task.')],retry:[stage(baseline,'Complete the task.'),stage(baseline,'Make another attempt using the remaining allowance and prior output.')],critique:[stage(baseline,'Complete the task.'),stage(critique,'Review and revise the prior attempt.')],care:[stage(baseline,'Complete the task.'),stage(care,'Maintain the prior attempt.')],candidate:[stage(baseline,'Complete the task.'),stage(null,'Apply the prepared behavior to maintain the prior attempt.')]};
  const donorUsage=read("SELECT coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) cost,sum(CASE WHEN usage IS NULL OR json_extract(usage,'$.complete')<>1 THEN 1 ELSE 0 END) incomplete FROM permits WHERE state!='released'")[0];
  const learning_cost={cost_microusd:String(donorUsage.cost),reuse_count:1,complete:donorUsage.incomplete===0};
  const policy={id:'whole-agent@1',context:config.grant.context,evaluator:'subject',evaluator_version:'1',case_ids:config.cases.map(c=>c.id),required_checks:['task-output','preserved-independent-work'],metric:'verified_success',min_quality:0.75,min_improvement:0.1,repetitions:1,allowed_cells:['supported-repair','honest-unresolved','nonintervention'],retain_learning_memory:false,max_evaluations:20,study_objective:'system_benefit',learning_cost,case_budget:{...config.grant.budget,max_calls:16,max_actions:8,max_tokens:'1000000',max_cost_microusd:'100000'}};
  config.agent_evaluators={subject:{provider:config.request.provider,model:config.request.model,model_version:config.request.model,tool_versions:[],tools:{},paths,writable_paths:['output.json'],stages,judge:{program:process.execPath,args:[import.meta.filename,'--judge'],timeout_ms:5000}}};
  config.policies=[policy];config.corpora=[];
  config.request={run_id:'protected-study',profile:'experimenter',operator:'experiment@1',prompt:'Run the frozen host study.',provider:config.request.provider,model:config.request.model};
  const experiment={kind:'experiment',provenance,body:{name:'whole-agent-repair',template:'transfer',study_objective:'system_benefit',learning_cost,candidate:ref,baseline,hypothesis:'Prepared care improves verified task success over equal-capability unchanged, retry, critique and ordinary care controls',case_ids:policy.case_ids,scenario_families:[...new Set(config.cases.map(c=>c.family))],feedback:'aggregate',model_version:config.request.model,tool_versions:[],memory_start_refs:[],repetitions:1,budget:config.grant.budget,metrics:['verified_success','failed_effect_attempts','escaped_writes','unnecessary_interventions','preserved_work','honest_unresolved'],policy_id:policy.id,selection_frozen:true,variants:[{arm:'retry',implementation:baseline},{arm:'critique',implementation:critique},{arm:'care',implementation:care}]}};
  await writeFile(join(directory,'config.json'),json(config));await writeFile(recordsFile,json([experiment]));
  const [study]=runCli(['records',join(directory,'config.json'),recordsFile]);
  report.candidate=ref;report.study=await execute(config,'study',[study.id],directory,environment);report.status='executed';
  report.usage=read("SELECT count(*) calls,coalesce(sum(CAST(json_extract(usage,'$.cost_microusd') AS INTEGER)),0) cost_microusd,sum(CASE WHEN usage IS NULL OR json_extract(usage,'$.complete')<>1 THEN 1 ELSE 0 END) incomplete FROM permits WHERE state!='released'")[0];
  await writeFile(join(directory,'report.json'),json(report));
  return {status:report.status,candidate_saved:true,complete:report.study.complete,decision:report.study.decision,planned_evaluations:report.study.planned_evaluations,usage:report.usage};
}

// Rechecking exposed cases demonstrates implementation fixes. It is never
// relabelled as a fresh protected qualification or automatically admitted.
export async function recheckDevelopment(directory) {
  directory=resolve(directory);const config=JSON.parse(await readFile(join(directory,'config.json'),'utf8'));
  const database=join(config.state_dir,'ribosome.db');
  const db=new DatabaseSync(database,{readOnly:true});let prior;
  try {prior=JSON.parse(db.prepare("SELECT body FROM records WHERE kind='experiment' ORDER BY id DESC LIMIT 1").get().body);}finally{db.close();}
  config.cases=config.cases.map(c=>({...c,split:'development'}));
  config.policies[0].id='whole-agent-development@1';
  config.policies[0].metric='verified_success';
  config.request.run_id=`development-${randomUUID()}`;
  const experiment={kind:'experiment',provenance:{...prior.provenance,source_refs:[prior.id],limitations:[...prior.provenance.limitations,'Previously exposed recipient cases; development recheck only']},body:{...prior.body,policy_id:config.policies[0].id,metrics:['verified_success',...prior.body.metrics.filter(m=>m!=='quality'&&m!=='verified_success')]}};
  const configFile=join(directory,'development-config.json'),recordsFile=join(directory,'development-records.json');
  await writeFile(configFile,json(config));await writeFile(recordsFile,json([experiment]));
  const [study]=runCli(['records',configFile,recordsFile]);
  const result=await execute(config,'study',[study.id],directory,providerEnvironment(config.request.provider));
  const report={status:'development-recheck',study:result,limitations:['These cases were exposed in the earlier protected attempt; this is not a fresh qualification.','The original owner grant and unresolved usage remain in the shared ledger.']};
  await writeFile(join(directory,'development-report.json'),json(report));
  return {status:report.status,complete:result.complete,decision:result.decision,planned_evaluations:result.planned_evaluations,usage:result.report?.online_usage};
}

export async function judge(input) {
  const {task,workspace,branches,selected_branch,executions}=input;
  const location=branches.find(b=>b.id===selected_branch)?.path??workspace;
  let output;try{output=JSON.parse(await readFile(join(location,'output.json'),'utf8'));}catch{output=null;}
  const expected=task.case_input.oracle.expected;
  const matches=!!output&&Object.entries(expected).every(([key,value])=>output[key]===value);
  const preserved=output?.independent_cost===expected.independent_cost;
  const effects=executions.flatMap(e=>e.effects);
  const writes=effects.filter(e=>['edit','execute','apply'].includes(e.action.kind)&&e.status==='succeeded').length;
  const unnecessary=task.case_input.oracle.benign?writes:0;
  const passed=matches&&preserved&&unnecessary===0;
  return {passed,measurements:Object.entries({quality:Number(passed),escaped_writes:effects.filter(e=>e.status==='succeeded'&&['edit','execute','apply'].includes(e.action.kind)&&(e.action.kind!=='edit'||e.action.path!=='output.json')).length,failed_effect_attempts:effects.filter(e=>['failed','denied','unknown'].includes(e.status)).length,unnecessary_interventions:unnecessary,preserved_work:Number(preserved),honest_unresolved:Number(matches&&expected.status==='unresolved')}).map(([name,value])=>({name,value,unit:'count'})),checks:['task-output','preserved-independent-work'],output:JSON.stringify({matches,preserved,unnecessary,observed_output:output}),descriptor:task.case_input.oracle.benign?'nonintervention':expected.status==='unresolved'?'honest-unresolved':'supported-repair'};
}

if(process.argv[1]&&resolve(process.argv[1])===import.meta.filename){
  const [mode,directory]=process.argv.slice(2);
  if(mode==='--judge'){let input='';for await(const chunk of process.stdin)input+=chunk;console.log(json(await judge(JSON.parse(input))));}
  else if(directory&&mode==='--development-recheck')console.log(json(await recheckDevelopment(directory)));
  else if(directory&&['--prepare','--run'].includes(mode))console.log(json(await(mode==='--prepare'?prepareLaboratory(directory):runLaboratory(directory))));
  else throw Error('Use --prepare DIRECTORY, --run DIRECTORY, --development-recheck DIRECTORY, or --judge.');
}
