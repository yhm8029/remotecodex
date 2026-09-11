import { promises as fs } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { gzipSync } from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { withBrowserFixture } from './browser-harness.mjs';
import { stats } from './stats.mjs';

const [agentExe, outputArg, mode = 'normal', samplesArg = mode === 'burst' ? '100' : '10000', durationArg = '600'] = process.argv.slice(2);
const samples = Number(samplesArg);
const durationSeconds = Number(durationArg);
const validInt = (value, min, max) => /^\d+$/.test(value) && Number.isInteger(Number(value)) && Number(value) >= min && Number(value) <= max;
const usage = 'usage: browser-benchmark.mjs <agentExe> <outputFile> [normal|stress|burst] [samples 1..10000] [duration 1..600]';

if (!agentExe || !outputArg || !['normal', 'stress', 'burst'].includes(mode) || !validInt(samplesArg, 1, 10000) || !validInt(durationArg, 1, 600)) {
  console.error(usage);
  process.exit(2);
}

const outputFile = resolve(outputArg);
const helper = resolve('tests/perf/pty-workload.mjs');
const unsafe = (value) => /["\r\n&|<>^]/.test(value);
const quotedCommand = () => {
  if (unsafe(process.execPath) || unsafe(helper)) throw new Error('workload path contains unsafe command characters');
  return `"${process.execPath}" "${helper}"`;
};

function stageMetrics(events, sessionId) {
  const stages = {};
  for (const name of ['client_input_enqueue', 'agent_input_write', 'client_output_parse_apply']) {
    stages[name] = stats(events.filter((event) => event.session_id === sessionId && event.stage === name).map((event) => event.elapsed_ms));
  }
  return stages;
}

async function runScenario({ page, sessions, host }, runMode, runSamples, runDuration, baseCmd) {
  const first = sessions[0].session_id;
  const second = sessions[1].session_id;
  const firstIdentity = { pid: sessions[0].pid, process_created: sessions[0].process_created };
  const result = { firstId: first, recoverySessionId: first, echoMs: [], reconnectMs: [], events: [], recoveryEchoMs: [], recoveryEvents: [], workload: null, perf: null, backgroundOutput: null, rawMetricScope: 'active-session-only', viewReady: false, controllerConnected: false };
  const waitText = async (id, marker, timeout) => page.waitForFunction(({ id: sessionId, marker: text }) => window.RCPerf.text(sessionId).includes(text), { id, marker }, { timeout });
  try {
  await page.evaluate(({ id, cmd }) => window.RCPerf.input(id, cmd), { id: first, cmd: `${baseCmd} echo\r` });
  await waitText(first, 'RC_ECHO_READY', 15000);
  await page.waitForTimeout(300);
  if (runMode === 'normal') {
    result.echoMs = await page.evaluate(({ id, count }) => window.RCPerf.echoSamples(id, count), { id: first, count: runSamples });
    result.events = await page.evaluate(() => window.RCPerf.perf.events);
    result.reconnectMs = await page.evaluate((id) => window.RCPerf.reconnectSamples(id, 100), first);
  } else {
    const rate = runMode === 'stress' ? 1048576 : 5242880;
    const seconds = runMode === 'stress' ? runDuration : 5;
    await page.evaluate(({ id, cmd }) => window.RCPerf.controller.input(id, cmd), { id: second, cmd: `${baseCmd} output ${rate} ${seconds}\r` });
    await page.waitForTimeout(1000);
    const beforeEcho = await page.evaluate((id) => ({
      done: window.RCPerf.text(id).includes('RC_OUTPUT_DONE:'),
      events: window.RCPerf.perf.events,
      dropped: window.RCPerf.perf.dropped,
      background: window.RCPerf.backgroundOutput.get(id) ?? null,
    }), second);
    const outputActivityBeforeEcho = (beforeEcho.background?.events ?? 0) > 0;
    result.workload = { target_bytes_per_second: rate, overlap: { output_activity_before_echo: outputActivityBeforeEcho, output_done_before_echo: beforeEcho.done, output_done_after_echo: false }, early_dropped: beforeEcho.dropped };
    result.echoMs = await page.evaluate(({ id, count }) => window.RCPerf.echoSamples(id, count), { id: first, count: runSamples });
    const doneAfterEcho = await page.evaluate((id) => window.RCPerf.text(id).includes('RC_OUTPUT_DONE:'), second);
    result.workload.overlap.output_done_after_echo = doneAfterEcho;
    if (beforeEcho.done || doneAfterEcho) {
      result.workload.error = 'input samples outlasted output workload';
      result.workload.overlap.overlap = false;
    } else {
      result.workload.overlap.overlap = true;
      result.workload.overlap.description = 'echo sequence completed before workload completion marker was observed';
    }
    await waitText(second, 'RC_OUTPUT_DONE:', (seconds + 30) * 1000);
    const eventsBeforeRecovery = await page.evaluate(() => ({ events: window.RCPerf.perf.events, dropped: window.RCPerf.perf.dropped }));
    result.events = eventsBeforeRecovery.events;
    result.perf = { early_dropped: beforeEcho.dropped, pre_recovery_dropped: eventsBeforeRecovery.dropped };
    result.recoveryEchoMs = await page.evaluate((id) => window.RCPerf.echoSamples(id, 100), first);
    result.recoveryEvents = await page.evaluate(() => window.RCPerf.perf.events);
    const text = await page.evaluate((id) => window.RCPerf.text(id), second);
    const match = text.match(/RC_OUTPUT_DONE:(\{[\s\S]*?\})/);
    if (!match) throw new Error('output completion JSON missing');
    const workload = JSON.parse(match[1].replace(/\s+/g, ''));
    if (!Number.isFinite(workload.achieved_bytes_per_second) || workload.achieved_bytes_per_second < rate * 0.9) {
      result.workload = { ...result.workload, ...workload, target_bytes_per_second: rate, throughput_error: 'throughput below 90% target' };
      result.workload.error ??= result.workload.throughput_error;
    } else {
      result.workload = { ...result.workload, ...workload, target_bytes_per_second: rate };
    }
    const finalPerf = await page.evaluate(() => ({ dropped: window.RCPerf.perf.dropped, event_count: window.RCPerf.perf.events.length }));
    result.backgroundOutput = await page.evaluate((id) => window.RCPerf.backgroundOutput.get(id) ?? null, second);
    result.perf = { ...result.perf, final_dropped: finalPerf.dropped, event_count: result.events.length, recovery_event_count: result.recoveryEvents.length };
  }
  if (!result.perf) result.perf = await page.evaluate(() => ({ final_dropped: window.RCPerf.perf.dropped, event_count: window.RCPerf.perf.events.length }));
  const drops = Object.entries(result.perf).filter(([key]) => key.includes('dropped')).reduce((sum, [, value]) => sum + (Number.isFinite(value) ? value : 0), 0);
  if (drops > 0) result.instrumentation_failure = `perf buffer dropped ${drops} events`;
  const readiness = await page.evaluate((ids) => ({
    viewReady: ids.every((id) => window.RCPerf.views.get(id)?.ready === true),
    controllerConnected: window.RCPerf.controller.connected === true,
  }), [first, second]);
  result.viewReady = readiness.viewReady;
  result.controllerConnected = readiness.controllerConnected;
  const current = (await host.api('/sessions')).find((session) => session.session_id === first);
  if (!current || current.pid !== firstIdentity.pid || current.process_created !== firstIdentity.process_created) {
    throw new Error('first session identity changed');
  }
  return result;
  } catch (error) {
    error.partial = result;
    throw error;
  }
}

const started = new Date().toISOString();
let status = 'FAIL';
let result = null;
let error = null;
try {
  const baseCmd = quotedCommand();
  result = await withBrowserFixture(agentExe, dirname(outputFile), (fixture) => runScenario(fixture, mode, samples, durationSeconds, baseCmd));
  if (result.workload?.error || result.instrumentation_failure) throw new Error(result.workload?.error ?? result.instrumentation_failure);
  status = 'MEASURED';
} catch (cause) {
  error = cause;
  result = cause.partial ?? cause.errors?.find((inner) => inner?.partial)?.partial ?? null;
}

const finished = new Date().toISOString();
const events = result?.events ?? [];
const summary = {
  started, finished, mode, samples, build_mode: 'Release', input_method: 'public xterm input -> onData',
  render_boundary: 'xterm parse/apply callback, not paint', status,
  error: error ? [error.message, ...(error.errors ?? []).map((inner) => inner?.message ?? String(inner))] : result?.workload?.error ?? null,
  metrics: result ? { echoMs: stats(result.echoMs), reconnectMs: stats(result.reconnectMs), recoveryEchoMs: stats(result.recoveryEchoMs), stages: stageMetrics(events, result.firstId), recoveryStages: stageMetrics(result.recoveryEvents, result.recoverySessionId) } : { echoMs: stats([]), reconnectMs: stats([]), recoveryEchoMs: stats([]), stages: {}, recoveryStages: {} },
  workload: result?.workload ?? null,
  perf: result?.perf ?? null,
  background_output: result?.backgroundOutput ?? null,
  raw_metric_scope: result?.rawMetricScope ?? 'active-session-only',
  instrumentation_failure: result?.instrumentation_failure ?? null,
  view_ready: result?.viewReady ?? false,
  controller_connected: result?.controllerConnected ?? false,
};

await mkdir(dirname(outputFile), { recursive: true });
if (result) await writeFile(outputFile + '.raw.json.gz', gzipSync(JSON.stringify({ summary, result })), { flag: 'wx' });
await writeFile(outputFile, JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
if (status !== 'MEASURED') process.exitCode = 1;
