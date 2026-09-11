import { startSoakSampler } from './soak-sampler.mjs';
import { mkdir, open, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { withBrowserFixture } from './browser-harness.mjs';
import { runSoakLoop } from './soak-loop.mjs';

function usage() {
  throw new Error('usage: node tests/perf/soak.mjs <agentExe> <directory> [seconds=86400] [interval=300] [agentManifest]');
}

function parseBoundedInt(value, name, min, max) {
  if (!/^\d+$/.test(value ?? '')) throw new Error(`${name} must be an integer in ${min}..${max}`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${name} must be an integer in ${min}..${max}`);
  }
  return parsed;
}

function flattenErrors(error, output) {
  if (error instanceof AggregateError) {
    for (const child of error.errors) flattenErrors(child, output);
    return;
  }
  if (error && Array.isArray(error.errors)) {
    for (const child of error.errors) flattenErrors(child, output);
    return;
  }
  output.push(String(error?.message ?? error));
}

async function sha256(path) {
  try {
    const bytes = await readFile(path);
    return createHash('sha256').update(bytes).digest('hex');
  } catch {
    return null;
  }
}

const args = process.argv.slice(2);
if (args.length < 2 || args.length > 5) usage();
const [agentArg, directoryArg, secondsArg = '86400', intervalArg = '300', agentManifest] = args;
const seconds = parseBoundedInt(secondsArg, 'seconds', 1, 86400);
const interval = parseBoundedInt(intervalArg, 'interval', 1, 3600);
const agentExe = resolve(agentArg);
if (seconds === 86400 && !agentManifest) throw new Error('24-hour soak requires an Agent process manifest');
const directory = resolve(directoryArg);

const helper = resolve('tests/perf/pty-workload.mjs');
const unsafePath = /["\r\n&|<>^]/;
if (unsafePath.test(process.execPath) || unsafePath.test(helper)) {
  throw new Error('Node executable or PTY helper path contains an unsafe shell character');
}
const command = `"${process.execPath}" "${helper}" echo\r`;

await mkdir(directory, { recursive: false });
const probesPath = resolve(directory, 'probes.ndjson');
const identitiesPath = resolve(directory, 'identities.json');
const resultPath = resolve(directory, 'result.json');
const probes = await open(probesPath, 'wx');
const started = new Date().toISOString();
const errors = [];
let loopResult = null;
let observedSessions = null;
let agentSha256 = null;

try {
  agentSha256 = await sha256(agentExe);
  try {
    loopResult = await withBrowserFixture(agentExe, directory, async ({ page, sessions, host }) => {
      observedSessions = sessions;
      await writeFile(
        identitiesPath,
        JSON.stringify({
          sessions,
          scope: 'loopback Agent with 2 CMD PTYs; Chrome is test harness',
          requested_seconds: seconds,
          stabilization_seconds: 3600,
        }) + '\n',
        { flag: 'wx' },
      );

      const id = sessions[0].session_id;
      await page.evaluate(({ id: sessionId, cmd }) => window.RCPerf.input(sessionId, cmd), { id, cmd: command });
      await page.waitForFunction(
        (sessionId) => window.RCPerf.text(sessionId).includes('RC_ECHO_READY'),
        id,
        { timeout: 15000 },
      );

      const sampler = agentManifest ? await startSoakSampler({directory, agentManifest, sessions, durationSeconds:seconds}) : null;
      try {
      const loop = await runSoakLoop({
        page,
        host,
        sessions,
        durationSeconds: seconds,
        intervalSeconds: interval,
        append: async (row) => {
          if (sampler) await sampler.checkpoint();
          await probes.write(JSON.stringify(row) + '\n');
          await probes.sync();
          console.log(JSON.stringify({ elapsed_seconds: row.elapsed_seconds, at: row.at }));
        },
      });
      const resources = sampler ? await sampler.finish() : null;
      if (resources?.leak_flag || resources?.handles_monotonic_growth || resources?.threads_monotonic_growth) throw new Error('resource growth requires investigation');
      return {...loop, resources};
      } finally { if(sampler) await sampler.abort(); }
    });
  } catch (error) {
    flattenErrors(error, errors);
  }
} catch (error) {
  flattenErrors(error, errors);
} finally {
  try {
    await probes.close();
  } catch (error) {
    flattenErrors(error, errors);
  }
}

const finished = new Date().toISOString();
const elapsed = Number(loopResult?.elapsed_seconds ?? 0);
const fullRunPassed = seconds === 86400 && elapsed >= 86400 && loopResult?.resources?.observed_seconds >= 86400 && loopResult.resources.post_cutoff_count >= 2 && Number.isFinite(loopResult.resources.private_bytes_slope_per_hour) && loopResult.resources.leak_flag === false;
if (seconds === 86400 && !fullRunPassed && !errors.length) errors.push('Incomplete 24-hour resource evidence');
const status = errors.length > 0
  ? 'FAIL'
  : fullRunPassed
    ? 'PASS_BOUNDED'
    : 'SMOKE_PASS';
const result = {
  status,
  started_utc: started,
  finished_utc: finished,
  requested_seconds: seconds,
  interval_seconds: interval,
  agent_exe: agentExe,
  agent_sha256: agentSha256,
  sessions: observedSessions,
  loop: loopResult,
  errors,
};

await writeFile(resultPath, JSON.stringify(result) + '\n', { flag: 'wx' });
console.log(JSON.stringify(result));
if (errors.length > 0) process.exitCode = 1;
