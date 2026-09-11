export function aggregateProductSample(sample, identities) {
  if (!Array.isArray(identities) || !Array.isArray(sample?.processes)) throw new Error('invalid input');
  const productIdentities = identities.filter(i => i.Group === 'product');
  if (productIdentities.length < 1) {
    throw new Error('No product identities provided');
  }
  const idSet = new Set();
  for (const ident of productIdentities) {
    if (!Number.isInteger(ident.Id) || ident.Id <= 0 || typeof ident.StartTicks !== 'string' || !/^\d+$/.test(ident.StartTicks)) throw new Error('invalid identity');
    if (idSet.has(ident.Id)) {
      throw new Error('Duplicate product identity Id: ' + ident.Id);
    }
    idSet.add(ident.Id);
  }
  if (!Number.isFinite(sample.elapsed_seconds) || sample.elapsed_seconds < 0) {
    throw new Error('Invalid elapsed_seconds');
  }
  let private_bytes = 0;
  let handles = 0;
  let threads = 0;
  for (const ident of productIdentities) {
    const matchingRows = sample.processes.filter(p => p.pid === ident.Id);
    if (matchingRows.length !== 1) {
      throw new Error('Expected exactly one row for product Id ' + ident.Id);
    }
    const row = matchingRows[0];
    if (row.group !== 'product') {
      throw new Error('Row group mismatch for Id ' + ident.Id);
    }
    if (row.status !== 'ok') {
      throw new Error('Row status not ok for Id ' + ident.Id);
    }
    if (row.started_ticks !== ident.StartTicks) {
      throw new Error('Started ticks mismatch for Id ' + ident.Id);
    }
    if (!Number.isFinite(row.private_bytes) || row.private_bytes < 0) {
      throw new Error('Invalid private_bytes for Id ' + ident.Id);
    }
    if (!Number.isFinite(row.handles) || row.handles < 0) {
      throw new Error('Invalid handles for Id ' + ident.Id);
    }
    if (!Number.isFinite(row.threads) || row.threads < 0) {
      throw new Error('Invalid threads for Id ' + ident.Id);
    }
    private_bytes += row.private_bytes;
    handles += row.handles;
    threads += row.threads;
  }
  return {
    elapsed_seconds: sample.elapsed_seconds,
    private_bytes: private_bytes,
    handles: handles,
    threads: threads
  };
}
export function summarizeSoakResources(samples, identities, cutoffSeconds=3600) {
  if (!Array.isArray(samples) || !samples.length || !Number.isFinite(cutoffSeconds) || cutoffSeconds < 0) throw new Error('invalid samples/cutoff');
  const cleaned = samples.map(sample => aggregateProductSample(sample, identities));
  for (let i=1;i<cleaned.length;i++) if(cleaned[i].elapsed_seconds <= cleaned[i-1].elapsed_seconds) throw new Error('nonmonotonic elapsed');
  const observedSeconds = cleaned[cleaned.length - 1].elapsed_seconds;
  const postCutoff = cleaned.filter(s => s.elapsed_seconds >= cutoffSeconds);
  const postCutoffCount = postCutoff.length;

  let slope = null;
  if (postCutoff.length >= 2) {
    const tMin = postCutoff[0].elapsed_seconds;
    const tMax = postCutoff[postCutoff.length - 1].elapsed_seconds;
    if (tMax - tMin >= 3600) {
      let sx = 0, sy = 0, sxx = 0, sxy = 0;
      for (const p of postCutoff) {
        const x = (p.elapsed_seconds - tMin) / 3600;
        const y = p.private_bytes;
        sx += x; sy += y; sxx += x * x; sxy += x * y;
      }
      const n = postCutoff.length;
      const denom = n * sxx - sx * sx;
      slope = denom !== 0 ? (n * sxy - sx * sy) / denom : 0;

    }
  }

  const leakFlag = slope === null ? null : (slope >= 10485760);

  const buckets = new Map();
  for (const s of cleaned) {
    const hour = Math.floor(s.elapsed_seconds / 3600);
    buckets.set(hour, s);
  }
  const sortedHours = [...buckets.keys()].sort((a, b) => a - b);
  const hourly = [];
  let prev = null;
  for (const h of sortedHours) {
    const s = buckets.get(h);
    const delta = prev === null ? null : (s.private_bytes - prev.private_bytes);
    hourly.push({ hour: h, elapsed_seconds: s.elapsed_seconds, private_bytes: s.private_bytes, handles: s.handles, threads: s.threads, private_bytes_delta: delta });
    prev = s;
  }

  const postHourly = hourly.filter(x => x.elapsed_seconds >= cutoffSeconds);
  let handlesMonotonic = null, threadsMonotonic = null;
  function growth(arr) {
    if (arr.length < 3) return null;
    if (!arr.slice(1).every((value, index) => value >= arr[index])) return false;
    return arr[arr.length - 1] > arr[0];
  }
  handlesMonotonic = growth(postHourly.map(x => x.handles));
  threadsMonotonic = growth(postHourly.map(x => x.threads));

  return {
    sample_count: cleaned.length,
    cutoff_seconds: cutoffSeconds,
    observed_seconds: observedSeconds,
    post_cutoff_count: postCutoffCount,
    private_bytes_slope_per_hour: slope,
    leak_threshold_bytes_per_hour: 10485760,
    leak_flag: leakFlag,
    hourly,
    handles_monotonic_growth: handlesMonotonic,
    threads_monotonic_growth: threadsMonotonic
  };
}
