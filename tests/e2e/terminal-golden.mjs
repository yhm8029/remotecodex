import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { test } from 'node:test';
import headless from '@xterm/headless';
import { configureUnicode } from '../../.test-build/client/packages/terminal-client/src/unicode.js';

const { Terminal } = headless;
const targetDir = process.env.TEMP
  ? `${process.env.TEMP}/remotecodex-cargo-test-agent`
  : 'runtime/remotecodex-cargo-test-agent';

function producer() {
  const stdout = execFileSync(
    'cargo',
    [
      'run',
      '--offline',
      '--quiet',
      '-p',
      'rc-agent',
      '--example',
      'snapshot-fixtures',
      '--target-dir',
      targetDir,
    ],
    { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 },
  ).trim();
  return JSON.parse(stdout);
}

function write(term, data) {
  return new Promise((resolve, reject) => {
    try { term.write(data, resolve); } catch (error) { reject(error); }
  });
}

function observeUnderline(term) {
  const seen = [];
  const disposable = term.parser.registerCsiHandler({ final: 'm' }, params => {
    const values = Array.from(params, value => Array.isArray(value) ? Array.from(value) : value);
    // Snapshot styles combine reset, colors, underline style, and underline
    // color into one SGR. Keep the whole parameter list so the observer also
    // verifies those combined forms, rather than only prefix SGRs.
    if (values.some(value => value === 4 || value === 38 || value === 58)) seen.push(values);
    return false;
  });
  return { seen, disposable };
}

function cellState(cell) {
  if (!cell) return null;
  const data = cell.getAsCharData();
  const blank = data[0] === 0 && data[1] === ' ' && data[2] === 1 && data[3] === 32;
  return {
    chars: blank ? '' : data[1],
    width: data[2],
    code: blank ? 0 : data[3],
    colors: {
      foregroundMode: cell.getFgColorMode(),
      foreground: cell.getFgColor(),
      backgroundMode: cell.getBgColorMode(),
      background: cell.getBgColor(),
      underlineMode: cell.getUnderlineColorMode(),
      underline: cell.getUnderlineColor(),
    },
    flags: {
      inverse: !!cell.isInverse(),
      bold: !!cell.isBold(),
      underline: !!cell.isUnderline(),
      blink: !!cell.isBlink(),
      invisible: !!cell.isInvisible(),
      italic: !!cell.isItalic(),
      dim: !!cell.isDim(),
      strike: !!cell.isStrikethrough(),
      overline: !!cell.isOverline(),
      protected: !!cell.isProtected(),
      underlineStyle: cell.getUnderlineStyle(),
    },
    extended: cell.hasExtendedAttrs(),
  };
}

function bufferState(buffer, width) {
  const lines = [];
  for (let row = 0; row < buffer.length; row += 1) {
    const line = buffer.getLine(row);
    lines.push({
      wrapped: !!line?.isWrapped,
      cells: Array.from({ length: width }, (_, col) => cellState(line?.getCell(col))),
    });
  }
  return {
    type: buffer.type,
    baseY: buffer.baseY,
    viewportY: buffer.viewportY,
    cursorX: buffer.cursorX,
    cursorY: buffer.cursorY,
    length: buffer.length,
    lines,
  };
}

function terminalState(term) {
  const active = term.buffer.active;
  const width = term.cols;
  const buffers = {};
  for (const name of ['normal', 'alternate', 'active']) {
    const buffer = term.buffer[name];
    if (!buffer) continue;
    const state = bufferState(buffer, width);
    buffers[name] = state;
  }
  return { buffers, activeType: active.type, modes: { ...term.modes } };
}

async function runState(fixture, input, withObserver) {
  const term = new Terminal({
    cols: fixture.cols,
    rows: fixture.rows,
    scrollback: 2000,
    allowProposedApi: true,
  });
  configureUnicode(term);
  const observer = withObserver ? observeUnderline(term) : null;
  for (const chunk of input) await write(term, chunk);
  observer?.disposable.dispose();
  const state = terminalState(term);
  term.dispose();
  return { state, underline: observer?.seen ?? [] };
}

const fixtures = producer();

for (const fixture of fixtures) {
      test(`production terminal snapshot golden: ${fixture.name}`, async () => {
    const live = await runState(
      fixture,
      [...fixture.prefix_chunks, fixture.continuation],
      fixture.name.includes('underline'),
    );
    const restored = await runState(
      fixture,
      [fixture.snapshot, fixture.continuation],
      fixture.name.includes('underline'),
    );
    assert.deepEqual(restored.state, live.state);
    if (fixture.name.includes('underline')) {
      const variants = records => new Set(records.flatMap(v => {
        const index = v.indexOf(4);
        if (index < 0) return [];
        const value = v[index + 1];
        return [Array.isArray(value) ? value[0] : value];
      }));
      for (const code of [1, 2, 3, 4, 5]) {
        assert.ok(variants(live.underline).has(code), `missing live underline CSI 4:${code}`);
        assert.ok(variants(restored.underline).has(code), `missing restored underline CSI 4:${code}`);
      }
      assert.ok(live.underline.some(v => v.includes(58)), 'missing underline color CSI');
      assert.ok(restored.underline.some(v => v.includes(58)), 'snapshot lost underline color CSI');
      assert.ok(live.underline.some(v => v.includes(58) && v.includes(2)), 'missing RGB underline color CSI');
      assert.ok(restored.underline.some(v => v.includes(58) && v.includes(2)), 'snapshot lost RGB underline color CSI');
    }
  });
}
