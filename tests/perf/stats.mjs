function stats(values) {
  if (!Array.isArray(values) || values.length === 0) return { n: 0, p50: null, p95: null, p99: null, max: null, mean: null };
  if (!values.every((value) => typeof value === 'number' && Number.isFinite(value) && value >= 0)) {
    throw new TypeError('All values must be finite non-negative numbers');
  }
  const sorted = [...values].sort((a, b) => a - b);
  const pick = (fraction) => sorted[Math.max(0, Math.ceil(fraction * sorted.length) - 1)];
  return {
    n: sorted.length,
    p50: pick(0.5),
    p95: pick(0.95),
    p99: pick(0.99),
    max: sorted.at(-1),
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
  };
}

export { stats };
