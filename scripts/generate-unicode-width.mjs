import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { readFileSync, writeFileSync } from 'node:fs';

const root = fileURLToPath(new URL('../', import.meta.url));
const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
const args = ['run', '--quiet', '--locked', '-p', 'rc-agent', '--example', 'unicode-width-profile'];

const stdout = execFileSync(cargo, args, {
  cwd: root,
  windowsHide: true,
  encoding: 'utf8',
  timeout: 120000,
  maxBuffer: 2 * 1024 * 1024,
});

const profile = JSON.parse(stdout);
const EXPECTED_UNICODE = [17, 0, 0];
const EXPECTED_CRATE = '0.2.2';
const EXPECTED_DEFAULT = 1;

if (!Array.isArray(profile.unicode_version) || profile.unicode_version.length !== 3
    || profile.unicode_version[0] !== EXPECTED_UNICODE[0]
    || profile.unicode_version[1] !== EXPECTED_UNICODE[1]
    || profile.unicode_version[2] !== EXPECTED_UNICODE[2]) {
  throw new Error(`Unexpected unicode_version: ${JSON.stringify(profile.unicode_version)}`);
}
if (profile.crate_version !== EXPECTED_CRATE) {
  throw new Error(`Unexpected crate_version: ${profile.crate_version}`);
}
if (profile.default_width !== EXPECTED_DEFAULT) {
  throw new Error(`Unexpected default_width: ${profile.default_width}`);
}

const ranges = profile.ranges;
if (!Array.isArray(ranges)) {
  throw new Error('ranges must be an array');
}
const seen = new Set();
for (const [idx, r] of ranges.entries()) {
  if (!Array.isArray(r) || r.length !== 3) {
    throw new Error(`range ${idx} must be [start,end,width]`);
  }
  const [start, end, width] = r;
  if (!Number.isInteger(start) || !Number.isInteger(end) || !Number.isInteger(width)) {
    throw new Error(`range ${idx} non-integer value`);
  }
  if (start < 0 || start > 0x10ffff || end < 0 || end > 0x10ffff) {
    throw new Error(`range ${idx} out of 0..0x10ffff`);
  }
  if (start > end) {
    throw new Error(`range ${idx} start > end`);
  }
  if (width !== 0 && width !== 2) {
    throw new Error(`range ${idx} width must be 0 or 2`);
  }
  const key = `${start}-${end}`;
  if (seen.has(key)) {
    throw new Error(`range ${idx} duplicate`);
  }
  seen.add(key);
}
for (let i = 1; i < ranges.length; i++) {
  if (ranges[i][0] <= ranges[i - 1][1]) {
    throw new Error(`ranges overlap or unsorted at index ${i}`);
  }
}
const sorted = ranges.slice().sort((a, b) => a[0] - b[0]);
for (let i = 0; i < ranges.length; i++) {
  if (ranges[i][0] !== sorted[i][0] || ranges[i][1] !== sorted[i][1] || ranges[i][2] !== sorted[i][2]) {
    throw new Error('ranges not sorted ascending');
  }
}

const header = '// Generated from unicode-width =0.2.2, Unicode 17.0.0, terminal widths capped at 2. MIT/Apache-2.0.\n';
const body = 'export const widthRanges: readonly (readonly [number, number, 0 | 2])[] = ' + JSON.stringify(ranges) + ';\n';
const content = header + body;

const outPath = resolve(root, 'packages/terminal-client/src/unicode-width.generated.ts');
const check = process.argv.includes('--check');

if (check) {
  let existing;
  try {
    existing = readFileSync(outPath, 'utf8');
  } catch {
    throw new Error(`--check failed: missing ${outPath}`);
  }
  if (existing !== content) {
    throw new Error(`--check failed: drift detected in ${outPath}`);
  }
  console.log('check');
  console.log(ranges.length);
} else {
  writeFileSync(outPath, content, 'utf8');
  console.log('write');
  console.log(ranges.length);
}
