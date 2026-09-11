import { HostClient } from './host-client.mjs';
import { performance } from 'node:perf_hooks';
import { writeFile, mkdir } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';

const [, , agentExe, outputFile, countArg = '500'] = process.argv;
const count = Number(countArg);
assert(Number.isInteger(count) && count >= 1 && count <= 500, 'count must be integer 1..500');

const started = new Date().toISOString();
let passed = false;
let cleanupOk = true;
let error;
const latency = [];
const sids = new Set();
let client;
let keeper;

try {
  client = new HostClient(agentExe);
  await client.pair();
  const tmp = tmpdir();
  keeper = await client.create('keeper', tmp);
  const keeperSid = keeper.session_id;

  for (let i = 1; i <= count; i++) {
    const t0 = performance.now();
    const s = await client.create('loop-' + i, tmp);
    await client.remove(s);
    latency.push(performance.now() - t0);
    assert(!sids.has(s.session_id), 'duplicate sid');
    sids.add(s.session_id);
    assert(s.session_id !== keeperSid, 'sid equals keeper');
    if (i % 25 === 0 || i === count) {
      console.log(`sessions ${i}/${count}`);
      const list = await client.api('/sessions');
      const k = list.find(x => x.session_id === keeperSid);
      assert(k, 'keeper missing');
      assert(k.pid === keeper.pid, 'keeper pid changed');
      assert(k.process_created === keeper.process_created, 'keeper process_created changed');
      assert(k.agent_epoch === keeper.agent_epoch, 'keeper agent_epoch changed');
      assert(k.state === 'running', 'keeper not running');
    }
  }

  const made = client.made;
  assert(made.length === 1 && made[0].session_id === keeperSid, 'leftover host.made sessions');
  passed = true;
} catch (e) {
  error = e && e.message ? e.message : String(e);
} finally {
  if (client) {
    try { await client.cleanup(); } catch (e) { cleanupOk = false; if (!error) error = e.message || String(e); }
  }
}

const finished = new Date().toISOString();
const summary = {
  started, finished, count, completed: latency.length, passed,
  latency_ms: latency, error, cleanup: cleanupOk ? 'ok' : 'failed'
};
await mkdir(dirname(outputFile), { recursive: true });
await writeFile(resolve(outputFile), JSON.stringify(summary), { flag: 'wx' });

if (!passed || !cleanupOk) process.exit(1);