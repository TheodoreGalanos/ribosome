import { existsSync, realpathSync } from 'node:fs';
import { delimiter, dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

// Cargo can discover a stale cargo-clippy from CARGO_HOME before PATH. Select
// the companion shipped with the compiler installation being used for builds.
const cargo = process.env.PATH.split(delimiter).map(p => join(p, 'cargo')).find(existsSync);
if (!cargo) throw new Error('cargo is required');
const executable = join(dirname(realpathSync(cargo)), 'cargo-clippy');
const clippy = existsSync(executable) ? executable : 'cargo-clippy';
const result = spawnSync(clippy, ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'], { stdio: 'inherit' });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
