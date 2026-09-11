export function checkProbeCadence(previous, intervalSeconds) {
  const current = { monotonic_ms: performance.now(), wall_ms: Date.now() };
  if (previous != null) {
    const tolerance = (intervalSeconds + Math.max(5, intervalSeconds * 0.1)) * 1000;
    const monoGap = current.monotonic_ms - previous.monotonic_ms;
    const wallGap = current.wall_ms - previous.wall_ms;
    if (monoGap > tolerance || wallGap > tolerance) {
      throw new Error(
        `Probe cadence violated: monotonic gap ${monoGap.toFixed(0)}ms, wall gap ${wallGap.toFixed(0)}ms (tolerance ${tolerance.toFixed(0)}ms)`
      );
    }
  }
  return current;
}
