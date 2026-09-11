import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { withBrowserFixture } from './browser-harness.mjs';

const [agentExe, outputArg] = process.argv.slice(2);
if (!agentExe || !outputArg) {
  console.error('usage: credit-timeout.mjs <agentExe> <outputJSON>');
  process.exit(2);
}
const outputFile = resolve(outputArg);
const errors = [];
const timings = {};

async function waitFor(page, expression, timeout = 5000, arg) {
  await page.waitForFunction(expression, arg, { timeout });
}

async function probeState(page, name) {
  return page.evaluate((key) => {
    const probe = globalThis[key];
    return probe ? { ready: probe.ready, closed: probe.closed, outputCount: probe.outputCount, lastSequence: probe.lastSequence } : null;
  }, name);
}

let result;
try {
  result = await withBrowserFixture(agentExe, dirname(outputFile), async ({ page, sessions }) => {
    const activeId = sessions[0].session_id;
    const started = performance.now();
    try {
    await page.evaluate(async (id) => { globalThis.__creditProbe1 = await window.RCPerf.openCreditProbe(window.RCPerf.controller.api, id); }, activeId);
    await waitFor(page, () => globalThis.__creditProbe1?.ready, 10000);
    await page.waitForTimeout(21000);
    await page.evaluate(() => { globalThis.__creditProbe1.mode = 'delayed'; });
    const firstBefore = (await probeState(page, '__creditProbe1')).outputCount;
    await page.evaluate((id) => window.RCPerf.controller.input(id, 'echo RC_CREDIT_TIMEOUT_1\r'), activeId);
    await waitFor(page, (minimum) => (globalThis.__creditProbe1?.outputCount ?? 0) > minimum, 10000, firstBefore);
    const firstOutputAt = performance.now();
    await page.waitForTimeout(100);
    const afterAck = await probeState(page, '__creditProbe1');
    if (!afterAck || afterAck.closed) throw new Error('delayed probe closed before ACK grace check');
    const secondBefore = afterAck.outputCount;
    await page.evaluate((id) => window.RCPerf.controller.input(id, 'echo RC_CREDIT_TIMEOUT_2\r'), activeId);
    await waitFor(page, (minimum) => (globalThis.__creditProbe1?.outputCount ?? 0) > minimum, 10000, secondBefore);
    const secondAfter = await probeState(page, '__creditProbe1');
    if (!secondAfter || secondAfter.closed || secondAfter.outputCount <= secondBefore) throw new Error('second output did not arrive on delayed probe socket');
    timings.probe1 = { quiet_ms: 21000, first_output_ms: firstOutputAt - started, first_output_count: firstBefore, second_output_count: secondAfter.outputCount };

    await page.evaluate(async (id) => { globalThis.__creditProbe2 = await window.RCPerf.openCreditProbe(window.RCPerf.controller.api, id, 'duplicate'); }, activeId);
    await waitFor(page, () => globalThis.__creditProbe2?.ready, 10000);
    await page.evaluate(() => { globalThis.__creditProbe2.mode = 'duplicate'; });
    const duplicateStarted = performance.now();
    await page.evaluate((id) => {
      window.__creditExtra = setInterval(() => {
        if (!window.RCPerf.controller.connected) return;
        try { window.RCPerf.controller.input(id, 'echo RC_CREDIT_EXTRA\r'); } catch {}
      }, 5000);
      window.RCPerf.controller.input(id, 'echo RC_CREDIT_DUPLICATE\r');
    }, activeId);
    await waitFor(page, () => globalThis.__creditProbe2?.closed, 23000);
    const closeMs = performance.now() - duplicateStarted;
    if (closeMs < 19000 || closeMs > 23000) throw new Error(`duplicate probe closed outside 19..23s: ${closeMs}`);
    timings.probe2 = { close_ms: closeMs, output_count: (await probeState(page, '__creditProbe2'))?.outputCount ?? 0 };
    return { status: 'PASS', timings };
    } finally {
      await page.evaluate(() => {
        if (globalThis.__creditExtra) clearInterval(globalThis.__creditExtra);
        globalThis.__creditProbe1?.close?.();
        globalThis.__creditProbe2?.close?.();
        delete globalThis.__creditExtra;
        delete globalThis.__creditProbe1;
        delete globalThis.__creditProbe2;
      }).catch(() => {});
    }
  });
} catch (error) {
  errors.push(error?.message ?? String(error), ...(error?.errors ?? []).flatMap((inner) => [inner?.message ?? String(inner)]));
}

const report = { status: errors.length ? 'FAIL' : result?.status ?? 'FAIL', timings: result?.timings ?? timings, errors };
await mkdir(dirname(outputFile), { recursive: true });
await writeFile(outputFile, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
if (errors.length || report.status !== 'PASS') process.exitCode = 1;
