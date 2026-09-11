import { promises as fs } from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { HostClient } from './host-client.mjs';
import { build } from 'esbuild';

const execFileP = promisify(execFile);

export async function withBrowserFixture(agentExe, runDirectory, body) {
  const host = new HostClient(agentExe);
  const ownedFiles = [];
  const cleanupErrors = [];
  let browser;
  let page;
  let fixture;
  let listenerTasks = [];
  let capturedClientId = null;
  let sessions = [];

  const addOwned = async (filePath, contents) => {
    await fs.writeFile(filePath, contents, { flag: 'wx' });
    ownedFiles.push(filePath);
  };

  const runBody = async () => {
    await host.pair();

    const tmp = os.tmpdir();
    const s1 = await host.create('perf-cmd-1', tmp);
    sessions.push(s1);
    const s2 = await host.create('perf-cmd-2', tmp);
    sessions.push(s2);

    const buildResult = await build({
      entryPoints: ['tests/perf/browser-fixture.ts'],
      bundle: true,
      format: 'iife',
      globalName: 'RCPerf',
      platform: 'browser',
      write: false,
      minify: true,
    });
    const bundleJs = buildResult.outputFiles[0].text;

    const distDir = path.resolve('apps/web/dist');
    await fs.mkdir(distDir, { recursive: true });
    const stem = crypto.randomUUID();
    const htmlPath = path.join(distDir, `${stem}.html`);
    const jsPath = path.join(distDir, `${stem}.js`);
    const cssPath = path.join(distDir, `${stem}.css`);

    const xtermCssPath = fileURLToPath(
      new URL('../../node_modules/@xterm/xterm/css/xterm.css', import.meta.url)
    );
    const cssContent = await fs.readFile(xtermCssPath);

    await addOwned(htmlPath,
      `<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="./${stem}.css"></head><body><script src="./${stem}.js"></script></body></html>`
    );
    await addOwned(jsPath, bundleJs);
    await addOwned(cssPath, cssContent);

    const { chromium } = await import(process.env.RC_PLAYWRIGHT_MODULE);
    browser = await chromium.launch({
      executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe',
      headless: true,
    });
    page = await browser.newPage();

    page.on('response', (resp) => {
      try {
        const url = resp.url();
        if (url.includes('/auth/pair') && resp.status() >= 200 && resp.status() < 300) {
          const task = resp.json().then((j) => { if (j && j.client_id) capturedClientId = j.client_id; }).catch(() => {});
          listenerTasks.push(task);
        }
      } catch {}
    });

    const agentResolved = path.resolve(agentExe);
    const { stdout } = await execFileP(agentResolved, ['pair'], { windowsHide: true, timeout: 5000, maxBuffer: 16384 });
    const ticket = JSON.parse(stdout).ticket;

    const pageUrl = `http://127.0.0.1:3847/${stem}.html`;
    await page.goto(pageUrl);
    fixture = await page.evaluate((arg) => window.RCPerf.init(arg.ticket, arg.sessions), { ticket, sessions });

    const result = await body({ page, sessions, host });

    return result;
  };
let result;
try {
  result = await runBody();
} catch (error) {
  cleanupErrors.push(error);
} finally {
  if (page) {
    try { await page.evaluate(() => window.RCPerf?.stop()); } catch (e) { cleanupErrors.push(e); }
  }
  if (browser) {
    try { await browser.close(); } catch (e) { cleanupErrors.push(e); }
  }
  try { await Promise.all(listenerTasks); } catch (e) { cleanupErrors.push(e); }
  if (capturedClientId) {
    try { await host.api('/devices/' + encodeURIComponent(capturedClientId), undefined, 'DELETE'); } catch (e) { cleanupErrors.push(e); }
  }
  try { await host.cleanup(); } catch (e) { cleanupErrors.push(e); }
  for (const path of ownedFiles) {
    try { await fs.unlink(path); } catch (e) { cleanupErrors.push(e); }
  }
}
if (cleanupErrors.length) throw new AggregateError(cleanupErrors, 'Browser fixture failed');
return result;
}
