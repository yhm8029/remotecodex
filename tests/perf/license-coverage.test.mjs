import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { classifyLicenseCoverage } from '../../scripts/license-coverage.mjs';

const digest = (value) => createHash('sha256').update(value).digest('hex');
const missing = (installed) => [{ ecosystem: 'npm', name: 'demo', version: '1.2.3', installed }];

function rule(sourceHash, stagedHash) {
  return {
    ecosystem: 'npm', name: 'demo', version: '1.2.3', status: 'covered',
    provenance: { readme: 'upstream/README', revision: 'abc123' },
    files: [{ source: '/source/LICENSE', staged: '/stage/LICENSE', sha256: sourceHash }],
  };
}

test('matching rule verifies source and staged bytes and preserves metadata', () => {
  const source = Buffer.from('same-license-bytes');
  const staged = Buffer.from('same-license-bytes');
  const coverageRule = rule(digest(source), digest(staged));
  const result = classifyLicenseCoverage(missing(true), { rules: [coverageRule], upstream_missing: [] }, {}, (path) =>
    path === '/source/LICENSE' ? source : staged,
  );
  assert.equal(result.coverage[0].status, 'covered');
  assert.deepEqual(result.coverage[0].provenance, coverageRule.provenance);
});

test('source and staged hash mismatches are each rejected eagerly', () => {
  const source = Buffer.from('same-license-bytes');
  const staged = Buffer.from('same-license-bytes');
  const valid = rule(digest(source), digest(staged));
  assert.throws(
    () => classifyLicenseCoverage(missing(true), { rules: [valid], upstream_missing: [] }, {}, () => Buffer.from('wrong-source')),
    /Hash mismatch/,
  );
  assert.throws(
    () => classifyLicenseCoverage(missing(true), { rules: [valid], upstream_missing: [] }, {}, (path) =>
      path === '/source/LICENSE' ? source : Buffer.from('wrong-staged'),
    ),
    /Hash mismatch/,
  );
});

test('rule matching requires exact ecosystem, name, and version', () => {
  const bytes = Buffer.from('license');
  const coverageRule = rule(digest(bytes), digest(bytes));
  const result = classifyLicenseCoverage(
    [{ ecosystem: 'npm', name: 'demo', version: '1.2.4', installed: true }],
    { rules: [coverageRule], upstream_missing: [] }, {}, () => bytes,
  );
  assert.equal(result.coverage[0].status, 'unresolved');
});

test('optional absent npm package is excluded only when target constraints reject win32/x64', () => {
  const packageInfo = { version: '1.2.3', optional: true, os: ['win32', '!linux'], cpu: ['x64', '!arm64'] };
  const excluded = classifyLicenseCoverage(missing(false), { rules: [], upstream_missing: [] }, { 'node_modules/demo': packageInfo }, () => null);
  assert.equal(excluded.coverage[0].status, 'unresolved');

  const targetExcluded = { ...packageInfo, os: ['linux', '!win32'] };
  const result = classifyLicenseCoverage(missing(false), { rules: [], upstream_missing: [] }, { 'node_modules/demo': targetExcluded }, () => null);
  assert.equal(result.coverage[0].status, 'not_shipped_optional_target');
});

test('installed and absent nonoptional packages remain unresolved', () => {
  const lockPackages = { 'node_modules/demo': { version: '1.2.3', optional: false } };
  const result = classifyLicenseCoverage([...missing(true), ...missing(false)], { rules: [], upstream_missing: [] }, lockPackages, () => null);
  assert.equal(result.coverage.length, 2);
  assert.deepEqual(result.coverage.map((item) => item.status), ['unresolved', 'unresolved']);
});

test('upstream_missing is reported as unresolved coverage', () => {
  const result = classifyLicenseCoverage(
    missing(true),
    { rules: [], upstream_missing: [{ name: 'demo', version: '1.2.3', repository: 'upstream/demo', revision: 'abc123' }] },
    {}, () => null,
  );
  assert.equal(result.coverage[0].status, 'missing_upstream_text');
  assert.equal(result.unresolved.length, 1);
});
