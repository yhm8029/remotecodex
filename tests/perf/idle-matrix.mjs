import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { mkdir, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { HostClient } from './host-client.mjs';

const execFileP = promisify(execFile);
const POWERSHELL = 'powershell.exe';
const MEASURE_TIMEOUT = 660000;
const MANIFEST_TIMEOUT = 30000;

async function delay(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function writeExclusive(path, data) {
  await writeFile(path, data, { flag: 'wx' });
}

async function invokeScript(scriptPath, args, timeout) {
  return execFileP(
    POWERSHELL,
    ['-NoProfile', '-File', scriptPath, ...args],
    { timeout, windowsHide: true }
  );
}

async function measure(count, host, sessionsPath, runDirectory, agentManifest) {
  const manifestPath = resolve(runDirectory, `idle${count}-manifest.json`);
  await invokeScript(
    resolve('tests/perf/process-manifest.ps1'),
    ['-AgentManifest', agentManifest, '-Sessions', sessionsPath, '-Output', manifestPath],
    MANIFEST_TIMEOUT
  );

  const ndjsonPath = resolve(runDirectory, `idle${count}-10m.ndjson`);
  await invokeScript(
    resolve('scripts/measure-resources.ps1'),
    ['-Manifest', manifestPath, '-Output', ndjsonPath, '-DurationSeconds', '600', '-IntervalMs', '1000'],
    MEASURE_TIMEOUT
  );
}

const [, , agentExe, runDirectoryArg, agentManifest] = process.argv;
if (!agentExe || !runDirectoryArg || !agentManifest) {
  console.error('Usage: idle-matrix.mjs <agentExe> <runDirectory> <agentManifest>');
  process.exit(2);
}

const runDirectory = resolve(runDirectoryArg);
const agentManifestAbs = resolve(agentManifest);

await mkdir(runDirectory, { recursive: true });

const client = new HostClient(agentExe);
const summary = {
  started: new Date().toISOString(),
  finished: null,
  scenarioCompleted: [],
  error: null,
  cleanup: 'pending'
};

const owned = [];

try {
  await client.pair();
  for (const count of (process.argv[5] === '8' ? [8] : [2, 8])) {
    while (owned.length < count) {
      owned.push(await client.create('IDLE-' + randomUUID(), tmpdir()));
    }
    console.log('scenario', count);
    await delay(3000);
    const sessionsPath = resolve(runDirectory, 'idle' + count + '-sessions.json');
    await writeExclusive(sessionsPath, JSON.stringify(owned, null, 2));
    await measure(count, client, sessionsPath, runDirectory, agentManifestAbs);
    summary.scenarioCompleted.push(count);
    console.log('finished', count);
  }
} catch (err) {
  summary.error = err.message;
} finally {
  try {
    await client.cleanup();
    summary.cleanup = 'ok';
  } catch (cleanupErr) {
    summary.cleanup = 'failed';
    summary.error = (summary.error ? summary.error + '; ' : '') + cleanupErr.message;
  }
  summary.finished = new Date().toISOString();
  summary.remaining_owned_session_ids = client.made.map(s => s.session_id);
  summary.remaining_test_device_id = client.deviceId;
}

const summaryPath = resolve(runDirectory, 'idle-matrix.json');
await writeExclusive(summaryPath, JSON.stringify(summary, null, 2));

process.exit(summary.error ? 1 : 0);