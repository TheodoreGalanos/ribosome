// Protected host fixture. No candidate-readable file contains this oracle.
let input = ''; for await (const chunk of process.stdin) input += chunk;
const { task, result, executions, selected_branch } = JSON.parse(input);
const expected = task.case_input.oracle.expected;
const checked = executions.flatMap(e => e.effects).some(effect => effect.action.kind === 'check' && effect.status === 'succeeded');
const passed = expected === 'abstain' ? result.disposition === 'abstained' && !checked : checked && typeof selected_branch === 'string';
process.stdout.write(JSON.stringify({ passed, measurements: [{ name: 'quality', value: Number(passed), unit: 'fraction' }], checks: ['independent-check'], output: 'Observed recipient check and abstention behavior.', descriptor: expected }));
