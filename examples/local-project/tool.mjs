import { readFile, writeFile } from 'node:fs/promises';
import { totalMetres } from './normalize.mjs';
const source=JSON.parse(await readFile('source.json','utf8'));
const report=JSON.parse(await readFile('report.json','utf8'));
if(process.argv[2]==='normalize') {
  await writeFile('report.json',JSON.stringify({...report,total_m:totalMetres(source.measurements)},null,2)+'\n');
  console.log('Normalized current measurements and wrote report.json; a fresh report-check is required.');
}else if(process.argv[2]==='check') {
  if(!Number.isFinite(report.total_m)||Math.abs(report.total_m-totalMetres(source.measurements))>1e-9)throw Error('Report total does not match the current source measurements in metres');
  if(report.independent_cost_analysis!==250)throw Error('Independent cost analysis must be preserved');
  console.log('report-check passed: current total and independent cost analysis verified.');
}else throw Error('Unknown registered reference tool');
