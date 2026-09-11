import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

const origin = 'https://example.test';
const workerPath = resolve('apps/web/public/sw.js');
const offlinePath = resolve('apps/web/public/offline.html');

async function loadWorker({ cached = new Map(), network = async () => new Response('network') } = {}) {
  const handlers = new Map();
  const calls = { addAll: [], match: [], put: [], fetch: [] };
  const cache = {
    async addAll(paths) { calls.addAll.push(paths); },
    async match(path) { calls.match.push(path); return cached.get(path) ?? null; },
    async put(...args) { calls.put.push(args); }
  };
  const self = {
    location: { origin },
    addEventListener(name, handler) { handlers.set(name, handler); },
    skipWaiting: async () => {},
    clients: { claim: async () => {} }
  };
  const context = vm.createContext({
    self, URL, Request, Response,
    caches: { open: async () => cache, keys: async () => [] },
    fetch: async (request) => { calls.fetch.push(request); return network(request); }
  });
  vm.runInContext(await readFile(workerPath, 'utf8'), context, { filename: workerPath });
  return { fetch: handlers.get('fetch'), calls };
}

function event(url, { method = 'GET', mode = 'cors' } = {}) {
  let response;
  return {
    request: { url, method, mode },
    respondWith(value) { response = Promise.resolve(value); },
    response: () => response
  };
}

test('PWA static shell keeps live navigations and protected data outside its cache', async (t) => {
  await t.test('root navigation is network-first and never stored', async () => {
    const live = new Response('live root');
    const worker = await loadWorker({ cached: new Map([['/offline.html', new Response('stale offline')]]), network: async () => live });
    const root = event(`${origin}/`, { mode: 'navigate' });
    worker.fetch(root);
    assert.equal(await root.response(), live);
    assert.equal(worker.calls.fetch[0], root.request);
    assert.equal(worker.calls.put.length, 0);
  });

  await t.test('only offline root fallback is cached', async () => {
    const offline = new Response('offline page');
    const worker = await loadWorker({ cached: new Map([['/offline.html', offline]]), network: async () => { throw new Error('offline'); } });
    const root = event(`${origin}/index.html`, { mode: 'navigate' });
    worker.fetch(root);
    assert.equal(await root.response(), offline);
    assert.deepEqual(worker.calls.match, ['/offline.html']);
    assert.equal(worker.calls.put.length, 0);
  });

  await t.test('API, preview, foreign, non-root, and subresource requests are ignored', async () => {
    const { fetch } = await loadWorker();
    for (const input of [
      event(`${origin}/api/session`, { mode: 'navigate' }), event(`${origin}/_rc/auth`, { mode: 'navigate' }),
      event('https://cdn.example.test/app.js', { mode: 'navigate' }), event(`${origin}/workspace`, { mode: 'navigate' }),
      event(`${origin}/manifest.webmanifest`), event(`${origin}/icons/remotecodex.svg`), event(`${origin}/`, { method: 'POST', mode: 'navigate' })
    ]) {
      fetch(input);
      assert.equal(input.response(), undefined, input.request.url);
    }
  });

  await t.test('offline page is self-contained and instructs reconnection', async () => {
    const offline = await readFile(offlinePath, 'utf8');
    assert.doesNotMatch(offline, /<script|https?:\/\//i);
    assert.match(offline, /connection unavailable/i);
    assert.match(offline, /reconnect.*retry/i);
  });
});
