import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { randomUUID, webcrypto } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, readFile, rm, realpath, lstat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, dirname, basename } from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import WebSocket from 'ws';

if (process.platform !== 'win32') throw new Error('NOT_RUN: Windows Agent preview test');
const agent = resolve(process.argv[2] ?? 'runtime/test-agent/rc-agent.exe');
const management = 'http://127.0.0.1:3847';
const authority = 'fixture.ts.net:8444';
const gatewayPort = 13844;
const gatewayOrigin = 'https://fixture.ts.net:8444';
let token = '', sessionId, previewId, devServer, fixture, cleanupToken, nextError = '';
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
  deviceIds.push(paired.client_id);
  const challenge = await api('/auth/challenge', { client_id: paired.client_id }, 'POST', '');
  const signature = Buffer.from(await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, keys.privateKey, new TextEncoder().encode(challenge.message))).toString('base64url');
  const verified = await api('/auth/verify', { challenge_id: challenge.challenge_id, signature }, 'POST', '');
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
async function waitLocalNext() {
  for (let i = 0; i < 80; i++) {
    try { const res = await fetch('http://127.0.0.1:4322/', { signal: AbortSignal.timeout(1500) }); if (res.ok) return; } catch {}
    await wait(250);
  }
  throw new Error(`NEXT_START_TIMEOUT:${nextError.slice(-500)}`);
}
async function prepareNextFixture(){
  fixture=await mkdtemp(join(tmpdir(),'rc-next-gateway-'))
  await mkdir(join(fixture,'app'),{recursive:true})
  const pkg={private:true,type:'module',dependencies:{next:'16.3.3',react:'19.2.0','react-dom':'19.2.0'}}
  await writeFile(join(fixture,'package.json'),JSON.stringify(pkg))
  await writeFile(join(fixture,'app','layout.js'),`export default function Layout({children}){return (<html><body>{children}</body></html>)}`)
  await writeFile(join(fixture,'app','page.js'),`"use client";import{useState}from"react";export default function Page(){const[c,setC]=useState(0);return(<div><h1>NEXT_MARKER_V1</h1><button onClick={()=>setC(c+1)}>count {c}</button></div>)}`)
  await writeFile(join(fixture,'next.config.mjs'),`export default {allowedDevOrigins:['fixture.ts.net'],devIndicators:false}`)
  execFileSync(process.execPath,[join(dirname(process.execPath),'node_modules/npm/bin/npm-cli.js'),'install','--offline','--ignore-scripts','--no-audit','--no-fund'],{cwd:fixture,windowsHide:true,stdio:'ignore',timeout:120000})
  const proc=spawn(process.execPath,[join(fixture,'node_modules/next/dist/bin/next'),'dev','--hostname','127.0.0.1','--port','4322'],{cwd:fixture,windowsHide:true,stdio:['ignore','ignore','pipe']})
  proc.stderr.on('data',chunk=>{nextError=(nextError+chunk.toString()).slice(-4000)})
  proc.on('error',error=>{nextError=error.message})
  devServer=proc
}
async function verifyNextHmr(cookie, runtimeId) {
  const ws = new WebSocket(`ws://127.0.0.1:${gatewayPort}/_next/hmr?id=${encodeURIComponent(runtimeId)}`, { headers: { Host: authority, Origin: gatewayOrigin, Cookie: cookie } });
  const messages = [];
  let error = null;
  ws.on('message', (data) => {
    try { messages.push(JSON.parse(data.toString())); } catch {}
  });
  ws.on('error', (err) => { error = err; });
  try {
    let connected = false;
    for (let i = 0; i < 120; i++) {
      if (error) throw error;
      if (messages.some(m => m.type === 'turbopack-connected')) { connected = true; break; }
      await wait(100);
    }
    assert(connected);
    messages.length = 0;
    const pagePath = join(fixture, 'app', 'page.js');
    const original = await readFile(pagePath, 'utf8');
    const updated = original.replace(/NEXT_MARKER_V1/g, 'NEXT_MARKER_V2');
    await writeFile(pagePath, updated);
    let observed = false;
    for (let i = 0; i < 300; i++) {
      if (error) throw error;
      if (messages.some(m => m.type === 'built' || m.type === 'serverComponentChanges')) { observed = true; break; }
      await wait(100);
    }
    assert(observed);
  } finally {
    ws.terminate();
  }
}
async function verifyNextPage(browserCookie) {

  const path = '/';
  const headers = { cookie: browserCookie };

  const initial = await gateway(path, { headers });
  assert.strictEqual(initial.res.statusCode, 200, 'status code should be 200');
  const initialBody = initial.body.toString();
  assert.ok(initialBody.includes('NEXT_MARKER_V1'), 'initial body should contain NEXT_MARKER_V1');

  const match = initialBody.match(/__next_r="([^"]+)"/);
  assert.ok(match && match[1], 'runtime id should be present in HTML');

  pass('real Next HTML served through gateway');

  await verifyNextHmr(browserCookie, match[1]);

  const updated = await gateway(path, { headers });
  assert.strictEqual(updated.res.statusCode, 200, 'status code should be 200 after HMR');
  const updatedBody = updated.body.toString();
  assert.ok(updatedBody.includes('NEXT_MARKER_V2'), 'updated body should contain NEXT_MARKER_V2');

  pass('real Next HMR and changed HTML through gateway');
}
async function cleanupNextProcessAndFiles() {
  if (devServer?.pid && devServer.exitCode === null) {
    try {
      execFileSync(
        'taskkill.exe',
        ['/PID', String(devServer.pid), '/T', '/F'],
        { windowsHide: true, stdio: 'ignore', timeout: 10000 }
      );
    } catch (err) {
      if (devServer.exitCode == null) {
        throw err;
      }
    }
  }

  if (fixture) {
    const stats = await lstat(fixture);
    if (stats.isSymbolicLink()) {
      throw new Error('Fixture is a symbolic link');
    }

    const resolvedFixture = await realpath(fixture);
    const resolvedTemp = await realpath(tmpdir());

    if (dirname(resolvedFixture).toLowerCase() !== resolvedTemp.toLowerCase()) {
      throw new Error('Fixture is not inside the system temp directory');
    }

    if (!basename(resolvedFixture).startsWith('rc-next-gateway-')) {
      throw new Error('Fixture name does not match expected prefix');
    }

    await rm(resolvedFixture, {
      recursive: true,
      force: true,
      maxRetries: 4,
      retryDelay: 500,
    });
  }
}

async function run() {
  await prepareNextFixture();
  await waitLocalNext();
  const first = await authenticate(`NEXT-FIXTURE-${randomUUID()}`); token = first.token;
  // The Next process runs only inside its private temp fixture directory.
  const session = await api('/sessions', { label: `NEXT-PTY-${randomUUID()}`, project_id: null, cwd: tmpdir(), profile: 'cmd', cols: 80, rows: 24 }); sessionId = session.session_id;
  const registered = await api('/previews', { project_id: session.project_id, port: 4322, public_port: 8444 }); previewId = registered.id;
  assert.equal(registered.gateway_port, gatewayPort); pass('registered actual Next fixture on approved preview port');
  const launched = await api(`/previews/${previewId}/launch`, {}, 'POST');
  const consume = await gateway('/_rc/consume', { method: 'POST', body: JSON.stringify({ ticket: new URL(launched.url).hash.slice(1) }), headers: { 'content-type': 'application/json' } });
  assert.equal(consume.res.statusCode, 204); const browserCookie = consume.res.headers['set-cookie'][0].split(';', 1)[0];
  await verifyNextPage(browserCookie);
}
let failure;
try { await run(); report.overall = 'PASS'; } catch (error) { failure = error; report.checks.push({ name: 'real Next adapter fixture', status: 'FAIL', error: error.message }); }
finally {
  if (token) {
    try { const cleanup = await authenticate(`NEXT-CLEANUP-${randomUUID()}`); cleanupToken = cleanup.token; token = cleanupToken; } catch (error) { report.cleanup.push({ type: 'cleanup_auth', status: 'FAILED', error: error.message }); failure ??= error; }
    for (const id of [previewId]) if (id) { try { await api(`/previews/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'preview', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'preview', status: 'FAILED', error: error.message }); failure ??= error; } }
    if (sessionId) { try { await api(`/sessions/${sessionId}`, undefined, 'DELETE'); report.cleanup.push({ type: 'session', status: 'removed' }); } catch (error) { report.cleanup.push({ type: 'session', status: 'FAILED', error: error.message }); failure ??= error; } }
    for (const id of [...new Set(deviceIds)]) { try { await api(`/devices/${id}`, undefined, 'DELETE'); report.cleanup.push({ type: 'device', status: 'revoked' }); } catch (error) { report.cleanup.push({ type: 'device', status: 'FAILED', error: error.message }); failure ??= error; } }
  }
  try { await cleanupNextProcessAndFiles(); report.cleanup.push({type:'fixture_tree',status:'removed'}); } catch(error) { report.cleanup.push({type:'fixture_tree',status:'FAILED',error:error.message}); failure ??= error; }
  report.overall = failure ? 'FAIL' : report.overall;
  await writeFile('docs/test-results/local-preview/next-adapter.json', JSON.stringify(report, null, 2));
}
if (failure) { console.error(`NEXT PREVIEW E2E FAILED: ${failure.message}`); process.exitCode = 1; }
