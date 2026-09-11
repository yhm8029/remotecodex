#!/usr/bin/env node

import { once } from 'node:events';
import { performance } from 'node:perf_hooks';

const MAX_TOKEN_BYTES = 80;
const PATTERN_WIDTH = 78;
const OUTPUT_CHUNK = 16 * 1024;

function usage() {
  console.error('usage: pty-workload.mjs echo | output <bytesPerSecond> <seconds>');
}

function integer(value, min, max) {
  if (!/^\d+$/.test(value ?? '')) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= min && parsed <= max ? parsed : null;
}

async function write(buffer) {
  if (buffer.length === 0) return;
  if (process.stdout.write(buffer)) return;
  await once(process.stdout, 'drain');
}

function installInterrupt(onInterrupt) {
  const handler = () => onInterrupt();
  process.on('SIGINT', handler);
  return () => process.off('SIGINT', handler);
}

async function runEcho() {
  if (!process.stdin.isTTY) {
    console.error('echo mode requires a TTY');
    process.exitCode = 2;
    return;
  }
  let stopped = false;
  let line = Buffer.alloc(0);
  let discarding = false;
  let releaseStop;
  const stoppedPromise = new Promise((resolve) => { releaseStop = resolve; });

  const stop = () => {
    if (stopped) return;
    stopped = true;
    if (process.stdin.isTTY) process.stdin.setRawMode(false);
    process.stdin.pause();
    releaseStop();
  };
  const removeInterrupt = installInterrupt(stop);
  try {
    process.stdin.setRawMode(true);
    await write(Buffer.from('RC_ECHO_READY\r\n', 'ascii'));
    const processChunk = async (chunk) => {
      if (stopped) return;
      for (const byte of chunk) {
        if (byte === 0x03) {
          stop();
          return;
        }
        if (byte === 0x0a) {
          if (!discarding) {
            const token = line[line.length - 1] === 0x0d ? line.subarray(0, line.length - 1) : line;
            const valid = token.length >= 1 && token.length <= MAX_TOKEN_BYTES && /^[A-Za-z0-9_-]+$/.test(token.toString('ascii'));
            if (valid) await write(Buffer.concat([Buffer.from('RC_ECHO:', 'ascii'), token, Buffer.from('\r\n', 'ascii')]));
          }
          line = Buffer.alloc(0);
          discarding = false;
        } else if (!discarding) {
          if (line.length >= MAX_TOKEN_BYTES + 1) {
            discarding = true;
            line = Buffer.alloc(0);
          } else {
            line = Buffer.concat([line, Buffer.from([byte])]);
          }
        }
      }
    }
    const onData = async (chunk) => {
      process.stdin.pause();
      try { await processChunk(chunk); } finally { if (!stopped) process.stdin.resume(); }
    };
    process.stdin.on('data', onData);
    process.stdin.resume();
    await Promise.race([once(process.stdin, 'end'), stoppedPromise]).catch(() => {});
  } finally {
    removeInterrupt();
    process.stdin.removeAllListeners('data');
    process.stdin.setRawMode(false);
  }
}

function patternBuffer() {
  const buffer = Buffer.alloc(OUTPUT_CHUNK + 80, 0x58);
  for (let offset = 0; offset < buffer.length; offset += 80) {
    buffer[offset + 78] = 0x0d;
    buffer[offset + 79] = 0x0a;
  }
  return buffer;
}

async function runOutput(rate, seconds) {
  let stopped = false;
  const stop = () => { stopped = true; };
  const removeInterrupt = installInterrupt(stop);
  const pattern = patternBuffer();
  const burst = rate * 0.1;
  let tokens = 0;
  let started;
  let deadline;
  let last;
  let written = 0;
  let patternOffset = 0;

  try {
    await write(Buffer.from('RC_OUTPUT_READY\r\n', 'ascii'));
  started = performance.now();
  deadline = started + seconds * 1000;
  last = started;

  while (!stopped && performance.now() < deadline) {
    const now = performance.now();
    tokens = Math.min(burst, tokens + (now - last) * rate / 1000);
    last = now;
    const remaining = Math.min(Number.MAX_SAFE_INTEGER, Math.ceil(rate * Math.max(0, (deadline - now) / 1000)));
    const available = Math.min(remaining, Math.floor(tokens));
    if (available < 1) {
      const waitMs = Math.max(1, Math.min(100, Math.ceil((1 - tokens) * 1000 / rate)));
      await new Promise((resolve) => setTimeout(resolve, waitMs));
      continue;
    }

    const count = Math.min(OUTPUT_CHUNK, available);
    const output = Buffer.allocUnsafe(count);
    patternOffset %= 80;
    pattern.copy(output, 0, patternOffset, patternOffset + count);
    patternOffset = (patternOffset + count) % 80;
    tokens -= count;
    await write(output);
    written += count;
  }

  // A false stdout.write has already been drained by write(); this keeps the
  // marker behind every byte accepted by the stream.
  await new Promise((resolve, reject) => process.stdout.write(Buffer.alloc(0), (error) => error ? reject(error) : resolve()));
  const elapsedSeconds = Math.max((performance.now() - started) / 1000, Number.EPSILON);
  await write(Buffer.from(`RC_OUTPUT_DONE:${JSON.stringify({ bytes: written, elapsed_seconds: elapsedSeconds, achieved_bytes_per_second: written / elapsedSeconds })}\r\n`, 'ascii'));
  } finally {
    removeInterrupt();
  }
}

const args = process.argv.slice(2);
if (args.length === 1 && args[0] === 'echo') {
  await runEcho();
} else if (args.length === 3 && args[0] === 'output') {
  const rate = integer(args[1], 1, 10 * 1024 * 1024);
  const seconds = integer(args[2], 1, 3600);
  if (rate === null || seconds === null) {
    usage();
    process.exitCode = 2;
  } else {
    await runOutput(rate, seconds);
  }
} else {
  usage();
  process.exitCode = 2;
}
