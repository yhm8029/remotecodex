import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const { default: playwright } = await import(process.env.RC_PLAYWRIGHT_MODULE);
const root = process.cwd();
await fs.mkdir(path.join(root, 'runtime'), { recursive: true });
const dir = await fs.mkdtemp(path.join(root, 'runtime', 'tailscale-ui-'));
const component = path.join(root, 'apps/web/src/TailscaleSetup.svelte').replaceAll('\\', '/');
const mock = `export async function invoke(c,a){window.calls.push({c,a});let v=window.r[c];if(v instanceof Error)throw v;return typeof v==='function'?v(a):structuredClone(v)}`;

await fs.writeFile(path.join(dir, 'index.html'), `<script type="module">
  import { mount, unmount } from 'svelte';
  import C from '/@fs/${component}';
  const q = new URLSearchParams(location.search);
  window.calls = [];
  window.r = {
    tailscale_setup_status: { state: q.get('s') || 'not_installed', detail: 'x' },
    tailscale_status: { ownership: q.get('o') || 'absent', public_origin: q.get('p') || undefined, restart_required: q.has('r'), detail: 'x' },
    tailscale_setup_install: () => { const code = q.get('i') || 'installed'; if (code === 'installed' || code === 'already_installed') window.r.tailscale_setup_status = { state: 'login_required', detail: 'x' }; return { code }; },
    tailscale_setup_login: null,
    set_tailscale_serve: () => ({ ownership: 'owned', public_origin: 'https://a.ts.net', detail: 'x' })
  };
  window.u = mount(C, { target: document.body, props: { native: q.get('n') !== '0', role: q.get('role') || 'host', canConfigureHost: q.get('c') === '1' } });
  window.destroy = () => unmount(window.u);
</script>`);

const server = await createServer({ root: dir, configFile: false, plugins: [{ name: 'mock-tauri', enforce: 'pre', resolveId: id => id === '@tauri-apps/api/core' ? '\0mock-tauri' : undefined, load: id => id === '\0mock-tauri' ? mock : undefined }, svelte()], server: { host: '127.0.0.1', port: 0, fs: { allow: [root, dir] } } });
await server.listen();
const browser = await playwright.chromium.launch({ headless: true });
const base = `http://127.0.0.1:${server.httpServer.address().port}`;
const results = [];
async function page(query = '') {
  const p = await browser.newPage();
  await p.goto(`${base}?${query}`);
  if (query.includes('n=0')) {
    assert.equal(await p.getByTestId('open').textContent(), '원격 연결 준비하기');
    await p.getByTestId('open').click();
  } else {
    assert.equal(await p.getByTestId('open').count(), 0);
  }
  await p.getByTestId('close').waitFor();
  return p;
}
async function seen(p, command, count = 1) { await p.waitForFunction(([command, count]) => window.calls.filter(call => call.c === command).length >= count, [command, count]); }
async function test(name, fn) { await fn(); results.push(name); }

try {
  await test('fresh host connected before Agent never reads Serve', async () => {
    const p = await page('s=connected&role=host');
    await seen(p, 'tailscale_setup_status');
    assert.match(await p.locator('section').textContent(), /Agent 시작 또는 연결/);
    assert.equal(await p.evaluate(() => window.calls.some(call => call.c === 'tailscale_status')), false);
    assert.equal(await p.getByTestId('serve').count(), 0);
    await p.close();
  });
  await test('login deadline stops and connected poll checks host address', async () => {
    let p = await page('s=login_required');
    await p.clock.install();
    await p.getByTestId('login').click();
    await seen(p, 'tailscale_setup_login');
    await p.getByTestId('message').waitFor();
    await p.clock.fastForward(121000);
    assert.match(await p.getByTestId('message').textContent(), /연결 확인 시간이 지났습니다/);
    const count = await p.evaluate(() => window.calls.length);
    await p.clock.fastForward(10000);
    assert.equal(await p.evaluate(() => window.calls.length), count);
    await p.close();
    p = await page('s=login_required&c=1&o=owned&p=https%3A%2F%2Fa.ts.net');
    await p.clock.install();
    await p.getByTestId('login').click();
    await seen(p, 'tailscale_setup_login');
    await p.getByTestId('message').waitFor();
    await p.evaluate(() => window.r.tailscale_setup_status = { state: 'connected', detail: 'x' });
    await p.clock.fastForward(3000);
    await seen(p, 'tailscale_status');
    await p.getByTestId('ready').waitFor();
    assert.equal(await p.evaluate(() => window.calls.some(call => call.c === 'set_tailscale_serve')), false);
    await p.close();
  });
  await test('browser0IPC', async () => { const p = await page('n=0'); assert.equal(await p.evaluate(() => window.calls.length), 0); assert.equal(await p.getByRole('link').count(), 4); await p.close(); });
  await test('both roles bootstrap automatically and never show an install button', async () => { for (const role of ['host', 'client']) { const p = await page(`role=${role}`); await seen(p, 'tailscale_setup_status'); await seen(p, 'tailscale_setup_install'); await p.waitForFunction(() => document.querySelector('[data-testid=login]')); assert.equal(await p.getByTestId('install').count(), 0); assert.equal(await p.evaluate(() => window.calls.some(call => call.c === 'set_tailscale_serve')), false); await p.close(); } });
  await test('host consent, conflict, client no serve', async () => { let p = await page('s=connected&role=host&c=1'); await seen(p, 'tailscale_status'); assert.equal(await p.evaluate(() => window.calls.some(call => call.c === 'set_tailscale_serve')), false); await p.getByRole('checkbox').check(); await p.getByTestId('serve').click(); await seen(p, 'set_tailscale_serve'); assert.deepEqual(await p.evaluate(() => window.calls.find(call => call.c === 'set_tailscale_serve').a), { request: { enabled: true, consent: true } }); await p.close(); p = await page('s=connected&role=host&c=1&o=conflict'); await seen(p, 'tailscale_status'); assert.equal(await p.getByTestId('serve').count(), 0); await p.close(); p = await page('s=connected&role=client'); await seen(p, 'tailscale_setup_status'); assert.equal(await p.evaluate(() => window.calls.some(call => call.c === 'tailscale_status')), false); await p.close(); });
  await test('ready requires origin and no restart', async () => { let p = await page('s=connected&role=host&c=1&o=owned'); await seen(p, 'tailscale_status'); assert.equal(await p.getByTestId('ready').count(), 0); await p.close(); p = await page('s=connected&role=host&c=1&o=owned&p=https%3A%2F%2Fa.ts.net&r=1'); await seen(p, 'tailscale_status'); assert.equal(await p.getByTestId('ready').count(), 0); await p.close(); p = await page('s=connected&role=host&c=1&o=owned&p=https%3A%2F%2Fa.ts.net'); await seen(p, 'tailscale_status'); await p.waitForFunction(() => document.querySelector('[data-testid=ready]')); await p.close(); });
  await test('poll cleanup close and destroy', async () => { for (const action of ['close', 'destroy']) { const p = await page('s=login_required'); await seen(p, 'tailscale_setup_status'); await p.getByTestId('login').click(); await seen(p, 'tailscale_setup_login'); const calls = await p.evaluate(() => window.calls.length); if (action === 'close') await p.getByTestId('close').click(); else await p.evaluate(() => window.destroy()); await p.waitForTimeout(3100); assert.equal(await p.evaluate(() => window.calls.length), calls); await p.close(); } });
  await test('bootstrap failure shows explicit retry and reboot remains latched', async () => { for (const code of ['cancelled', 'approval_denied']) { const p = await page(`i=${code}`); await seen(p, 'tailscale_setup_install'); await p.getByTestId('message').waitFor(); await p.getByTestId('retry-bootstrap').click(); await seen(p, 'tailscale_setup_install', 2); await p.close(); } const p = await page('i=reboot_required'); await seen(p, 'tailscale_setup_install'); assert.equal(await p.getByTestId('retry-bootstrap').count(), 0); await p.evaluate(() => window.r.tailscale_setup_status = { state: 'connected', detail: 'x' }); await p.getByTestId('refresh').click(); await seen(p, 'tailscale_setup_status', 2); assert.equal(await p.getByTestId('ready').count(), 0); await p.close(); });
  await test('refresh failure clears stale ready', async () => { const p = await page('s=connected&role=host&c=1&o=owned&p=https%3A%2F%2Fa.ts.net'); await seen(p, 'tailscale_status'); await p.waitForFunction(() => document.querySelector('[data-testid=ready]')); await p.evaluate(() => window.r.tailscale_setup_status = Error('x')); await p.getByTestId('refresh').click(); await p.waitForSelector('[role=alert]'); assert.equal(await p.getByTestId('ready').count(), 0); await p.close(); });
} finally { await browser.close(); await server.close(); await fs.rm(dir, { recursive: true, force: true }); }
await fs.writeFile(path.join(root, 'docs/test-results/tailscale-onboarding-2026-09-12/ui.json'), JSON.stringify({status:'PASS',groups:results}, null, 2));
console.log(JSON.stringify(results));
