import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const { chromium } = await import(process.env.RC_PLAYWRIGHT_MODULE);
const root = process.cwd();
const webRoot = path.join(root, 'apps', 'web');
const ticket = 'a'.repeat(43);
const origin = 'https://office.ts.net';
const label = 'Office';
const invite = `${origin}/#rc-invite=${Buffer.from(JSON.stringify({ v: 1, origin, label, ticket })).toString('base64url')}`;

const mockTerminal = `
export class AgentApi {
  constructor(base) { this.base = base; window.apiCalls ??= []; window.apiCalls.push({ op: 'construct', base }); }
  async pair(value, name) { window.apiCalls.push({ op: 'pair', ticket: value, label: name, base: this.base }); }
  async authenticate() {}
  async request(path) {
    window.apiCalls.push({ op: 'request', path, base: this.base });
    if (path === '/profiles') return [];
    if (path === '/history') return [];
    if (path === '/sessions') return [];
    if (path === '/host') return { client_id: 'fixture', scopes: ['terminal.read'], agent_epoch: 1 };
    return {};
  }
  async websocket() { return { readyState: 1, close() {}, send() {} }; }
}
export class Controller extends EventTarget {
  constructor(api) { super(); this.api = api; this.sessions = []; this.host = null; this.connected = false; this.notice = ''; }
  async connect() { this.host = { client_id: 'fixture', scopes: ['terminal.read'], agent_epoch: 1 }; this.connected = true; this.dispatchEvent(new Event('change')); }
  stop() { this.connected = false; }
  fail(error) { this.notice = String(error); this.dispatchEvent(new Event('change')); }
}
export class TerminalView {}
export class MediaClient {}
`;
const mockTauri = `
export async function invoke(command, args) {
  window.tauriCalls ??= []; window.tauriCalls.push({ command, args });
  if (command === 'ensure_host') return 'attached';
  if (command === 'local_admin') return { ticket: 'local-' + 'b'.repeat(32) };
  return null;
}
`;

const server = await createServer({
  root: webRoot,
  configFile: false,
  plugins: [
    svelte(),
    { name: 'mock-terminal', enforce: 'pre', resolveId: (id) => id === '@remotecodex/terminal-client' ? '\0mock-terminal' : undefined, load: (id) => id === '\0mock-terminal' ? mockTerminal : undefined },
    { name: 'mock-tauri', enforce: 'pre', resolveId: (id) => id === '@tauri-apps/api/core' ? '\0mock-tauri' : undefined, load: (id) => id === '\0mock-tauri' ? mockTauri : undefined },
  ],
  server: { host: '127.0.0.1', port: 0, hmr: false, hmr: false, fs: { allow: [root] } },
});
await server.listen();
const port = server.httpServer.address().port;
console.log(`fixture server ${port}`);
const browser = await chromium.launch({ headless: true, executablePath: process.env.RC_PLAYWRIGHT_BROWSER });
console.log('browser launched');
const results = [];

async function pageFor(host = 'office.ts.net') {
  const context = await browser.newContext({ serviceWorkers: 'block', viewport: host === 'office.ts.net' ? {width:390,height:844} : {width:1280,height:900} });
  context.setDefaultTimeout(5000);
  context.setDefaultNavigationTimeout(5000);
  await context.route('**/*', async (route) => {
    const requestUrl = new URL(route.request().url());
    if (requestUrl.hostname === host) {
      await route.fetch({ url: `http://127.0.0.1:${port}${requestUrl.pathname}${requestUrl.search}` }).then((response) => route.fulfill({ response })).catch(() => route.fulfill({ status: 502, body: 'fixture route failed' }));
    } else await route.abort();
  });
  const page = await context.newPage();
  page.on('pageerror', (error) => console.error('pageerror', error.message));
  page.on('console', (message) => { if (message.type() === 'error') console.error('console', message.text()); });
  return { context, page };
}

async function verifyMobile(page) {
  await page.goto(`${origin}/#${invite.slice(invite.indexOf('#') + 1)}`);
  await page.getByTestId('invite-target').waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth), true);
  await fs.mkdir(path.join(root, 'runtime', 'invite-20260914'), { recursive: true });
  await page.screenshot({ path: path.join(root, 'runtime', 'invite-20260914', 'invitation-app-mobile.png'), fullPage: true });
  assert.equal(new URL(page.url()).hash, '');
  assert.equal(await page.getByTestId('host-address').inputValue(), origin);
  assert.equal(await page.getByTestId('pair-ticket').inputValue(), ticket);
  assert.equal(await page.evaluate(() => window.apiCalls?.length ?? 0), 0);
  assert.equal(await page.getByTestId('pair-device').isDisabled(), true);
  await page.getByRole('checkbox').check();
  await page.getByTestId('pair-device').click();
  await page.waitForFunction(() => window.apiCalls?.some((call) => call.op === 'pair'));
  await page.waitForFunction(() => localStorage.getItem('remotecodex-hosts-v1') !== null);
  const records = await page.evaluate(() => JSON.parse(localStorage.getItem('remotecodex-hosts-v1')));
  assert.deepEqual(records, [{ origin, label }]);
  assert.ok(!JSON.stringify(records).includes(ticket));
}

try {
  { const { context, page } = await pageFor(); try { await verifyMobile(page); results.push('mobile invitation'); } finally { await context.close(); } }
  { const { context, page } = await pageFor(); try {
      await page.goto(`${origin}/#${invite.slice(invite.indexOf('#') + 1)}`); await page.getByTestId('invite-target').waitFor();
      await page.getByTestId('host-address').fill('https://changed.ts.net');
      assert.equal(await page.getByTestId('pair-ticket').inputValue(), '');
      await page.getByTestId('invite-paste').fill(invite); await page.getByTestId('invite-import').click(); await page.getByTestId('invite-target').waitFor();
      await page.getByTestId('invite-paste').fill(`${origin}/#rc-invite=bad`); await page.getByTestId('invite-import').click();
      assert.equal(await page.getByTestId('pair-ticket').inputValue(), '');
      results.push('address edit and invalid invitation clear ticket');
    } finally { await context.close(); } }
  { const { context, page } = await pageFor('tauri.localhost'); try {
      await page.goto(`https://tauri.localhost/`); await page.getByRole('button', { name: /다른 PC/ }).click();
      await page.getByTestId('invite-paste').fill(invite); await page.getByTestId('invite-import').click(); await page.getByTestId('invite-target').waitFor();
      const localPair = page.getByRole('button', { name: /로컬 사용자/ });
      assert.equal(await localPair.isDisabled(), true);
      await page.getByRole('button', { name: /뒤로/ }).click(); await page.getByRole('button', { name: /이 PC/ }).click();
      await page.getByRole('checkbox').check(); await page.getByRole('button', { name: /Agent 시작 또는 연결/ }).click();
      await page.waitForFunction(() => window.tauriCalls?.some((call) => call.command === 'local_admin'));
      await page.waitForFunction(() => window.apiCalls?.some((call) => call.op === 'pair'));
      const calls = await page.evaluate(() => window.tauriCalls);
      const pairs = await page.evaluate(() => window.apiCalls.filter((call) => call.op === 'pair'));
      assert.ok(calls.some((call) => call.command === 'ensure_host'));
      assert.ok(calls.some((call) => call.command === 'local_admin'));
      assert.ok(pairs.length >= 1);
      assert.ok(pairs.every((call) => call.base === 'http://127.0.0.1:3847'));
      results.push('native local pairing guard and host start');
    } finally { await context.close(); } }
  { const { context, page } = await pageFor(); try {
      const foreign = {v:1,origin:'https://foreign.ts.net',label:'Foreign',ticket};
      await page.goto(origin + '/#rc-invite=' + Buffer.from(JSON.stringify(foreign)).toString('base64url'));
      await page.getByRole('status').waitFor();
      assert.equal(new URL(page.url()).hash, '');
      assert.equal(await page.getByTestId('invite-target').count(), 0);
      assert.equal(await page.getByTestId('pair-ticket').inputValue(), '');
      assert.equal(await page.evaluate(() => window.apiCalls?.length ?? 0), 0);
      results.push('foreign-origin fragment scrubbed and rejected without API');
    } finally { await context.close(); } }
  await fs.writeFile(path.join(root,'runtime/invite-20260914/app-ui.json'),JSON.stringify({status:'PASS',groups:results,mobile_viewport:{width:390,height:844}},null,2));
  console.log(JSON.stringify(results));
} finally { await browser.close(); await server.close(); }
