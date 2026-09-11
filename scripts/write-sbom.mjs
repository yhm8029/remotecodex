import fs from 'node:fs';
import { randomUUID } from 'node:crypto';

const argv = process.argv.slice(2);
if (argv.length !== 2) { process.exit(1); }
const [inputPath, outputPath] = argv;

let input;
try {
  input = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
} catch { process.exit(1); }

if (!input || typeof input !== 'object') process.exit(1);
if (typeof input.name !== 'string' || input.name.length === 0 || typeof input.version !== 'string' || input.version.length === 0) process.exit(1);
if (!Array.isArray(input.rust) || !Array.isArray(input.npm) || !Array.isArray(input.files)) process.exit(1);

const seen = new Map();
const components = [];

function addComponent(c) {
  if (seen.has(c['bom-ref'])) return;
  seen.set(c['bom-ref'], true);
  components.push(c);
}

for (const r of input.rust) {
  if (!r || typeof r.name !== 'string' || r.name.length === 0 || typeof r.version !== 'string' || r.version.length === 0) process.exit(1);
  const purl = `pkg:cargo/${encodeURIComponent(r.name)}@${encodeURIComponent(r.version)}`;
  const c = { type: 'library', name: r.name, version: r.version, 'bom-ref': purl, purl };
  if (typeof r.license === 'string' && r.license.length > 0) {
    c.licenses = [{ license: { name: r.license } }];
  }
  addComponent(c);
}

for (const n of input.npm) {
  if (!n || typeof n.name !== 'string' || n.name.length === 0 || typeof n.version !== 'string' || n.version.length === 0) process.exit(1);
  const parts = n.name.split('/');
  let purl;
  if (parts.length === 2 && parts[0].startsWith('@')) {
    purl = `pkg:npm/${encodeURIComponent(parts[0])}/${encodeURIComponent(parts[1])}@${encodeURIComponent(n.version)}`;
  } else {
    purl = `pkg:npm/${encodeURIComponent(n.name)}@${encodeURIComponent(n.version)}`;
  }
  const c = { type: 'library', name: n.name, version: n.version, 'bom-ref': purl, purl };
  if (typeof n.license === 'string' && n.license.length > 0) {
    c.licenses = [{ license: { name: n.license } }];
  }
  addComponent(c);
}

for (const f of input.files) {
  if (!f || !f.path || !f.sha256) process.exit(1);
  if (typeof f.path !== 'string' || f.path.length === 0) process.exit(1);
  if (f.path.includes('\\') || f.path.includes(':') || f.path.startsWith('/') || f.path.split('/').includes('..')) process.exit(1);
  if (!/^[A-Fa-f0-9]{64}$/.test(f.sha256)) process.exit(1);
  const ref = `file:${f.path}`;
  const c = {
    type: 'file',
    name: f.path,
    'bom-ref': ref,
    hashes: [{ alg: 'SHA-256', content: f.sha256 }]
  };
  addComponent(c);
}

const bom = {
  bomFormat: 'CycloneDX',
  specVersion: '1.6',
  version: 1,
  serialNumber: `urn:uuid:${randomUUID()}`,
  metadata: {
    timestamp: new Date().toISOString(),
    component: { type: 'application', name: input.name, version: input.version }
  },
  components,
  properties: [
    { name: 'remotecodex:inventory-scope', value: 'Resolved Rust/npm dependencies and bundled binaries; includes build and target-specific dependencies.' }
  ]
};

fs.writeFileSync(outputPath, JSON.stringify(bom, null, 2) + '\n', 'utf8');
