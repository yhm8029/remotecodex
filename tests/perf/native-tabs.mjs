import { mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { HostClient } from './host-client.mjs';
import { nativeCycle } from './native-cycle.mjs';
import { measureTabSwitches } from './tab-samples.mjs';
import { stats } from './stats.mjs';

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

const [nativeArg, agentArg, directoryArg, countArg = '100'] = process.argv.slice(2);
if (!nativeArg || !agentArg || !directoryArg || !/^\d+$/.test(countArg)) {
  throw new Error('usage: native-tabs.mjs <nativeExe> <agentExe> <directory> [count 1..1000]');
}
const count = Number(countArg);
if (!Number.isSafeInteger(count) || count < 1 || count > 1000) {
  throw new Error('count must be an integer in 1..1000');
}

const nativeExe = resolve(nativeArg);
const agentExe = resolve(agentArg);
const directory = resolve(directoryArg);
const profile = resolve(directory, 'profile');
const resultPath = resolve(directory, 'result.json');
const started = new Date().toISOString();
const errors = [];
const deviceIds = new Set();
const rawCycles = [];
let samples = [];
let host = null;
let first = null;
let keeper = null;
let second = null;

try {
  await mkdir(directory, { recursive: false });
  await mkdir(profile, { recursive: false });

  const playwrightModule = process.env.RC_PLAYWRIGHT_MODULE;
  if (!playwrightModule) throw new Error('RC_PLAYWRIGHT_MODULE is required');
  const { chromium } = await import(playwrightModule);

  host = new HostClient(agentExe);
  await host.pair();
  first = await host.create('RC_TAB_A', tmpdir());
  keeper = first;
  second = await host.create('RC_TAB_B', tmpdir());

  const cycle = await nativeCycle({
    exe: nativeExe,
    profile,
    directory,
    index: 0,
    chromium,
    agentExe,
    keeper,
    first: true,
    onPaired: (clientId) => deviceIds.add(clientId),
    beforeClose: async ({ page }) => {
      samples = await measureTabSwitches(page, [first.label, second.label], count);
    },
  });
  rawCycles.push({ cycle, samples });
} catch (error) {
  flattenErrors(error, errors);
}

for (const id of deviceIds) {
  if (!host) break;
  try {
    await host.api('/devices/' + encodeURIComponent(id), undefined, 'DELETE');
  } catch (error) {
    flattenErrors(error, errors);
  }
}
if (host) {
  try {
    await host.cleanup();
  } catch (error) {
    flattenErrors(error, errors);
  }
}

const finished = new Date().toISOString();
const report = {
  status: errors.length === 0 ? 'MEASURED' : 'FAIL',
  started,
  finished,
  native_exe: nativeExe,
  agent_exe: agentExe,
  profile,
  count,
  completed: rawCycles.length,
  stats: stats(samples),
  samples,
  raw: rawCycles,
  errors,
};
await writeFile(resultPath, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
if (errors.length > 0) process.exitCode = 1;
