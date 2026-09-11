import { createHash } from 'node:crypto';

const TARGET_OS = 'win32';
const TARGET_CPU = 'x64';

function allowed(list, value) {
  if (list === undefined || list.length === 0) return true;
  if (list.includes('!' + value)) return false;
  const positives = list.filter(entry => !entry.startsWith('!'));
  return positives.length === 0 || positives.includes(value);
}

function classifyLicenseCoverage(missing, rules, lockPackages, readBytes) {
  const rulesArr = Array.isArray(rules?.rules) ? rules.rules : [];
  const upstreamArr = Array.isArray(rules?.upstream_missing) ? rules.upstream_missing : [];

  const coverage = [];

  for (const m of missing) {
    const { ecosystem, name, version, installed } = m;

    const matchingRule = rulesArr.find(
      (r) => r.ecosystem === ecosystem && r.name === name && r.version === version
    );

    if (matchingRule) {
      if (!Array.isArray(matchingRule.files) || matchingRule.files.length === 0) {
        throw new Error(`Rule for ${name}@${version} has no files`);
      }
      for (const f of matchingRule.files) {
        for (const key of ['source', 'staged']) {
          const path = f[key];
          const buf = readBytes(path);
          if (!buf) throw new Error(`Missing file for ${name}: ${path}`);
          const got = createHash('sha256').update(buf).digest('hex');
          if (got !== f.sha256) throw new Error(`Hash mismatch for ${name}: ${path}`);
        }
      }
      coverage.push({
        ...m,
        status: matchingRule.status,
        files: matchingRule.files,
        provenance: matchingRule.provenance
      });
      continue;
    }

    if (!installed && ecosystem === 'npm') {
      let lockEntry = null;
      if (lockPackages && lockPackages[name] && lockPackages[name].version === version) {
        lockEntry = lockPackages[name];
      } else {
        for (const key of Object.keys(lockPackages || {})) {
          if (!key.startsWith('node_modules/')) continue;
          const seg = key.split('node_modules/').pop();
          if (seg === name) {
            const meta = lockPackages[key];
            if (meta && meta.version === version) {
              lockEntry = meta;
              break;
            }
          }
        }
      }
      if (
        lockEntry &&
        lockEntry.optional === true &&
        (!allowed(lockEntry.os, TARGET_OS) || !allowed(lockEntry.cpu, TARGET_CPU))
      ) {
        coverage.push({ ...m, status: 'not_shipped_optional_target' });
        continue;
      }
    }

    if (installed) {
      const upMatch = upstreamArr.find(
        (u) => u.name === name && u.version === version
      );
      if (upMatch) {
        coverage.push({
          ...m,
          status: 'missing_upstream_text',
          provenance: { readme: `${upMatch.repository}/blob/${upMatch.revision}/README.md`, revision: upMatch.revision }
        });
        continue;
      }
    }

    coverage.push({ ...m, status: 'unresolved' });
  }

  const unresolved = coverage.filter(
    (c) => c.status === 'missing_upstream_text' || c.status === 'unresolved'
  );
  const counts = {};
  for (const c of coverage) counts[c.status] = (counts[c.status] || 0) + 1;

  return { coverage, unresolved, counts };
}

export { classifyLicenseCoverage };