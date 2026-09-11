import assert from 'node:assert/strict';
import http from 'node:http';
import { execFileSync } from 'node:child_process';
import { randomUUID, webcrypto } from 'node:crypto';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { mkdir, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import WebSocket, { WebSocketServer } from 'ws';

if (process.platform !== 'win32') throw new Error('NOT_RUN: Windows Agent preview test');
const agent = resolve(process.argv[2] ?? 'target/debug/rc-agent.exe');
const management = 'http://127.0.0.1:3847', authority = 'fixture.ts.net:8444', gatewayPort = 13844;
const gatewayOrigin = 'https://fixture.ts.net:8444', report = { checks: [], cleanup: [], overall: 'FAIL' };
let token = '', deviceId, cleanupToken = '', cleanupDevice, sessionId, previewId, upstream, wsServer, socket, sse;
const madeDevices = [];
const pass = (name) => { report.checks.push({ name, status: 'PASS' }); console.log(`PASS ${name}`); };
async function api(path, body, method = body === undefined ? 'GET' : 'POST', auth = token) {
  const headers = { Origin: management, ...(body === undefined ? {} : { 'content-type': 'application/json' }) };
  if (auth) headers.authorization = `Bearer ${auth}`;
  const response = await fetch(`${management}/api/v1${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(12000) });
  if (!response.ok) { const error = new Error(`HTTP_${response.status}`); error.status = response.status; throw error; }
  return response.status === 204 ? null : response.json();
}
async function authenticate(label) {
  const raw = execFileSync(agent, ['pair'], { encoding: 'utf8', windowsHide: true, maxBuffer: 16384, timeout: 5000 });
  const ticket = JSON.parse(raw).ticket;
  const keys = await webcrypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
  const publicKey = Buffer.from(await webcrypto.subtle.exportKey('raw', keys.publicKey)).toString('base64url');
  const d = await api('/auth/pair', { ticket, public_key: publicKey, label }, 'POST', '');
  const challenge = await api('/auth/challenge', { client_id: d.client_id }, 'POST', '');
  const signature = Buffer.from(await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, keys.privateKey, new TextEncoder().encode(challenge.message))).toString('base64url');
  const verified = await api('/auth/verify', { challenge_id: challenge.challenge_id, signature }, 'POST', '');
  madeDevices.push(d.client_id); return { id: d.client_id, token: verified.access_token };
}
function request(path, options = {}) {
  return new Promise((resolveRequest, reject) => {
    const req = http.request({ hostname: '127.0.0.1', port: options.port ?? gatewayPort, path, method: options.method ?? 'GET', headers: options.headers ?? {}, timeout: 12000 }, (res) => {
      if (options.stream) return resolveRequest(res);
      const chunks = []; res.on('data', (chunk) => chunks.push(chunk)); res.on('end', () => resolveRequest({ res, body: Buffer.concat(chunks) }));
    });
    req.on('timeout', () => req.destroy(new Error('HTTP_TIMEOUT'))); req.on('error', reject);
    if (options.body) req.write(options.body); req.end();
  });
}
function gateway(path, options = {}) {
  return request(path, { ...options, headers: { Host: authority, Origin: gatewayOrigin, ...(options.headers ?? {}) } });
}
function cookieHeader(value) { return { cookie: value }; }
async function consume(ticket) {
  const body = JSON.stringify({ ticket });
  const result = await gateway('/_rc/consume', { method: 'POST', body, headers: { 'content-type': 'application/json' } });
  assert.equal(result.res.statusCode, 204); const raw = result.res.headers['set-cookie']?.[0] ?? '';
  assert.match(raw, /^__Host-rcpv-[^=]+=.+; Secure;/); assert.doesNotMatch(raw, /Domain=/i); return raw.split(';', 1)[0];
}
function timeout(ms) { return new Promise((_, reject) => setTimeout(() => reject(new Error('TIMEOUT')), ms)); }
async function cleanupAuth() { if (cleanupToken) return; const d = await authenticate(`PREVIEW-CLEANUP-${randomUUID()}`); cleanupDevice = d.id; cleanupToken = d.token; }
function startUpstream() {
  upstream = http.createServer((req, res) => {
    if (req.url === '/cookie') { res.writeHead(200, { 'set-cookie': 'sid=fixture; Path=/; Domain=localhost', 'content-security-policy': "default-src 'self'", 'x-frame-options': 'DENY' }); return res.end('cookie'); }
    if (req.url === '/echo') { const chunks = []; req.on('data', (x) => chunks.push(x)); return req.on('end', () => { res.setHeader('content-type', 'application/json'); res.end(JSON.stringify({ headers: req.headers, body: Buffer.concat(chunks).toString() })); }); }
    if (req.url === '/events') { res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' }); res.write('data: first\n\n'); const timer = setInterval(() => res.write('data: second\n\n'), 100); req.on('close', () => clearInterval(timer)); return; }
    res.writeHead(404); res.end();
  });
  wsServer = new WebSocketServer({ noServer: true, handleProtocols: (protocols) => protocols.has('vite-hmr') ? 'vite-hmr' : false });
  wsServer.on('connection', (client) => client.on('message', (message) => client.send(message)));
  upstream.on('upgrade', (req, raw, head) => req.url === '/hmr' ? wsServer.handleUpgrade(req, raw, head, (client) => wsServer.emit('connection', client, req)) : raw.destroy());
  return new Promise((resolveUpstream) => upstream.listen(0, '127.0.0.1', () => resolveUpstream(upstream.address().port)));
}
async function run() {
  const upstreamPort = await startUpstream();
  const first = await authenticate(`PREVIEW-FIXTURE-${randomUUID()}`); deviceId = first.id; token = first.token;
  const host = await api('/host'); assert.equal(host.public_origin, 'https://fixture.ts.net'); pass('isolated Agent public_origin is exact');
  const session = await api('/sessions', { label: `PREVIEW-PTY-${randomUUID()}`, project_id: null, cwd: tmpdir(), profile: 'cmd', cols: 80, rows: 24 }); sessionId = session.session_id;
  const registered = await api('/previews', { project_id: session.project_id, port: upstreamPort, public_port: 8444 }); previewId = registered.id; assert.equal(registered.gateway_port, gatewayPort); pass('owned CMD project and preview gateway registered');
  const unauth = await gateway('/echo'); assert.ok([401, 403].includes(unauth.res.statusCode)); pass('unauthenticated gateway request rejected');
  const launched = await api(`/previews/${previewId}/launch`, {}, 'POST'); const ticket = new URL(launched.url).hash.slice(1); assert.ok(ticket); const browserCookie = await consume(ticket); const replay = await gateway('/_rc/consume', { method: 'POST', body: JSON.stringify({ ticket }), headers: { 'content-type': 'application/json' } }); assert.ok([401, 403].includes(replay.res.statusCode)); pass('launch ticket is single-use');
  const cookieResponse = await gateway('/cookie', { headers: cookieHeader(browserCookie) }); const setCookie = cookieResponse.res.headers['set-cookie']?.[0] ?? ''; assert.match(setCookie, /rcapp_[^=]+=.*Secure/i); assert.doesNotMatch(setCookie, /Domain=/i); assert.match(cookieResponse.res.headers['content-security-policy'] ?? '', /default-src 'self'/); assert.equal(cookieResponse.res.headers['x-frame-options'], 'DENY'); const appCookie = setCookie.split(';', 1)[0]; pass('application cookies are namespaced and security headers preserved');
  const echoed = await gateway('/echo', { headers: { ...cookieHeader(`${browserCookie}; ${appCookie}; mgmt=strip`), authorization: 'Bearer DO-NOT-FORWARD', 'Tailscale-User-Login': 'fixture' } }); assert.equal(echoed.res.statusCode, 200); const echo = JSON.parse(echoed.body); assert.equal(echo.headers.authorization, undefined); assert.equal(echo.headers['tailscale-user-login'], undefined); assert.equal(echo.headers.cookie, 'sid=fixture'); pass('management credentials are stripped and app cookie is decoded');
  socket = new WebSocket(`ws://127.0.0.1:${gatewayPort}/hmr`, ['vite-hmr'], { headers: { Host: authority, Origin: gatewayOrigin, Cookie: browserCookie } }); await Promise.race([once(socket, 'open'), timeout(12000)]); socket.send('preview-ws-echo'); const [echoMessage] = await Promise.race([once(socket, 'message'), timeout(12000)]); assert.equal(echoMessage.toString(), 'preview-ws-echo'); pass('WebSocket echo and subprotocol passed through gateway');
  sse = await gateway('/events', { stream: true, headers: cookieHeader(browserCookie) }); let sseFirst = ''; const firstEvent = new Promise((resolveEvent, rejectEvent) => { const onData = (chunk) => { sseFirst += chunk.toString(); if (sseFirst.includes('data: first')) { sse.removeListener('data', onData); resolveEvent(); } }; sse.on('data', onData); sse.on('error', rejectEvent); }); await Promise.race([firstEvent, timeout(12000)]); pass('SSE first event streamed before connection end');
  const socketClosed = Promise.race([once(socket, 'close'), timeout(5000)]); const sseClosed = Promise.race([once(sse, 'close'), once(sse, 'end'), timeout(5000)]);
  await api(`/devices/${deviceId}`, undefined, 'DELETE'); await socketClosed; await sseClosed; pass('revoking paired device cancels active WebSocket and SSE'); const revoked = await gateway('/echo', { headers: cookieHeader(browserCookie) }); assert.ok([401, 403].includes(revoked.res.statusCode)); pass('revoked preview credential is rejected');
}
let failure;
try { await run(); report.overall = 'PASS'; } catch (error) { failure = error; report.checks.push({ name: 'preview gateway run', status: 'FAIL', error: error.message }); }
finally {
  try { if (sse) sse.destroy(); } catch {}
  try { if (socket) socket.close(); } catch {}
  try { await cleanupAuth(); } catch (error) { report.cleanup.push({ type: 'cleanup_device', status: 'FAILED', error: error.message }); failure ??= error; }
  if (cleanupToken) {
    token = cleanupToken;
    for (const id of [previewId]) if (id) { try { await api(`/previews/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'preview', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'preview', status: 'FAILED', error: error.message }); failure ??= error; } }
    if (sessionId) { try { await api(`/sessions/${sessionId}`, undefined, 'DELETE'); report.cleanup.push({ type: 'session', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'session', status: 'FAILED', error: error.message }); failure ??= error; } }
    for (const id of [...new Set(madeDevices)]) { try { await api(`/devices/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'device', status: 'revoked' }); } catch (error) { report.cleanup.push({ type: 'device', status: 'FAILED', error: error.message }); failure ??= error; } }
  }
  try { wsServer?.close(); upstream?.close(); } catch {}
  report.overall = failure ? 'FAIL' : report.overall; await mkdir('docs/test-results/local-preview', { recursive: true }); await writeFile('docs/test-results/local-preview/gateway.json', JSON.stringify(report, null, 2));
}
if (failure) { console.error(`PREVIEW E2E FAILED: ${failure.message}`); process.exitCode = 1; }
