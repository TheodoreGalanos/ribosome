import { readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

// This generator intentionally supports only the schema forms used by this
// protocol. New forms must be implemented here, never silently mapped to any.
const schema = JSON.parse(readFileSync('contracts/schema.json', 'utf8'));
const pascal = s => s.split(/[^a-zA-Z0-9]/).map(p => p[0].toUpperCase() + p.slice(1)).join('');
const enums = new Map();
function ts(s) {
  if (s.$ref) return s.$ref.split('/').at(-1);
  if ('const' in s) return JSON.stringify(s.const);
  if (s.enum) return s.enum.map(x => JSON.stringify(x)).join(' | ');
  if (s.type === 'string') return 'string';
  if (s.type === 'boolean') return 'boolean';
  if (['integer', 'number'].includes(s.type)) return 'number';
  if (s.type === 'array') return `Array<${ts(s.items)}>`;
  if (s.type === 'object' && !s.properties) return 'Record<string, unknown>';
  if (!s.type && !Object.keys(s).length) return 'unknown';
  throw new Error(`Unsupported TypeScript schema: ${JSON.stringify(s)}`);
}
function rust(s, name) {
  if (s.$ref) return s.$ref.split('/').at(-1);
  if (s.enum) { enums.set(name, s.enum); return name; }
  if (s.type === 'string') return 'String';
  if (s.type === 'boolean') return 'bool';
  if (s.type === 'integer') return 'u32';
  if (s.type === 'number') return 'f64';
  if (s.type === 'array') return `Vec<${rust(s.items, name + 'Item')}>`;
  if (s.type === 'object' && !s.properties) return 'serde_json::Map<String, serde_json::Value>';
  if (!s.type && !Object.keys(s).length) return 'serde_json::Value';
  throw new Error(`Unsupported Rust schema: ${JSON.stringify(s)}`);
}
let typescript = '// Generated from contracts/schema.json. Run npm run generate.\n';
let rs = '// Generated from contracts/schema.json. Run npm run generate.\nuse serde::{Deserialize, Serialize};\n';
for (const [name, s] of Object.entries(schema.$defs)) {
  if (s.properties) {
    typescript += `export interface ${name} {\n`;
    rs += `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct ${name} {\n`;
    for (const [key, value] of Object.entries(s.properties)) {
      const optional = !s.required.includes(key);
      typescript += `  ${key}${optional ? '?' : ''}: ${ts(value)};\n`;
      const rt = rust(value, name + pascal(key));
      if (optional) rs += '#[serde(default, skip_serializing_if = "Option::is_none")]\n';
      rs += `pub ${key}: ${optional ? `Option<${rt}>` : rt},\n`;
    }
    typescript += '}\n'; rs += '}\n';
  } else {
    typescript += `export type ${name} = ${ts(s)};\n`;
    const rt = rust(s, name);
    if (rt !== name) rs += `pub type ${name} = ${rt};\n`;
  }
}
for (const [name, values] of enums) {
  rs += `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum ${name} {\n`;
  for (const v of values) rs += `#[serde(rename = ${JSON.stringify(v)})]\n${pascal(v)},\n`;
  rs += '}\n';
}
for (const [key, name] of [['x-methods', 'RpcMethods'], ['x-host-methods', 'HostRpcMethods']]) {
  typescript += `export interface ${name} {\n`;
  for (const [method, [input, output]] of Object.entries(schema[key])) {
    typescript += `  ${JSON.stringify(method)}: { input: ${input}; output: ${output} };\n`;
  }
  typescript += '}\n';
}
const formatted = spawnSync('rustfmt', ['--edition', '2024', '--emit', 'stdout'], { input: rs, encoding: 'utf8' });
if (formatted.status !== 0) throw new Error(formatted.stderr);
const outputs = {
  'crates/ribosome-core/src/contracts.rs': formatted.stdout,
  'packages/agents/src/generated/contracts.ts': typescript,
  'packages/agents/src/generated/schema.json': JSON.stringify(schema, null, 2) + '\n',
};
for (const [path, content] of Object.entries(outputs)) {
  if (process.argv.includes('--check')) {
    if (readFileSync(path, 'utf8') !== content) throw new Error(`${path} is stale; run npm run generate`);
  } else writeFileSync(path, content);
}
console.log(`Contracts ${process.argv.includes('--check') ? 'verified' : 'generated'} (${Object.keys(schema.$defs).length} types).`);
