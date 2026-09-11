import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {test} from 'node:test';
import {scalarWidth, unicodeProfile} from '../../.test-build/client/packages/terminal-client/src/unicode.js';

const cargoArgs = [
  'run', '--offline', '--quiet',
  '-p', 'rc-agent',
  '--example', 'unicode-width-profile',
  '--target-dir',
  process.env.TEMP
    ? process.env.TEMP + '/remotecodex-cargo-test-agent'
    : 'runtime/remotecodex-cargo-test-agent',
  '--', '--raw',
];

const raw = execFileSync('cargo', cargoArgs, {
  maxBuffer: 4 * 1024 * 1024,
  windowsHide: true,
  timeout: 120000,
});

test('terminal scalarWidth matches normalized Rust widths for all 0x110000 codepoints', () => {
  assert.equal(raw.length, 0x110000);
  for (let cp = 0; cp < raw.length; cp++) {
    const expected = raw[cp] === 255 ? 0 : raw[cp];
    const actual = scalarWidth(cp);
    if (actual !== expected) {
      assert.fail('width mismatch at U+' + cp.toString(16));
    }
  }
});

test('scalarWidth returns 0 for invalid inputs', () => {
  assert.equal(scalarWidth(-1), 0);
  assert.equal(scalarWidth(0x110000), 0);
  assert.equal(scalarWidth(NaN), 0);
  assert.equal(scalarWidth(Infinity), 0);
  assert.equal(scalarWidth(1.5), 0);
});

test('unicodeProfile.charProperties combines and resets cluster widths', () => {
  assert.equal(unicodeProfile.charProperties(0x301, 2), 3);
  assert.equal(unicodeProfile.charProperties(0x301, 4), 5);
  assert.equal(unicodeProfile.charProperties(0x301, 0), 0);
  assert.equal(unicodeProfile.charProperties(0x301, 1), 0);
  assert.equal(unicodeProfile.charProperties(0x41, 4), 2);
});