import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';

export function expectedDelivery(workspace) {
  return readdirSync(workspace).filter(name => /^inspection-\d+\.json$/.test(name)).sort().map(name => {
    const source = JSON.parse(readFileSync(join(workspace, name), 'utf8'));
    const accepted = source.readings.filter(reading => reading.status === 'accepted');
    const unresolved = accepted.some(reading => reading.unit !== 'kWh');
    return { asset: source.asset, status: unresolved ? 'unresolved' : 'ready', accepted_count: accepted.length,
      total_kwh: unresolved ? null : accepted.reduce((total, reading) => total + reading.value, 0) };
  });
}

export function checkDelivery(workspace) {
  const expected = expectedDelivery(workspace);
  assert.equal(expected.length, 18, 'qualification requires all eighteen source inspections');
  assert.ok(expected.some(row => row.status === 'unresolved'));
  const actual = JSON.parse(readFileSync(join(workspace, 'delivery.json'), 'utf8'));
  assert.deepEqual(actual, expected, 'delivery must preserve acceptance rules, missing-unit uncertainty and asset order');
  return { assets: expected.length, unresolved: expected.filter(row => row.status === 'unresolved').length };
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  try { console.log(JSON.stringify(checkDelivery(process.cwd()))); }
  catch { console.error('Delivery does not match the inspection acceptance rules. Reinspect the sources and unresolved units.'); process.exitCode = 1; }
}
