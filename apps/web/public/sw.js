'use strict';

const CACHE_NAME = 'remotecodex-shell-v2';
const CACHE_PREFIX = 'remotecodex-shell-';
const PRECACHE_URLS = Object.freeze([
  '/offline.html',
  '/manifest.webmanifest',
  '/icons/remotecodex.svg'
]);

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE_NAME);
    await cache.addAll(PRECACHE_URLS);
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const cacheNames = await caches.keys();
    await Promise.all(cacheNames.map((name) => (
      name.startsWith(CACHE_PREFIX) && name !== CACHE_NAME ? caches.delete(name) : undefined
    )));
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname === '/api' || url.pathname.startsWith('/api/')) return;
  if (url.pathname === '/_rc' || url.pathname.startsWith('/_rc/')) return;
  if (request.mode !== 'navigate') return;
  if (url.pathname !== '/' && url.pathname !== '/index.html') return;

  event.respondWith((async () => {
    try {
      return await fetch(request);
    } catch {
      const cache = await caches.open(CACHE_NAME);
      const offline = await cache.match('/offline.html');
      if (offline) return offline;
      return new Response('RemoteCodex is offline. Reconnect and try again.', {
        status: 503,
        statusText: 'Service Unavailable',
        headers: { 'Content-Type': 'text/plain; charset=utf-8' }
      });
    }
  })());
});
