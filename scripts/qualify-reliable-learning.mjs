// Focused R7 regression selection. Full CI remains a separate release check.
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
export const checks = [
  ['rust-build', 'cargo', ['build', '--workspace', '--locked']],
  ['typescript-build', 'npm', ['run', 'build']],
  ['generated-contracts', 'npm', ['run', 'check:contracts']],
  ['schema-migrations', 'cargo', ['test', '-p', 'ribosome-core', '--lib', 'schema_', '--locked']],
  ['upgrade-rollback-newer-schema', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'context', 'r1_08_context_migration', '--locked']],
  ['current-store-upgrade', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'qualification', '--locked']],
  ['managed-exports', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'managed_exports', '--locked']],
  ['aggregate-accounting', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'accounting', '--locked']],
  ['contextual-admission', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'laboratory', 'archive_uses_all_repetitions', '--locked']],
  ['protected-exposure', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'laboratory', 'protected_family', '--locked']],
  ['typed-invocation', 'cargo', ['test', '-p', 'ribosome-core', '--test', 'invocation', '--locked']],
  ['compaction-interrupted-effect', process.execPath, ['--test', '--test-name-pattern=Pi compaction effect_interruption', 'tests/integration/compaction.test.mjs']],
  ['bridge-errors-cancellation', process.execPath, ['--test', '--test-name-pattern=RPC rejects|host deadline cancels|root deadline also', 'tests/integration/bridge.test.mjs']],
  ['integrated-recovery', process.execPath, ['--test', 'tests/integration/r7-recovery.test.mjs']],
  ['qualification-reporting', process.execPath, ['--test', 'tests/integration/r7-learning.test.mjs', 'tests/integration/r7-evidence.test.mjs']],
];

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const output = resolve(process.argv[2] ?? '.ribosome/r7-mechanical');
  await mkdir(output, { recursive: true });
  const requested = process.argv.slice(3);
  if (requested.some(id => !checks.some(c => c[0] === id))) throw Error('Unknown focused check ID');
  const selected = requested.length ? checks.filter(c => requested.includes(c[0])) : checks;
  let previous = [];
  if (requested.length) {
    try { previous = JSON.parse(await readFile(join(output, 'report.json'), 'utf8')).checks; }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  const report = { kind: 'focused-mechanical', full_suite_run: false, live_model_calls: 0, checks: previous };
  for (const [id, program, args] of selected) {
    console.log(`Checking ${id}`);
    const started = Date.now();
    const result = spawnSync(program, args, { encoding: 'utf8', timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
    await writeFile(join(output, `${id}.log`), `${result.stdout ?? ''}\n${result.stderr ?? ''}\n${result.error?.message ?? ''}`);
    report.checks = report.checks.filter(c => c.id !== id);
    report.checks.push({ id, status: result.status === 0 && !result.error ? 'passed' : 'failed', exit_code: result.status, elapsed_ms: Date.now() - started });
    report.status = report.checks.every(c => c.status === 'passed') ? 'passed' : 'failed';
    await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    if (result.status !== 0 || result.error) console.error(`${id} failed; inspect ${join(output, `${id}.log`)}`);
  }
  console.log(JSON.stringify(report));
  if (report.status !== 'passed') process.exitCode = 1;
}
