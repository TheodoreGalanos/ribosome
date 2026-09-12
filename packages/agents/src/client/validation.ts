import { Ajv, type ValidateFunction } from 'ajv';
import schema from '../generated/schema.json' with { type: 'json' };

export { schema };
export const MAX_FRAME = 1_048_576;
export const MAX_PENDING = 32;
const ajv = new Ajv({ strict: false, allErrors: false, validateFormats: false });
const validators = new Map<string, ValidateFunction>();
for (const name of Object.keys(schema.$defs)) {
  validators.set(name, ajv.compile({ $defs: schema.$defs, $ref: `#/$defs/${name}` }));
}
export class RpcError extends Error {
  constructor(public readonly code: number, message: string) { super(message); }
}
export function validate(name: string, value: unknown): void {
  const validator = validators.get(name);
  if (!validator || !validator(value)) {
    throw new RpcError(-32602, `${name}: invalid value at ${validator?.errors?.[0]?.instancePath ?? '/'}`);
  }
}
export function contract(method: string, protocol: 'worker' | 'host' = 'worker'): [string, string] {
  const entry = (schema[protocol === 'host' ? 'x-host-methods' : 'x-methods'] as Record<string, string[]>)[method];
  if (!entry?.[0] || !entry[1]) throw new RpcError(-32601, 'method not found');
  return [entry[0], entry[1]];
}
export function toolSchema(name: string): Record<string, unknown> {
  // Inline references for model providers and Pi's TypeBox validator. The
  // canonical schema is acyclic; neither path accepts schemas from the model.
  function inline(value: unknown): unknown {
    if (Array.isArray(value)) return value.map(inline);
    if (value && typeof value === 'object') {
      const obj = value as Record<string, unknown>;
      if (typeof obj.$ref === 'string') {
        return inline((schema.$defs as Record<string, unknown>)[obj.$ref.split('/').at(-1)!]);
      }
      return Object.fromEntries(Object.entries(obj).map(([key, child]) => [key, inline(child)]));
    }
    return value;
  }
  const definition = (schema.$defs as Record<string, unknown>)[name];
  if (!definition) throw new Error('unknown tool schema');
  return inline(definition) as Record<string, unknown>;
}
