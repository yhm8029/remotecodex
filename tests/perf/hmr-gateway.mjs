import http from 'node:http';
import WebSocket from 'ws';

const GATEWAY_HOST = '127.0.0.1';
const GATEWAY_PORT = 13844;

function httpRequest({ method, path, headers, body }) {
  return new Promise((resolve, reject) => {
    let req;
    const timer = setTimeout(() => { req?.destroy(new Error('http timeout')); reject(new Error('http timeout')); }, 10000);
    req = http.request({
      host: GATEWAY_HOST,
      port: GATEWAY_PORT,
      method,
      path,
      headers: {
        Host: 'fixture.ts.net:8444',
        Origin: 'https://fixture.ts.net:8444',
        ...headers,
      },
    }, (res) => {
      const chunks = [];
      let total = 0;
      const limit = 1024 * 1024;
      res.on('data', (chunk) => {
        total += chunk.length;
        if (total > limit) {
          clearTimeout(timer);
          res.destroy();
          req.destroy();
          reject(new Error('http response exceeded 1MiB'));
          return;
        }
        chunks.push(chunk);
      });
      res.on('end', () => {
        clearTimeout(timer);
        resolve({ statusCode: res.statusCode, headers: res.headers, body: Buffer.concat(chunks).toString('utf8') });
      });
      res.on('error', (err) => {
        clearTimeout(timer);
        reject(err);
      });
    });
    req.on('error', error => { clearTimeout(timer); reject(error); });
    if (body) req.write(body);
    req.end();
  });
}

export async function openHmrGateway(host, previewId) {
  const launched = await host.api(`/previews/${previewId}/launch`, {}, 'POST');
  const ticket = new URL(launched.url).hash.slice(1);

  const consume = await httpRequest({
    method: 'POST',
    path: '/_rc/consume',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ticket }),
  });
  if (consume.statusCode !== 204) {
    throw new Error(`consume failed: ${consume.statusCode}`);
  }
  const setCookie = consume.headers['set-cookie'];
  if (!setCookie || !setCookie[0]) throw new Error('no set-cookie');
  const cookie = setCookie[0].split(';', 1)[0];

  const vite = await httpRequest({
    method: 'GET',
    path: '/@vite/client',
    headers: { Cookie: cookie },
  });
  if (vite.statusCode !== 200) throw new Error(`vite client ${vite.statusCode}`);
  const match = vite.body.match(/const wsToken = ['"]([^'"]+)['"]/);
  if (!match) throw new Error('wsToken not found');
  const wsToken = match[1];
  const entry = await httpRequest({method:'GET', path:'/src/main.js', headers:{Cookie:cookie}});
  if (entry.statusCode !== 200) throw new Error('Vite entry load failed');

  let socket;
  try {
    socket = new WebSocket(
      `ws://${GATEWAY_HOST}:${GATEWAY_PORT}/hmr?token=${encodeURIComponent(wsToken)}`,
      'vite-hmr',
      { headers: { Host: 'fixture.ts.net:8444', Origin: 'https://fixture.ts.net:8444', Cookie: cookie } }
    );
  } catch (err) {
    throw err;
  }

  return await waitHmrConnected(socket);
}

async function waitHmrConnected(socket) {
  let connected = false;
  let closed = false;
  let errorValue = null;
  let updateCount = 0;
  let resolveWait = null;
  const start = performance.now();

  socket.on('message', (data) => {
    try {
      const msg = JSON.parse(data.toString());
      if (msg && msg.type === 'connected') {
        connected = true;
        if (resolveWait) { resolveWait(); resolveWait = null; }
      } else if (msg && msg.type === 'update') {
        updateCount++;
      }
    } catch {}
  });
  socket.on('error', (err) => {
    errorValue = err;
    if (resolveWait) { resolveWait(); resolveWait = null; }
  });
  socket.on('close', () => {
    closed = true;
    if (resolveWait) { resolveWait(); resolveWait = null; }
  });

  try {
    while (!connected) {
      if (errorValue) throw errorValue;
      if (closed) throw new Error('HMR socket closed');
      if (performance.now() - start > 10000) throw new Error('HMR connection timeout');
      await new Promise((resolve) => { resolveWait = resolve; setTimeout(resolve, 20); });
    }
  } catch (err) {
    try { socket.close(); } catch {}
    throw err;
  }

  return {
    socket,
    get updates() { return updateCount; },
    get error() { return errorValue ?? (closed ? new Error('HMR socket closed') : null); },
  };
}
