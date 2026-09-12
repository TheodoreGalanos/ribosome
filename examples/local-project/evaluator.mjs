import { totalMetres } from './normalize.mjs';
let input='';for await(const chunk of process.stdin){input+=chunk;if(input.length>1_048_576)throw Error('Evaluator input too large');}
const task=JSON.parse(input);
// The host installs these two procedures. Agent-authored code is never eval'd.
// This evaluator measures their actual outputs against protected expectations.
let output;
if(task.implementation.format!=='registered_tool')throw Error('Reference evaluator requires a host-registered procedure');
if(task.implementation.material==='normalize-measurements')output=totalMetres(task.case_input.measurements);
else if(task.implementation.material==='sum-unconverted')output=task.case_input.measurements.reduce((sum,m)=>sum+m.value,0);
else throw Error('Procedure is not installed in the reference evaluator');
const passed=Math.abs(output-task.case_input.expected_m)<1e-9;
console.log(JSON.stringify({passed,measurements:[{name:'quality',value:passed?1:0,unit:'fraction'},{name:'invalid_effects',value:0,unit:'count'}],checks:['dimensional-consistency','expected-total'],output:JSON.stringify({total_m:output}),descriptor:'mixed-length-units'}));
