import { checkProbeCadence } from './soak-cadence.mjs';
export async function runSoakLoop({ page, host, sessions, durationSeconds, intervalSeconds = 300, append }) {
  if (!Number.isInteger(durationSeconds) || durationSeconds < 1 || durationSeconds > 86400) throw new Error('invalid durationSeconds');
  if (!Number.isInteger(intervalSeconds) || intervalSeconds < 1 || intervalSeconds > 3600) throw new Error('invalid intervalSeconds');

  const start = performance.now();
  const firstId = sessions[0].session_id;
  let probes = 0;
  let previousProbe = null;

  const sleep = (ms) => new Promise((r) => setTimeout(r, Math.min(ms, 1000)));

  const probe = async () => {
    previousProbe = checkProbeCadence(previousProbe, intervalSeconds);
    const elapsed = (performance.now() - start) / 1000;
    const echo_before = await page.evaluate((id) => window.RCPerf.echoSamples(id, 10), firstId);
    const reconnect = await page.evaluate((id) => window.RCPerf.reconnectSamples(id, 1), firstId);
    const echo_after = await page.evaluate((id) => window.RCPerf.echoSamples(id, 10), firstId);

    const live = await host.api('/sessions');
    const liveById = new Map(live.map((s) => [s.session_id, s]));
    for (const s of sessions) {
      const cur = liveById.get(s.session_id);
      if (!cur) throw new Error('session missing');
      if (cur.pid !== s.pid || cur.process_created !== s.process_created || cur.agent_epoch !== s.agent_epoch || cur.state !== 'running') {
        throw new Error('session identity changed');
      }
    }

    const perfInfo = await page.evaluate(() => {
      const p = window.RCPerf.perf;
      const dropped = p.dropped;
      p.clear();
      return { dropped };
    });
    if (perfInfo.dropped > 0) throw new Error('dropped events');

    const row = {
      elapsed_seconds: elapsed,
      at: new Date().toISOString(),
      echo_before,
      reconnect,
      echo_after,
      identities_valid: true,
      dropped: 0,
    };
    await append(row);
    probes++;
    return elapsed;
  };

  let nextDue = start;
  while (true) {
    await probe();
    const now = performance.now();
    const totalElapsed = (now - start) / 1000;
    if (totalElapsed >= durationSeconds) break;
    nextDue = Math.min(start + durationSeconds * 1000, nextDue + intervalSeconds * 1000);
    let remaining = nextDue - performance.now();
    while (remaining > 0) {
      await sleep(remaining);
      remaining = nextDue - performance.now();
    }
  }

  const finalElapsed = (performance.now() - start) / 1000;
  await probe();

  return { elapsed_seconds: finalElapsed, probes };
}
