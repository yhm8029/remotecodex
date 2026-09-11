import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { withBrowserFixture } from './browser-harness.mjs';
import { withHmrFixture } from './hmr-fixture.mjs';
import { stats } from './stats.mjs';

const execFileP = promisify(execFile);
const UNSAFE_COMMAND_PATH = /["\r\n&|<>^]/;

function sleep(milliseconds) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));
}

function flattenErrors(error, output) {
  if (error instanceof AggregateError) {
    for (const nested of error.errors) flattenErrors(nested, output);
    return;
  }
  if (error && Array.isArray(error.errors)) {
    for (const nested of error.errors) flattenErrors(nested, output);
    return;
  }
  output.push(String(error?.message ?? error));
}

async function waitForFile(filePath, timeoutMilliseconds) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      await readFile(filePath);
      return;
    } catch (error) {
      if (!['ENOENT', 'EACCES', 'EBUSY', 'EPERM'].includes(error?.code)) throw error;
    }
    await sleep(100);
  }
  throw new Error(`Timed out waiting for ${filePath}`);
}

async function requireMissing(filePath) {
  try {
    await readFile(filePath);
  } catch (error) {
    if (error?.code === 'ENOENT') return;
    throw error;
  }
  throw new Error(`Media result appeared before the bounded stress finished: ${filePath}`);
}

async function verifyEncoder(identity, outputPath) {
  await execFileP(
    'powershell.exe',
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      identityHelper,
      '-Id',
      String(identity.Id),
      '-StartTicks',
      identity.StartTicks,
      '-Exe',
      encoderExe,
      '-Output',
      outputPath,
    ],
    { windowsHide: true, timeout: 15_000, maxBuffer: 16_384 },
  );
  return JSON.parse(await readFile(outputPath, 'utf8'));
}

function usage() {
  throw new Error('usage: node tests/perf/combined.mjs <agentExe> <newDirectory> <agentManifest>');
}

const [agentExeArgument, directoryArgument, agentManifestArgument] = process.argv.slice(2);
if (!agentExeArgument || !directoryArgument || !agentManifestArgument) usage();

const agentExe = resolve(agentExeArgument);
const directory = resolve(directoryArgument);
const agentManifest = resolve(agentManifestArgument);
const mediaDirectory = resolve(directory, 'media');
const mediaResultPath = resolve(mediaDirectory, 'result.json');
const processManifestPath = resolve(mediaDirectory, 'process-manifest.json');
const ptyWorkload = resolve('tests/perf/pty-workload.mjs');
const identityHelper = resolve('tests/perf/native-identity.ps1');
const encoderExe = resolve('runtime/build-media/x86_64-pc-windows-msvc/release/examples/native-capture-benchmark.exe');

if (
  UNSAFE_COMMAND_PATH.test(process.execPath)
  || UNSAFE_COMMAND_PATH.test(ptyWorkload)
  || UNSAFE_COMMAND_PATH.test(identityHelper)
  || UNSAFE_COMMAND_PATH.test(encoderExe)
) {
  throw new Error('The Node executable and PTY workload path contain unsafe shell metacharacters');
}

await mkdir(directory, { recursive: false });

const errors = [];
let runResult = null;
const started = new Date().toISOString();

try {
  runResult = await withBrowserFixture(agentExe, directory, async ({ page, sessions, host }) => {
    const session = sessions[0];
    const workloadId = session.session_id;
    const workloadCommand = `"${process.execPath}" "${ptyWorkload}" echo\r`;
    await page.evaluate(
      ({ id, command }) => window.RCPerf.input(id, command),
      { id: workloadId, command: workloadCommand },
    );
    await page.waitForFunction(
      (id) => window.RCPerf.text(id).includes('RC_ECHO_READY'),
      workloadId,
      { timeout: 15_000 },
    );

    const baselineLatencies = await page.evaluate(
      (id) => window.RCPerf.echoSamples(id, 10_000),
      workloadId,
    );

    let mediaPromise;
    let mediaOutcome;
    mediaPromise = execFileP(
      'powershell.exe',
      [
        '-NoProfile',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        resolve('tests/perf/media-smoke.ps1'),
        '-Directory',
        mediaDirectory,
        '-Seconds',
        '60',
        '-AgentManifest',
        agentManifest,
      ],
      { windowsHide: true, timeout: 120_000, maxBuffer: 1_048_576 },
    )
      .then(() => ({ error: null }))
      .catch((error) => ({ error }));

    let stressResult;
    let workloadError;
    let processManifest;
    try {
      await waitForFile(processManifestPath, 15_000);
      processManifest = JSON.parse(await readFile(processManifestPath, 'utf8'));
      const encoderIdentity = Array.isArray(processManifest)
        ? processManifest.find((entry) => entry?.Label === 'rc-media-benchmark')
        : null;
      if (!Number.isInteger(encoderIdentity?.Id) || typeof encoderIdentity.StartTicks !== 'string') {
        throw new Error('Media process manifest has no encoder PID and start identity');
      }
      await sleep(10_000);
      const hmrRun = await withHmrFixture(host, session, directory, async (hmr) => {
        const updatesBefore = hmr.updates;
        const encoderBefore = await verifyEncoder(
          encoderIdentity,
          resolve(directory, 'encoder-identity-before.json'),
        );
        const started = new Date().toISOString();
        const stressLatencies = await page.evaluate(
          (id) => window.RCPerf.echoSamples(id, 10_000),
          workloadId,
        );
        const finished = new Date().toISOString();
        const encoderAfter = await verifyEncoder(
          encoderIdentity,
          resolve(directory, 'encoder-identity-after.json'),
        );
        const hmrUpdatesDuring = hmr.updates - updatesBefore;
        if (hmrUpdatesDuring <= 0) {
          throw new Error('The Vite HMR fixture produced no updates during stress');
        }
        await requireMissing(mediaResultPath);
        return {
          encoderAfter,
          encoderBefore,
          finished,
          hmrUpdatesDuring,
          started,
          stressLatencies,
        };
      });
      stressResult = hmrRun.result;
    } catch (error) {
      workloadError = error;
    } finally {
      mediaOutcome = await mediaPromise;
    }

    if (workloadError && mediaOutcome.error) {
      throw new AggregateError([workloadError, mediaOutcome.error], 'Combined workload failed');
    }
    if (workloadError) throw workloadError;
    if (mediaOutcome.error) throw mediaOutcome.error;

    const mediaReport = JSON.parse(await readFile(mediaResultPath, 'utf8'));
    if (mediaReport?.status !== 'MEASURED') {
      throw new Error(`Media smoke status was ${String(mediaReport?.status)}`);
    }
    if (mediaReport?.report?.encoder !== 'hardware') {
      throw new Error('Media smoke did not report hardware encoding');
    }

    const dropped = await page.evaluate(() => window.RCPerf.perf.dropped);
    if (dropped !== 0) throw new Error(`RCPerf reported ${dropped} dropped events`);

    const baseline = stats(baselineLatencies);
    const stress = stats(stressResult.stressLatencies);
    const p95DeltaMilliseconds = stress.p95 - baseline.p95;
    if (p95DeltaMilliseconds > 10) {
      errors.push(`p95 latency delta exceeded 10ms: ${p95DeltaMilliseconds}ms`);
    }

    return {
      baseline: { latencies: baselineLatencies, stats: baseline },
      hmr_updates_during: stressResult.hmrUpdatesDuring,
      media: mediaReport,
      media_process_manifest: processManifest,
      p95_delta_ms: p95DeltaMilliseconds,
      perf_dropped: dropped,
      scope: 'native capture/encode + Node gateway/Vite HMR + two PTYs; WebRTC/viewer paint not measured',
      stress: {
        encoder_after: stressResult.encoderAfter,
        encoder_before: stressResult.encoderBefore,
        finished: stressResult.finished,
        latencies: stressResult.stressLatencies,
        started: stressResult.started,
        stats: stress,
      },
    };
  });
} catch (error) {
  flattenErrors(error, errors);
}

const result = {
  agent_exe: agentExe,
  agent_manifest: agentManifest,
  errors,
  finished: new Date().toISOString(),
  ...(runResult ?? {}),
  started,
  status: errors.length > 0 ? 'FAIL' : 'MEASURED',
};

await writeFile(
  resolve(directory, 'result.json'),
  `${JSON.stringify(result, null, 2)}\n`,
  { flag: 'wx' },
);
if (errors.length > 0) process.exitCode = 1;
