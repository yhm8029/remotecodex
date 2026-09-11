import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { withBrowserFixture } from './browser-harness.mjs';
import { stats } from './stats.mjs';

const [, , agentExe, outPath] = process.argv;
if (!agentExe || !outPath) {
  console.error('Usage: node ctrlc.mjs <agentExe> <outJson>');
  process.exit(1);
}

const META_RE = /["\r\n&|<>^]/;
function quote(p) {
  if (META_RE.test(p)) throw new Error(`unsafe path: ${p}`);
  return `"${p}"`;
}

const started = new Date().toISOString();
const result = { scope: 'Agent receive to PTY write, not target program termination', started };
let exitCode = 0;

try {
  await withBrowserFixture(agentExe, resolve(outPath, '..'), async ({ page, sessions }) => {
    const [first, second] = sessions;
    const firstId = first.session_id;
    const secondId = second.session_id;

    const baseline = await page.evaluate(id => window.RCPerf.ctrlCSamples(id, 1000), firstId);

    const workloadCmd = `${quote(process.execPath)} ${quote(resolve('tests/perf/pty-workload.mjs'))} output 5242880 5\r`;
    await page.evaluate(({ id, cmd }) => window.RCPerf.controller.input(id, cmd), { id: secondId, cmd: workloadCmd });

    await page.waitForTimeout(500);
    const midText = await page.evaluate(id => window.RCPerf.text(id), secondId);
    if (midText.includes('RC_OUTPUT_DONE:')) throw new Error('marker appeared before stress');

    const stress = await page.evaluate(id => window.RCPerf.ctrlCSamples(id, 100), firstId);

    const midText2 = await page.evaluate(id => window.RCPerf.text(id), secondId);
    if (midText2.includes('RC_OUTPUT_DONE:')) throw new Error('marker appeared during stress');

    await page.waitForFunction(id => {
      const t = window.RCPerf.text(id);
      return t.includes('RC_OUTPUT_DONE:');
    }, secondId, { timeout: 15000 });

    const dropped = await page.evaluate(() => window.RCPerf.perf.dropped);
    if (dropped !== 0) throw new Error(`dropped=${dropped}`);

    result.baseline = baseline;
    result.stress = stress;
    result.baselineStats = stats(baseline);
    result.stressStats = stats(stress);
    result.status = 'MEASURED';
  });
} catch (e) {
  exitCode = 1;
  const flat = (err) => {
    if (!err) return String(err);
    if (err instanceof AggregateError) return err.errors.map(flat).join(' | ');
    return err.stack || err.message || String(err);
  };
  result.status = 'FAIL';
  result.error = flat(e);
}

result.finished = new Date().toISOString();
await writeFile(outPath, JSON.stringify(result, null, 2), { flag: 'wx' });
process.exit(exitCode);
