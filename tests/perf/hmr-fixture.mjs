import { createServer } from 'vite';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { openHmrGateway } from './hmr-gateway.mjs';

export async function withHmrFixture(host, session, directory, body) {
  let fixture = resolve(directory, 'vite');
  let src = resolve(fixture, 'src');
  let server;
  let id;
  let hmr;
  let stopped = false;
  let pumpError;
  let pumpPromise;
  let iterations = 0;
  const errors = [];

  try {
    await mkdir(fixture, { recursive: false });
    await mkdir(src, { recursive: false });

    await writeFile(
      resolve(fixture, 'index.html'),
      `<!doctype html><html><body><script type="module" src="/src/main.js"></script></body></html>`
    );

    await writeFile(
      resolve(src, 'main.js'),
      `document.body.dataset.revision = '0';
import.meta.hot?.accept();
`
    );

    server = await createServer({
      configFile: false,
      root: fixture,
      cacheDir: resolve(fixture, 'cache'),
      server: {
        host: '127.0.0.1',
        port: 4321,
        strictPort: true,
        allowedHosts: ['fixture.ts.net'],
        cors: { origin: 'https://fixture.ts.net:8444' },
        hmr: {
          protocol: 'wss',
          host: 'fixture.ts.net',
          clientPort: 8444,
          path: '/hmr'
        }
      }
    });

    await server.listen();

    const registered = await host.api('/previews', {
      project_id: session.project_id,
      port: 4321,
      public_port: 8444
    });
    id = registered.id;

    hmr = await openHmrGateway(host, id);

    pumpPromise = (async () => {
      try {
        while (!stopped) {
          const before = hmr.updates;
          iterations++;
          const next = String(iterations);
          await writeFile(
            resolve(src, 'main.js'),
            `document.body.dataset.revision = '${next}';
import.meta.hot?.accept();
`
          );
          const deadline = Date.now() + 5000;
          while (hmr.updates <= before) {
            if (hmr.error) throw hmr.error;
            if (Date.now() > deadline) {
              throw new Error('HMR update timeout for revision ' + next);
            }
            await new Promise((r) => setTimeout(r, 20));
          }
          await new Promise((r) => setTimeout(r, 500));
        }
      } catch (err) {
        pumpError = err;
      }
    })();
    pumpPromise.catch(() => {});

    await waitFirstHmr(hmr, () => pumpError);

    const result = await body({ get updates() { return hmr.updates; } });
    if (pumpError) throw pumpError;

    return { result, hmr_updates: hmr.updates };
  } catch (err) {
    errors.push(err);
    throw err;
  } finally {
    stopped = true;
    try {
      if (pumpPromise) await pumpPromise;
      if (pumpError && !errors.includes(pumpError)) errors.push(pumpError);
    } catch (e) {
      errors.push(e);
    }
    if (hmr && hmr.socket) {
      try { hmr.socket.close(); } catch (e) { errors.push(e); }
    }
    if (id) {
      try { await host.api('/previews/' + id, undefined, 'DELETE'); } catch (e) { errors.push(e); }
    }
    if (server) {
      try { await server.close(); } catch (e) { errors.push(e); }
    }
    if (errors.length) throw new AggregateError(errors, 'withHmrFixture cleanup errors');
  }
}
async function waitFirstHmr(hmr, getPumpError) {
    const start = performance.now();
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

    while (hmr.updates <= 0) {
        if (performance.now() - start >= 10000) {
            const pumpError = getPumpError();
            throw pumpError !== undefined ? pumpError : new Error("Timed out waiting for first HMR update");
        }

        const pumpError = getPumpError();
        if (pumpError !== undefined) throw pumpError;
        if (hmr.error) throw hmr.error;

        await sleep(20);
    }

    const pumpError = getPumpError();
    if (pumpError !== undefined) throw pumpError;
    if (hmr.error) throw hmr.error;
}
