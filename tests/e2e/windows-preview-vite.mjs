import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { randomUUID, webcrypto } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import WebSocket from 'ws';

if (process.platform !== 'win32') throw new Error('NOT_RUN: Windows Agent preview test');
const agent = resolve(process.argv[2] ?? 'runtime/test-agent/rc-agent.exe');
const management = 'http://127.0.0.1:3847';
const authority = 'fixture.ts.net:8444';
const gatewayPort = 13844;
const gatewayOrigin = 'https://fixture.ts.net:8444';
const viteBin = resolve('node_modules/vite/bin/vite.js');
const viteModule = resolve('node_modules/vite/dist/node/index.js').replaceAll('\\', '/');
let token = '', sessionId, previewId, vite, fixture, cleanupToken, viteError = '';
const deviceIds = [];
const report = { checks: [], cleanup: [], overall: 'FAIL' };
const pass = (name) => { report.checks.push({ name, status: 'PASS' }); console.log(`PASS ${name}`); };
async function api(path, body, method = body === undefined ? 'GET' : 'POST', auth = token) {
  const headers = { Origin: management };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (auth) headers.authorization = `Bearer ${auth}`;
  const res = await fetch(`${management}/api/v1${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(12000) });
  if (!res.ok) throw new Error(`API_${res.status}_${path}`);
  return res.status === 204 ? null : res.json();
}
async function authenticate(label) {
  const raw = execFileSync(agent, ['pair'], { encoding: 'utf8', windowsHide: true, maxBuffer: 16384, timeout: 5000 });
  const { ticket } = JSON.parse(raw);
  const keys = await webcrypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
  const publicKey = Buffer.from(await webcrypto.subtle.exportKey('raw', keys.publicKey)).toString('base64url');
  const paired = await api('/auth/pair', { ticket, public_key: publicKey, label }, 'POST', '');
  const challenge = await api('/auth/challenge', { client_id: paired.client_id }, 'POST', '');
  const signature = Buffer.from(await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, keys.privateKey, new TextEncoder().encode(challenge.message))).toString('base64url');
  const verified = await api('/auth/verify', { challenge_id: challenge.challenge_id, signature }, 'POST', '');
  deviceIds.push(paired.client_id);
  return { id: paired.client_id, token: verified.access_token };
}
function gateway(path, options = {}) {
  return new Promise((resolveRequest, reject) => {
    const req = http.request({ hostname: '127.0.0.1', port: gatewayPort, path, method: options.method ?? 'GET', headers: { Host: authority, Origin: gatewayOrigin, ...(options.headers ?? {}) }, timeout: 12000 }, (res) => {
      if (options.stream) return resolveRequest(res);
      const chunks = []; res.on('data', (chunk) => chunks.push(chunk)); res.on('end', () => resolveRequest({ res, body: Buffer.concat(chunks) }));
    });
    req.on('timeout', () => req.destroy(new Error('GATEWAY_TIMEOUT'))); req.on('error', reject);
    if (options.body) req.write(options.body); req.end();
  });
}
function wait(ms) { return new Promise((resolveWait) => setTimeout(resolveWait, ms)); }
async function waitLocalVite() {
  for (let i = 0; i < 80; i++) {
    try { const res = await fetch('http://127.0.0.1:4321/', { signal: AbortSignal.timeout(500) }); if (res.ok) return; } catch {}
    await wait(250);
  }
  throw new Error(`VITE_START_TIMEOUT:${viteError.slice(-500)}`);
}
async function run() {
  fixture = await mkdtemp(join(tmpdir(), 'rc-vite-fixture-'));
  await writeFile(join(fixture, 'index.html'), '<!doctype html><html><body><div id="app"></div><script type="module" src="/src/main.js"></script></body></html>');
  await mkdir(join(fixture, 'src'));
  await writeFile(join(fixture, 'src/main.js'), "document.querySelector('#app').textContent = 'vite-hmr-v1'; import.meta.hot?.accept();\n");
  await writeFile(join(fixture, 'vite.config.mjs'), `import { defineConfig } from '${viteModule}';\nexport default defineConfig({ server: { host: '127.0.0.1', port: 4321, strictPort: true, allowedHosts: ['fixture.ts.net'], cors: { origin: 'https://fixture.ts.net:8444' }, hmr: { protocol: 'wss', host: 'fixture.ts.net', clientPort: 8444, path: '/hmr' } } });\n`);
  vite = spawn(process.execPath, [viteBin, '--config', join(fixture, 'vite.config.mjs')], { cwd: fixture, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
  vite.stderr.on('data', (chunk) => { viteError += chunk.toString(); });
  await waitLocalVite();
  const first = await authenticate(`VITE-FIXTURE-${randomUUID()}`); token = first.token;
  // Reuse the isolated fixture project's approved origin slot; the Vite process itself remains in its private temp cwd.
  const session = await api('/sessions', { label: `VITE-PTY-${randomUUID()}`, project_id: null, cwd: tmpdir(), profile: 'cmd', cols: 80, rows: 24 }); sessionId = session.session_id;
  const registered = await api('/previews', { project_id: session.project_id, port: 4321, public_port: 8444 }); previewId = registered.id;
  assert.equal(registered.gateway_port, gatewayPort); pass('registered actual Vite fixture on approved preview port');
  const launched = await api(`/previews/${previewId}/launch`, {}, 'POST');
  const consume = await gateway('/_rc/consume', { method: 'POST', body: JSON.stringify({ ticket: new URL(launched.url).hash.slice(1) }), headers: { 'content-type': 'application/json' } });
  assert.equal(consume.res.statusCode, 204); const browserCookie = consume.res.headers['set-cookie'][0].split(';', 1)[0];
  const page = await gateway('/', { headers: { cookie: browserCookie } });
  assert.equal(page.res.statusCode, 200); const html = page.body.toString(); assert.match(html, /@vite\/client/); pass('gateway served real Vite HTML and client adapter');
  const client = await gateway('/@vite/client', { headers: { cookie: browserCookie } });
  assert.equal(client.res.statusCode, 200);
  const tokenMatch = client.body.toString().match(/const wsToken = ['\"]([^'\"]+)['\"]/);
  assert.ok(tokenMatch, 'Vite client websocket token missing');
  const wsPath = `/hmr?token=${encodeURIComponent(tokenMatch[1])}`;
  const direct = new WebSocket(`ws://127.0.0.1:4321${wsPath}`, 'vite-hmr', { headers: { Host: 'fixture.ts.net:8444', Origin: gatewayOrigin } });
  await Promise.race([once(direct, 'open'), new Promise((_, reject) => setTimeout(() => reject(new Error('VITE_DIRECT_WS_TIMEOUT')), 5000))]);
  direct.close();
  const hmr = new WebSocket(`ws://127.0.0.1:${gatewayPort}${wsPath}`, 'vite-hmr', { headers: { Host: authority, Origin: gatewayOrigin, Cookie: browserCookie } });
  await Promise.race([once(hmr, 'open'), new Promise((_, reject) => setTimeout(() => reject(new Error('VITE_WS_OPEN_TIMEOUT')), 12000))]);
  const messages = [];
  hmr.on('message', (message) => { try { messages.push(JSON.parse(message.toString())); } catch {} });
  for (let i = 0; i < 40 && !messages.some((message) => message.type === 'connected'); i++) await wait(100);
  assert.ok(messages.some((message) => message.type === 'connected'));
  await writeFile(join(fixture, 'src/main.js'), "document.querySelector('#app').textContent = 'vite-hmr-v2'; import.meta.hot?.accept();\n");
  for (let i = 0; i < 80 && !messages.some((message) => message.type === 'update'); i++) await wait(100);
  assert.ok(messages.some((message) => message.type === 'update'), `no Vite update through gateway (${viteError.slice(-200)})`); pass('real Vite HMR update crossed gateway WebSocket');
  hmr.close();
}
let failure;
try { await run(); report.overall = 'PASS'; } catch (error) { failure = error; report.checks.push({ name: 'real Vite adapter fixture', status: 'FAIL', error: error.message }); }
finally {
  try { vite?.kill(); } catch {}
  if (token) {
    try { const cleanup = await authenticate(`VITE-CLEANUP-${randomUUID()}`); cleanupToken = cleanup.token; token = cleanupToken; } catch (error) { report.cleanup.push({ type: 'cleanup_auth', status: 'FAILED', error: error.message }); failure ??= error; }
    for (const id of [previewId]) if (id) { try { await api(`/previews/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'preview', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'preview', status: 'FAILED', error: error.message }); failure ??= error; } }
    if (sessionId) { try { await api(`/sessions/${sessionId}`, undefined, 'DELETE'); report.cleanup.push({ type: 'session', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'session', status: 'FAILED', error: error.message }); failure ??= error; } }
    for (const id of [...new Set(deviceIds)]) { try { await api(`/devices/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'device', status: 'revoked' }); } catch (error) { report.cleanup.push({ type: 'device', status: 'FAILED', error: error.message }); failure ??= error; } }
  }
  if (fixture) await rm(fixture, { recursive: true, force: true }).catch(() => {});
  report.overall = failure ? 'FAIL' : report.overall;
  await writeFile('docs/test-results/local-preview/vite-adapter.json', JSON.stringify(report, null, 2));
}
if (failure) { console.error(`VITE PREVIEW E2E FAILED: ${failure.message}`); process.exitCode = 1; }
