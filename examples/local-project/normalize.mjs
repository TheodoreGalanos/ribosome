/** Host project code used by the reference demonstration, independent of Ribosome. */
export function normalizeMeasurements(measurements) {
  const metresPerUnit = { m: 1, cm: 0.01, mm: 0.001, km: 1000 };
  return measurements.map(({ value, unit }) => {
    if (!Number.isFinite(value) || !Object.hasOwn(metresPerUnit, unit)) throw new Error('Unsupported or non-finite measurement');
    const result = value * metresPerUnit[unit];
    if (!Number.isFinite(result)) throw new Error('Conversion must produce a finite measurement');
    return result;
  });
}

export function totalMetres(measurements) {
  const total = normalizeMeasurements(measurements).reduce((a, b) => a + b, 0);
  if (!Number.isFinite(total)) throw new Error('Total must be finite');
  return total;
}
