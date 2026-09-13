import test from 'node:test';
import assert from 'node:assert/strict';
import {
  createInvitation,
  parseInvitation,
  normalizeHostOrigin,
} from '../../apps/web/src/invitations.ts';

const input = {
  origin: 'https://office.tail123.ts.net',
  label: 'Office',
  ticket: 'a'.repeat(43),
};

const encoded = (v) =>
  input.origin + '/#rc-invite=' + Buffer.from(JSON.stringify(v)).toString('base64url');

test('normalizeHostOrigin rules', () => {
  assert.equal(normalizeHostOrigin('https://office.tail123.ts.net:443/'), 'https://office.tail123.ts.net');
  for (const bad of [
    'http://host.ts.net',
    'https://host.evil',
    'https://user:pass@host.ts.net',
    'https://host.ts.net:8443',
    'https://host.ts.net/path',
    'https://host.ts.net?q=1',
    'https://host.ts.net#frag',
    'http://localhost',
  ]) {
    assert.throws(() => normalizeHostOrigin(bad));
  }
  for (const bad of [
    'https://office.ts.net/x/..',
    'https:///office.ts.net',
    'https://office.ts.net?',
    'https://office.ts.net#',
    'https://user:@office.ts.net',
    `https://${'a'.repeat(250)}.ts.net`,
  ]) {
    assert.throws(() => normalizeHostOrigin(bad));
  }
});

test('rejects malformed envelope', () => {
  const malformed = input.origin + '/#rc-invite=@@@not_base64!!!';
  assert.throws(() => parseInvitation(malformed));
  const huge = input.origin + '/#rc-invite=' + 'a'.repeat(2049);
  assert.throws(() => parseInvitation(huge));
});
test('korean label round-trips', () => {
  const kInput = { origin: 'https://office.tail123.ts.net', label: '\ud68c\uc0ac PC', ticket: 'a'.repeat(43) };
  assert.deepEqual(parseInvitation(createInvitation(kInput)), { v: 1, ...kInput });
});

test('rejects invalid payloads and enforces origin match', () => {
  const input = { origin: 'https://office.tail123.ts.net', label: 'Office', ticket: 'a'.repeat(43) };
  const invalid = [
    { v: 2, ...input },
    { v: 1, ...input, ticket: 'short' },
    { v: 1, ...input, extra: true },
    { v: 1, ...input, origin: 'https://other.ts.net' },
  ];
  for (const payload of invalid) {
    assert.throws(() => parseInvitation(encoded(payload)));
  }
  const valid = encoded({ v: 1, ...input });
  assert.deepEqual(parseInvitation(valid), { v: 1, ...input });
  assert.throws(() => parseInvitation(valid, 'https://other.ts.net'));
});

test('requires canonical raw invitation envelope and valid UTF-8', () => {
  const token = Buffer.from(JSON.stringify({ v: 1, ...input })).toString('base64url');
  assert.throws(() => parseInvitation('https://office.tail123.ts.net:443/#rc-invite=' + token));
  assert.throws(() => parseInvitation('https://office.tail123.ts.net/x/../#rc-invite=' + token));
  const malformedUtf8 = Buffer.from([0xc3, 0x28]).toString('base64url');
  assert.throws(() => parseInvitation(input.origin + '/#rc-invite=' + malformedUtf8));
});

test('compact v2 invitations round-trip and are shorter than legacy v1', () => {
  const value = { origin: input.origin, label: '\ud68c\ud68c Remote', ticket: input.ticket };
  const compact = createInvitation(value);
  const legacy = encoded({ v: 1, ...value });
  assert.ok(compact.length < legacy.length);
  assert.deepEqual(parseInvitation(compact), { v: 1, ...value });
});

test('compact v2 rejects malformed fields and foreign origins', () => {
  const label = Buffer.from('Office').toString('base64url');
  const base = `${input.origin}/#rc-invite=2.${input.ticket}.${label}`;
  for (const bad of [
    base + '.extra',
    `${input.origin}/#rc-invite=2.${input.ticket}.`,
    `${input.origin}/#rc-invite=2.${input.ticket}.${Buffer.from([0xc3, 0x28]).toString('base64url')}`,
    `${input.origin}/#rc-invite=2.${input.ticket}.${Buffer.from('   ').toString('base64url')}`,
  ]) assert.throws(() => parseInvitation(bad));
  assert.throws(() => parseInvitation(base, 'https://other.ts.net'));
});
