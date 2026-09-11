export type PreviewFramework = 'vite' | 'next';

export type PreviewAdapter = {
  framework: PreviewFramework;
  version: string;
  file: string;
  command: string;
  snippet: string;
  notes: string[];
};

const validPreviewOrigin = (value: string): URL => {
  const url = new URL(value);
  if (url.protocol !== 'https:' || !url.hostname.endsWith('.ts.net') || !url.port) {
    throw new Error('Preview adapter needs a registered HTTPS .ts.net origin');
  }
  const port = Number(url.port);
  if (!Number.isInteger(port) || port < 8444 || port > 8451) {
    throw new Error('Preview adapter port is outside the approved range');
  }
  return url;
};

export function previewAdapter(framework: PreviewFramework, origin: string, upstreamPort: number = 3000): PreviewAdapter {
  if (!Number.isInteger(upstreamPort) || upstreamPort < 1024 || upstreamPort > 65535) {
    throw new Error('Invalid upstream port');
  }
  const url = validPreviewOrigin(origin);
  const host = url.hostname;
  const authority = `${host}:${url.port}`;
  if (framework === 'vite') {
    return {
      framework,
      version: 'Vite 6.4.3',
      file: 'vite.config.ts',
      command: `vite --host 127.0.0.1 --port ${upstreamPort}`,
      snippet: `import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    host: '127.0.0.1',
    port: ${upstreamPort},
    strictPort: true,
    allowedHosts: ['${host}'],
    cors: { origin: '${url.origin}' },
    hmr: { protocol: 'wss', host: '${host}', clientPort: ${url.port} },
  },
});`,
      notes: [
        `Register this preview as ${authority} before starting the dev server.`,
        'The gateway translates the browser WSS connection to loopback; keep the upstream server HTTP-only.',
      ],
    };
  }
  return {
    framework,
    version: 'Next.js 16.3.3',
    file: 'next.config.mjs',
    command: `next dev --hostname 127.0.0.1 --port ${upstreamPort}`,
    snippet: `/** @type {import('next').NextConfig} */
const nextConfig = {
  allowedDevOrigins: ['${host}'],
};

export default nextConfig;`,
    notes: [
      `Register this preview as ${authority} before starting the dev server.`,
      'Next.js dev origin approval is explicit; route and Fast Refresh behavior still depends on the project version and app settings.',
    ],
  };
}
