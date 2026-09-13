import test from 'node:test';
import assert from 'node:assert/strict';
import { loadHosts, saveHost, removeHost } from '../../apps/web/src/saved-hosts.ts';

const KEY = 'remotecodex-hosts-v1';

const makeStore = () => {
  const data = new Map();
  return {
    getItem: (k) => (data.has(k) ? data.get(k) : null),
    setItem: (k, v) => { data.set(k, v); },
    data
  };
};

test('saveHost strips sensitive fields and load returns metadata only', () => {
  const store = makeStore();
  const list = saveHost(store, { origin: 'https://office.ts.net', label: 'Office', ticket: 'SECRET' });

  const raw = store.getItem(KEY);
  assert.ok(typeof raw === 'string');
  const parsed = JSON.parse(raw);
  assert.deepEqual(parsed, [{ origin: 'https://office.ts.net', label: 'Office' }]);
  assert.equal(parsed[0].ticket, undefined);

  const loaded = loadHosts(store);
  assert.deepEqual(loaded, [{ origin: 'https://office.ts.net', label: 'Office' }]);

  const updated = saveHost(store, { origin: 'https://office.ts.net', label: 'Office HQ' });
  assert.equal(updated.length, 1);
  assert.equal(updated[0].label, 'Office HQ');

  const removed = removeHost(store, 'https://office.ts.net');
  assert.deepEqual(removed, []);
  assert.deepEqual(loadHosts(store), []);
});

test('loadHosts handles corrupted and invalid entries gracefully', () => {
  const store = makeStore();

  store.setItem(KEY, 'badjson');
  assert.deepEqual(loadHosts(store), []);

  const store2 = makeStore();
  store2.setItem(KEY, JSON.stringify([
    { origin: 'https://valid.ts.net', label: 'Valid' },
    'string-entry',
    null,
    { origin: 'http://insecure.ts.net', label: 'Insecure' },
    { origin: 'https://otherdomain.com', label: 'Other' },
    { origin: 'https://also-valid.ts.net', label: 'AlsoValid', ticket: 'X' }
  ]));
  const loaded = loadHosts(store2);
  assert.deepEqual(loaded, [
    { origin: 'https://valid.ts.net', label: 'Valid' },
    { origin: 'https://also-valid.ts.net', label: 'AlsoValid' }
  ]);
});

test('saving many hosts caps at 32 and keeps newest first', () => {
  const store = makeStore();
  assert.deepEqual(loadHosts(store), []);

  for (let i = 0; i < 35; i++) {
    const n = String(i).padStart(2, '0');
    saveHost(store, { origin: `https://h${n}.ts.net`, label: `Host ${n}` });
  }

  const list = loadHosts(store);
  assert.equal(list.length, 32);
  assert.equal(list[0].origin, 'https://h34.ts.net');
  assert.equal(list[31].origin, 'https://h03.ts.net');
});

test('storage write failure propagates from saveHost without losing auth', () => {
  const store = {
    getItem: () => null,
    setItem: () => { throw new Error('quota'); }
  };

  assert.throws(
    () => saveHost(store, { origin: 'https://office.ts.net', label: 'Office', ticket: 'SECRET' }),
    /quota/
  );

  // In-memory state should not have mutated, and a subsequent call with a working store still works
  const working = makeStore();
  const list = saveHost(working, { origin: 'https://office.ts.net', label: 'Office', ticket: 'SECRET' });
  assert.equal(list.length, 1);
  assert.deepEqual(loadHosts(working), [{ origin: 'https://office.ts.net', label: 'Office' }]);
});