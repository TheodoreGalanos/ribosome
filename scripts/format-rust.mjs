import { existsSync, realpathSync } from 'node:fs';
import { delimiter, dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
const cargo = process.env.PATH.split(delimiter).map(p => join(p, 'cargo')).find(existsSync);
if (!cargo) throw new Error('cargo is required');
const companion = join(dirname(realpathSync(cargo)), 'cargo-fmt');
const result = spawnSync(existsSync(companion) ? companion : 'cargo-fmt', ['fmt', '--all', ...process.argv.slice(2)], { stdio: 'inherit', env: { ...process.env, CARGO: cargo } });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
