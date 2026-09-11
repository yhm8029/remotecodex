import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { summarizeSoakResources } from './soak-resources.mjs';

const execFileP = promisify(execFile);
const HOUR_SECONDS = 3600;

export async function startSoakSampler({ directory, agentManifest, sessions, durationSeconds }) {
  const sessionsPath = resolve(directory, 'resource-sessions.json');
  const manifestPath = resolve(directory, 'resources-manifest.json');
  const rawPath = resolve(directory, 'resources.ndjson');
  await writeFile(sessionsPath, JSON.stringify(sessions), {encoding:'utf8',flag:'wx'});

  let identities = [];
  try {
    const { stdout } = await execFileP(
      'powershell.exe',
      ['-NoProfile', '-File', resolve('tests/perf/process-manifest.ps1'),
       '-AgentManifest', resolve(agentManifest),
       '-Sessions', sessionsPath,
       '-Output', manifestPath],
      { windowsHide: true, timeout: 15000 }
    );
    void stdout;
    const manifestRaw = await readFile(manifestPath, 'utf8');
    identities = JSON.parse(manifestRaw);
    if (!Array.isArray(identities) || !identities.length) throw new Error('invalid manifest');
  } catch (err) { throw err; }

  const ctl = new AbortController();
  let completed = false;
  let failure = null;
  const outcome = execFileP(
    'powershell.exe',
    ['-NoProfile', '-File', resolve('scripts/measure-resources.ps1'),
     '-Manifest', manifestPath,
     '-Output', rawPath,
     '-DurationSeconds', String(durationSeconds),
     '-IntervalMs', '10000'],
    { windowsHide: true, signal: ctl.signal, timeout: (durationSeconds + 60) * 1000, maxBuffer: 65536 }
  ).then(
    () => { completed = true; },
    (err) => {
      completed = true;
      failure = err;
    }
  );

  let lastSavedHour = -1;
  let latestSummary = null;

  async function readRawSamples() {
    let text;
    try {
      text = await readFile(rawPath, 'utf8');
    } catch (err) {
      if (err && err.code === 'ENOENT') return null;
      throw err;
    }
    const lastNl = text.lastIndexOf('\n');
    const safe = lastNl >= 0 ? text.slice(0, lastNl) : '';
    const lines = safe.split('\n').filter((l) => l.length > 0);
    if (lines.length === 0) return null;
    const samples = [];
    for (const line of lines) {
      samples.push(JSON.parse(line));
    }
    if (samples.length === 0) return null;
    const meta = samples.shift();
    if (meta.type !== 'metadata') throw new Error('missing resource metadata');
    if (!samples.length) return null;
    return { meta, samples };
  }

  async function checkpoint() {
    if (failure) throw failure;
    const parsed = await readRawSamples();
    if (!parsed) return null;
    for(let i=1;i<parsed.samples.length;i++) if(parsed.samples[i].elapsed_seconds-parsed.samples[i-1].elapsed_seconds > 30) throw new Error('resource observation gap >30s');
    const summary = summarizeSoakResources(parsed.samples, identities, HOUR_SECONDS);
    latestSummary = summary;
    const observed = summary.observed_seconds || 0;
    const hour = Math.floor(observed / HOUR_SECONDS);
    if (hour > lastSavedHour) {
      const hourPath = resolve(directory, `resources-hour-${hour}.json`);
      const payload = { status: 'IN_PROGRESS', ...summary };
      await writeFile(hourPath, JSON.stringify(payload), {encoding:'utf8',flag:'wx'});
      lastSavedHour = hour;
    }
    return summary;
  }

  async function finish() {
    await outcome;
    if (failure) throw failure;
    const summary = await checkpoint();
    if (!summary || summary.observed_seconds < durationSeconds) throw new Error('incomplete resource duration');
    const finalPath = resolve(directory, 'resources-final.json');
    await writeFile(finalPath, JSON.stringify(summary), {encoding:'utf8',flag:'wx'});
    return summary;
  }

  async function abort() {
    ctl.abort();
    try { await outcome; } catch {}
  }

  return {
    checkpoint,
    finish,
    abort,
    _completed: () => completed,
    _latest: () => latestSummary,
  };
}
