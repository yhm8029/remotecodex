import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { HostClient } from './host-client.mjs';
import { nativeCycle } from './native-cycle.mjs';
import { stats } from './stats.mjs';
import { measureNative } from './native-resource.mjs';

async function measureButtonGeometry(page, button) {
        const result = await button.evaluate(el => {
            const r = el.getBoundingClientRect();
            const cx = r.x + r.width / 2;
            const cy = r.y + r.height / 2;
            const hit = document.elementFromPoint(cx, cy);
            return {
                innerWidth: window.innerWidth,
                innerHeight: window.innerHeight,
                visualWidth: visualViewport?.width,
                visualHeight: visualViewport?.height,
                rect: { x: r.x, y: r.y, width: r.width, height: r.height },
                hitTag: hit?.tagName,
                hitClass: hit?.className,
                targetHit: hit === el || el.contains(hit)
            };
        });
        return result;
}
const execFileP = promisify(execFile);
const [exeArg, agentArg, directoryArg, countArg = '50', resourceModeArg, agentManifestArg] = process.argv.slice(2);
const count = Number(countArg);
const resourceMode = resourceModeArg;
const resourceArgsValid = resourceModeArg === undefined
  ? agentManifestArg === undefined
  : ['idle', 'output'].includes(resourceModeArg) && count === 1 && typeof agentManifestArg === 'string' && agentManifestArg.length > 0;
if (!exeArg || !agentArg || !directoryArg || !/^\d+$/.test(countArg) || !Number.isInteger(count) || count < 1 || count > 50 || !resourceArgsValid) {
  console.error('usage: native-lifecycle.mjs <exe> <agentExe> <directory> [count 1..50] [idle|output <agentManifest>]');
  process.exit(2);
}

const directory = resolve(directoryArg);
const profile = resolve(directory, 'profile');
const exe = resolve(exeArg);
const agentExe = resolve(agentArg);
const agentManifest = agentManifestArg ? resolve(agentManifestArg) : null;
const helperDir = resolve('tests/perf');
const ids = new Set();
const results = [];
const errors = [];
const started = new Date().toISOString();
let host;
let keeper;

const messages = (error) => [error?.message ?? String(error), ...(error?.errors ?? []).flatMap(messages)];
const ps = async (script, args) => execFileP('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve(helperDir, script), ...args], { windowsHide: true, timeout: 20000 });

try {
  await mkdir(directory, { recursive: false });
  await mkdir(profile, { recursive: false });
  const { chromium } = await import(process.env.RC_PLAYWRIGHT_MODULE);
  host = new HostClient(agentExe);
  await host.pair();
  keeper = await host.create('native-keeper', tmpdir());
  for (let index = 0; index < count; index++) {
    const children = resolve(directory, `children-${index}.json`);
    let resourceInfo;
    const result = await nativeCycle({
      exe, agentExe, profile, directory, index, first: index === 0, chromium, keeper,
      onPaired: (clientId) => ids.add(clientId),
      beforeClose: async ({ page, identityFile }) => {
        const clientId = await page.evaluate(() => new Promise((resolve, reject) => {
          const request = indexedDB.open('remotecodex-device-v1');
          request.onerror = () => reject(request.error);
          request.onsuccess = () => {
            const db = request.result;
            const get = db.transaction('devices', 'readonly').objectStore('devices').get('http://127.0.0.1:3847');
            get.onerror = () => { db.close(); reject(get.error); };
            get.onsuccess = () => { const id = get.result?.clientId; db.close(); resolve(id ?? null); };
          };
        }));
        if (typeof clientId === 'string' && clientId) ids.add(clientId);
        await ps('native-descendants.ps1', ['-Identity', identityFile, '-Output', children]);
        const accept = (dialog) => dialog.accept().catch(() => {});
        page.on('dialog', accept);
        try {
          const acquireButton = page.getByRole('button', { name: '\uC81C\uC5B4\uAD8C \uAC00\uC838\uC624\uAE30', exact: true });
          const geometry = await measureButtonGeometry(page, acquireButton);
          await writeFile(resolve(directory, 'geometry-' + index + '.json'), JSON.stringify(geometry, null, 2), { flag: 'wx' });
          if (!geometry.targetHit) throw new Error('native button hit-test mismatch; inspect geometry-' + index + '.json');
          await acquireButton.click();
          await page.getByRole('button', { name: '\uC81C\uC5B4\uAD8C \uD574\uC81C', exact: true }).waitFor({ state: 'visible', timeout: 5000 });
          await page.locator('.xterm-helper-textarea').focus();
          await page.keyboard.type(`echo RC_NATIVE_${index}`);
          await page.keyboard.press('Enter');
        } finally { page.off('dialog', accept); }
        const deadline = Date.now() + 5000;
        let markerFound = false;
        while (Date.now() < deadline) {
          const projection = await host.api(`/sessions/${keeper.session_id}/projection`);
          if (projection?.projection?.lines?.some((line) => line.trim() === `RC_NATIVE_${index}`)) { markerFound = true; break; }
          await new Promise((r) => setTimeout(r, 100));
        }
        if (!markerFound) throw new Error(`native marker missing for cycle ${index}`);
        if (resourceMode) resourceInfo = await measureNative({ page, identityFile, keeper, agentManifest, directory, mode: resourceMode, duration: 600 });
        if (resourceMode === 'output') {
          const outputDeadline = Date.now() + 20000;
          let output = null;
          while (Date.now() < outputDeadline) {
            const current = await host.api(`/sessions/${keeper.session_id}/projection`);
            const text = (current?.projection?.lines ?? []).join('');
            const match = text.match(/RC_OUTPUT_DONE:(\{[\s\S]*?\})/);
            if (match) { output = JSON.parse(match[1].replace(/\s+/g, '')); break; }
            await new Promise((r) => setTimeout(r, 100));
          }
          if (!output || !Number.isFinite(output.achieved_bytes_per_second) || output.achieved_bytes_per_second < 102400 * 0.9) throw new Error(`resource output throughput below target for cycle ${index}`);
          resourceInfo.throughput = output;
        }
      },
    });
    result.resource = resourceInfo;
    await ps('native-exited.ps1', ['-Manifest', children]);
    const session = (await host.api('/sessions')).find((item) => item.session_id === keeper.session_id);
    if (!session || session.state !== 'running' || session.pid !== keeper.pid || session.process_created !== keeper.process_created) {
      throw new Error(`keeper identity changed after cycle ${index}`);
    }
    results.push(result);
    console.log(`native cycle ${index + 1}/${count}`);
  }
} catch (error) { errors.push(...messages(error)); }

const cleanupErrors = [];
if (host) for (const id of ids) {
  try { await host.api('/devices/' + encodeURIComponent(id), undefined, 'DELETE'); }
  catch (error) { cleanupErrors.push(...messages(error)); }
}
try { if (host) await host.cleanup(); } catch (error) { cleanupErrors.push(...messages(error)); }
errors.push(...cleanupErrors);
const report = {
  started, finished: new Date().toISOString(), count, completed: results.length,
  results, stats: stats(results.map((item) => item.elapsed_ms)), errors, cleanup: cleanupErrors.length === 0,
};
await writeFile(resolve(directory, 'result.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
if (errors.length) process.exitCode = 1;
