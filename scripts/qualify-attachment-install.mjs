import { mkdtemp, copyFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const consumer = await mkdtemp(join(tmpdir(), 'ribosome-installed-attachment-'));
const cache = join(consumer, 'npm-cache');
function run(program, args, cwd = process.cwd(), environment = process.env) {
  const result = spawnSync(program, args, { cwd, env: environment, encoding: 'utf8', timeout: 180000, maxBuffer: 8 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} ${args[0]} exited ${result.status}: ${result.stderr}\n${result.stdout}`);
  return result.stdout;
}
await writeFile(join(consumer, 'package.json'), JSON.stringify({ name: 'ribosome-installed-attachment', private: true, type: 'module' }));
run('cargo', ['install', '--debug', '--path', 'crates/ribosome-cli', '--locked', '--offline', '--root', consumer]);
const packed = JSON.parse(run('npm', ['pack', '--workspace', '@ribosome/agents', '--json', '--cache', cache, '--pack-destination', consumer]));
run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--cache', cache, join(consumer, packed[0].filename)], consumer);
for (const name of ['attachments.test.mjs', 'pi-attachment.test.mjs', 'attachment-worker-fixture.mjs', 'model-fixture.mjs', 'r7-learning.test.mjs', 'r7-recovery.test.mjs', 'r7-invocation.test.mjs', 'invocation-worker-fixture.mjs']) await copyFile(resolve('tests/integration', name), join(consumer, name));
await copyFile(resolve('examples/attached-agent/demo.mjs'), join(consumer, 'demo.mjs'));
await copyFile(resolve('examples/learned-behavior/demo.mjs'), join(consumer, 'learning-demo.mjs'));
const output = run(process.execPath, ['--test', '--test-concurrency=2', 'attachments.test.mjs', 'pi-attachment.test.mjs', 'r7-learning.test.mjs', 'r7-recovery.test.mjs', 'r7-invocation.test.mjs'], consumer, {
  ...process.env, RIBOSOME_TEST_INVOCATION_WORKER: join(consumer, 'invocation-worker-fixture.mjs'), RIBOSOME_TEST_LEARNING: join(consumer, 'learning-demo.mjs'), RIBOSOME_TEST_CLI: join(consumer, 'bin/ribosome'), RIBOSOME_TEST_WORKER: join(consumer, 'attachment-worker-fixture.mjs'),
});
await writeFile(join(consumer, 'qualification.log'), output);
console.log(output);
const report = { consumer, executable: join(consumer, 'bin/ribosome'), learning_demo: join(consumer, 'learning-demo.mjs'), package: packed[0].filename, evidence: join(consumer, 'qualification.log'), installed_checks: 'passed', prepared_invocation: 'passed', recovery_continuation: 'passed', learning_entrypoint: 'passed', distinct_tests: Number(output.match(/(?:#|ℹ) tests (\d+)/)?.[1]) || null, live_model_calls: 0 };
if (process.env.RIBOSOME_INSTALL_REPORT) await writeFile(process.env.RIBOSOME_INSTALL_REPORT, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
