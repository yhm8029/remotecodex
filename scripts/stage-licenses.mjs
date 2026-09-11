import { classifyLicenseCoverage } from './license-coverage.mjs';
import {
  copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync,
  writeFileSync,
} from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

if (process.argv.length !== 3) process.exit(1);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (!isAbsolute(process.argv[2])) throw new Error('SDK_ROOT must be absolute');
const sdk = resolve(process.argv[2]);
const stage = join(repo, 'runtime', 'package');
const stripBom = (s) => s.charCodeAt(0) === 0xfeff ? s.slice(1) : s;
const readJson = (p) => JSON.parse(stripBom(readFileSync(p, 'utf8')));
const safePart = /^[A-Za-z0-9_.+-]+$/;
const licenseName = /^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)/i;
const excluded = new Set(['target', 'node_modules', '.git']);

function within(base, candidate) {
  const r = relative(resolve(base), resolve(candidate));
  return r === '' || (!r.startsWith(`..${sep}`) && r !== '..' && !isAbsolute(r));
}
function symlink(p) {
  try { return lstatSync(p).isSymbolicLink(); } catch { return false; }
}
function ensure(p) { mkdirSync(p, { recursive: true }); }
function file(p) { return existsSync(p) && !symlink(p) && lstatSync(p).isFile(); }
function licenseFiles(root) {
  const result = [];
  function visit(dir, depth) {
    if (symlink(dir) || depth > 1) return;
    let entries;
    try { entries = readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const entry of entries) {
      if (excluded.has(entry.name)) continue;
      const p = join(dir, entry.name);
      if (symlink(p)) continue;
      if (entry.isFile() && licenseName.test(entry.name)) result.push(p);
      else if (entry.isDirectory() && depth < 1) visit(p, depth + 1);
    }
  }
  visit(root, 0);
  return result;
}
function copyOne(source, destination) {
  if (!file(source)) return false;
  ensure(dirname(destination));
  copyFileSync(source, destination);
  return true;
}
function packageRoot(manifest) {
  if (typeof manifest !== 'string' || !isAbsolute(manifest)) return null;
  return dirname(resolve(manifest));
}
function addMissing(missing, item) { missing.push(item); }

const rootPackage = readJson(join(repo, 'package.json'));
const cargo = readJson(join(repo, 'runtime', 'cargo-metadata.json'));
const desktop = readJson(join(repo, 'runtime', 'desktop-metadata.json'));
const lock = readJson(join(repo, 'package-lock.json'));
const supplementalPath = join(repo, 'docs', 'third-party', 'license-sources.json');
const supplemental = existsSync(supplementalPath) ? readJson(supplementalPath) : [];
if (!Array.isArray(supplemental)) throw new Error('supplemental license metadata is malformed');
const supplementalFor = (name, version) => supplemental.filter((item) => item?.group === `${name}-${version}`);
if (!Array.isArray(cargo.packages) || !Array.isArray(desktop.packages) || !lock.packages) {
  throw new Error('required dependency metadata is malformed');
}
const sdkLicenses = join(sdk, 'share', 'licenses');
if (!existsSync(sdk) || symlink(sdk) || !lstatSync(sdk).isDirectory()) throw new Error('SDK_ROOT is invalid');
if (!existsSync(sdkLicenses) || symlink(sdkLicenses) || !lstatSync(sdkLicenses).isDirectory()) {
  throw new Error('SDK share/licenses is invalid');
}

const rust = new Map();
const missing = [];
for (const pkg of [...cargo.packages, ...desktop.packages]) {
  if (typeof pkg.name !== 'string' || typeof pkg.version !== 'string' ||
      !safePart.test(pkg.name) || !safePart.test(pkg.version)) {
    throw new Error('invalid Rust package metadata');
  }
  const key = `${pkg.name}@${pkg.version}`;
  if (rust.has(key)) continue;
  const root = packageRoot(pkg.manifest_path);
  const record = { name: pkg.name, version: pkg.version, license: pkg.license ?? null };
  rust.set(key, record);
  const destination = join(stage, 'licenses', 'rust', `${pkg.name}-${pkg.version}`);
  const copied = new Set();
  if (root && file(join(root, 'Cargo.toml'))) {
    for (const source of licenseFiles(root)) {
      const rel = relative(root, source);
      if (copied.has(rel)) continue;
      copied.add(rel);
      copyOne(source, join(destination, rel));
    }
    if (typeof pkg.license_file === 'string') {
      const explicit = isAbsolute(pkg.license_file)
        ? resolve(pkg.license_file) : resolve(root, pkg.license_file);
      if (within(root, explicit) && file(explicit)) {
        const rel = relative(root, explicit);
        if (!copied.has(rel)) { copied.add(rel); copyOne(explicit, join(destination, rel)); }
      }
    }
  }
  if (copied.size === 0) {
    addMissing(missing, {
      name: pkg.name, version: pkg.version, ecosystem: 'rust',
      installed: !!root && file(join(root, 'Cargo.toml')), license: record.license,
      supplemental: supplementalFor(pkg.name, pkg.version),
    });
  }
}

const npm = new Map();
for (const [key, info] of Object.entries(lock.packages)) {
  if (!key.includes('node_modules/') || info?.link) continue;
  const suffix = key.split('node_modules/').pop();
  const name = typeof info?.name === 'string' ? info.name : suffix;
  const version = info?.version;
  if (!name || !version) throw new Error('invalid npm package metadata');
  const safeName = name.replace(/^@/, '').replaceAll('/', '__');
  if (!safePart.test(safeName) || !safePart.test(version)) throw new Error('invalid npm package id');
  const packageDir = join(repo, key);
  const dedup = `${name}@${version}`;
  if (npm.has(dedup)) continue;
  const record = { name, version, license: info?.license ?? null };
  npm.set(dedup, record);
  let copied = 0;
  if (file(join(packageDir, 'package.json'))) {
    for (const source of licenseFiles(packageDir)) {
      const rel = relative(packageDir, source);
      if (copyOne(source, join(stage, 'licenses', 'npm', `${safeName}-${version}`, rel))) copied++;
    }
  }
  if (copied === 0) {
    addMissing(missing, {
      name, version, ecosystem: 'npm', installed: file(join(packageDir, 'package.json')),
      license: record.license, supplemental: supplementalFor(name, version),
    });
  }
}

function copyTree(source, destination) {
  if (symlink(source)) throw new Error('license tree contains a symlink');
  ensure(destination);
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    const from = join(source, entry.name);
    const to = join(destination, entry.name);
    if (symlink(from)) throw new Error('license tree contains a symlink');
    if (entry.isDirectory()) copyTree(from, to);
    else if (entry.isFile()) copyOne(from, to);
  }
}
copyTree(sdkLicenses, join(stage, 'licenses', 'gstreamer'));
const upstream = join(repo, 'docs', 'third-party', 'licenses');
if (!existsSync(upstream) || symlink(upstream) || !lstatSync(upstream).isDirectory()) {
  throw new Error('vendored third-party licenses are invalid');
}
copyTree(upstream, join(stage, 'licenses', 'upstream'));
if (!file(join(repo, 'LICENSE'))) throw new Error('root LICENSE is missing');
copyOne(join(repo, 'LICENSE'), join(stage, 'licenses', 'RemoteCodex-LICENSE'));
ensure(join(stage, 'licenses'));
const rulesPath = join(repo, 'docs', 'third-party', 'license-coverage.json');
const rules = readJson(rulesPath);
const coverageReport = classifyLicenseCoverage(missing, rules, lock.packages, path => {
  if (typeof path !== 'string') throw new TypeError('path must be string');
  const base = path.startsWith('licenses/') ? stage : repo;
  const full = resolve(base, path);
  if (!within(base, full) || !file(full)) throw new Error('invalid license path: ' + path);
  return readFileSync(full);
});
writeFileSync(join(stage, 'licenses', 'inventory.json'), JSON.stringify({ missing, ...coverageReport }, null, 2) + '\n');

const files = [];
function hashTree(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (symlink(p)) continue;
    const rel = relative(stage, p).split(sep).join('/');
    if (entry.isDirectory()) {
      if (rel !== 'licenses' && !rel.startsWith('licenses/')) hashTree(p);
    } else if (entry.isFile() && rel !== 'SBOM.json' && !rel.startsWith('licenses/')) {
      files.push({ path: rel, sha256: createHash('sha256').update(readFileSync(p)).digest('hex') });
    }
  }
}
if (existsSync(stage)) hashTree(stage);
files.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
writeFileSync(join(repo, 'runtime', 'package-inventory.json'), JSON.stringify({
  name: 'remotecodex', version: rootPackage.version,
  rust: [...rust.values()], npm: [...npm.values()], supplemental_sources: supplemental, files,
}, null, 2) + '\n');
console.log(`rust=${rust.size} npm=${npm.size} files=${files.length} archive_text_missing=${missing.length} unresolved_text=${coverageReport.unresolved.length}`);
