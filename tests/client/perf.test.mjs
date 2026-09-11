// tests/perf.test.ts
import assert from 'node:assert/strict';
import { PerfBuffer, emitPerf } from '../../.test-build/client/packages/terminal-client/src/perf.js';

// capacity validation
assert.throws(() => new PerfBuffer(0), /capacity/);
assert.throws(() => new PerfBuffer(1.5), /capacity/);
assert.throws(() => new PerfBuffer(0), /capacity/);
assert.throws(() => new PerfBuffer(1000001), /capacity/);
assert.doesNotThrow(() => new PerfBuffer(1));
assert.doesNotThrow(() => new PerfBuffer(1000000));

// capacity overflow
{
  const buf = new PerfBuffer(2);
  const ev = { stage: 'client_input_enqueue', session_id: 's1', elapsed_ms: 1 };
  buf.record(ev);
  buf.record({ ...ev, elapsed_ms: 2 });
  buf.record({ ...ev, elapsed_ms: 3 });
  assert.equal(buf.events.length, 2);
  assert.equal(buf.events[0].elapsed_ms, 1);
  assert.equal(buf.events[1].elapsed_ms, 2);
  assert.equal(buf.dropped, 1);
  buf.clear();
  assert.equal(buf.events.length, 0);
  assert.equal(buf.dropped, 0);
}

// emitPerf with undefined sink is no-op (does not throw)
assert.doesNotThrow(() => emitPerf(undefined, { stage: 'client_input_enqueue', session_id: 's', elapsed_ms: 1 }));

// emitPerf swallows callback exceptions
{
  const badSink = () => { throw new Error('boom'); };
  assert.doesNotThrow(() => emitPerf(badSink, { stage: 'client_input_enqueue', session_id: 's', elapsed_ms: 1 }));
}

// invalid elapsed values are not recorded by PerfBuffer
{
  const buf = new PerfBuffer(10);
  const base = { stage: 'client_input_enqueue', session_id: 's', elapsed_ms: 1 };
  buf.record({ ...base, elapsed_ms: -1 });
  buf.record({ ...base, elapsed_ms: Number.NaN });
  buf.record({ ...base, elapsed_ms: Number.POSITIVE_INFINITY });
  assert.equal(buf.events.length, 0);
  assert.equal(buf.dropped, 0);
}

// invalid events ignored by emitPerf
{
  let called = 0;
  const sink = () => { called++; };
  emitPerf(sink, { stage: 'client_input_enqueue', session_id: 's', elapsed_ms: -5 });
  emitPerf(sink, { stage: 'client_input_enqueue', session_id: 's', elapsed_ms: Number.NaN });
  emitPerf(sink, { stage: 'invalid', session_id: 's', elapsed_ms: 1 });
  assert.equal(called, 0);
}

// bytes and sequence are forwarded
{
  const buf = new PerfBuffer(5);
  buf.record({ stage: 'agent_input_write', session_id: 'abc', elapsed_ms: 4.2, bytes: 128, sequence: 'seq-1' });
  assert.equal(buf.events.length, 1);
  assert.equal(buf.events[0].bytes, 128);
  assert.equal(buf.events[0].sequence, 'seq-1');
}

console.log('perf tests ok');
