import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeSoakResources as summarize } from './soak-resources.mjs';

const MiB = 1048576;
const HOUR = 3600;

function make(mibValues) {
  return mibValues.map((mib, idx) => ({
    elapsed_seconds: idx * HOUR,
    processes: [{
      pid: 1,
      started_ticks: '123',
      group: 'product',
      status: 'ok',
      private_bytes: mib * MiB,
      handles: 10 + idx,
      threads: 4
    }]
  }));
}

const baseIdentities = [{ Id: 1, StartTicks: '123', Group: 'product' }];

test('detects memory leak across hourly samples', () => {
  const samples = make([1, 11, 21, 31]);
  const result = summarize(samples, baseIdentities, HOUR);
  assert.equal(result.private_bytes_slope_per_hour, 10485760);
  assert.equal(result.leak_flag, true);
  assert.equal(result.handles_monotonic_growth, true);
});

test('no leak when memory stable, threads do not grow', () => {
  const samples = make([1, 11, 10, 9]);
  const samplesWithThreads = samples.map((s, i) => ({
    ...s,
    processes: s.processes.map(p => ({ ...p, threads: 4 + (i === 1 ? 1 : 0) }))
  }));
  const result = summarize(samplesWithThreads, baseIdentities, HOUR);
  assert.equal(result.leak_flag, false);
  assert.equal(result.threads_monotonic_growth, false);
});

test('returns null leak when insufficient post-cutoff samples', () => {
  const samples = make([1, 1]);
  const result = summarize(samples, baseIdentities, HOUR);
  assert.equal(result.leak_flag, null);
});

test('throws when started_ticks changes for same pid', () => {
  const samples = make([1, 11, 21, 31]);
  samples[1].processes[0].started_ticks = '456';
  assert.throws(() => summarize(samples, baseIdentities, HOUR));
});

test('throws on duplicate pid row within first sample', () => {
  const samples = make([1, 11, 21, 31]);
  samples[0].processes.push({ ...samples[0].processes[0] });
  assert.throws(() => summarize(samples, baseIdentities, HOUR));
});

test('throws when pid missing from first sample', () => {
  const samples = make([1, 11, 21, 31]);
  samples[0].processes = [];
  assert.throws(() => summarize(samples, baseIdentities, HOUR));
});

test('throws on non-monotonic elapsed seconds', () => {
  const samples = make([1, 11, 21, 31]);
  samples[2].elapsed_seconds = samples[1].elapsed_seconds - 10;
  assert.throws(() => summarize(samples, baseIdentities, HOUR));
});
