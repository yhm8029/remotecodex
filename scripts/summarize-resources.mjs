import { readFileSync, writeFileSync } from 'node:fs';
import { argv } from 'node:process';
import { stats } from '../tests/perf/stats.mjs';

const inPath = argv[2];
const outPath = argv[3];
const raw = readFileSync(inPath, 'utf8');
const lines = raw.split(/\r?\n/).filter(l => l.length > 0);

let meta;
try {
  meta = JSON.parse(lines[0]);
} catch (e) {
  throw new Error('Malformed metadata JSON');
}
if (!meta || typeof meta !== 'object') throw new Error('Missing metadata');
if (typeof meta.duration_seconds !== 'number' || !Array.isArray(meta.identities)) {
  throw new Error('Missing metadata fields');
}

const samples = [];
for (let i = 1; i < lines.length; i++) {
  let s;
  try { s = JSON.parse(lines[i]); } catch (e) { throw new Error('Malformed sample JSON at line ' + (i+1)); }
  if (!s || !Array.isArray(s.processes)) throw new Error('Malformed sample at line ' + (i+1));
  samples.push(s);
}

if (!samples.length || meta.type !== 'metadata') throw new Error('Missing metadata or samples');
for (let i=0;i<samples.length;i++) { if (!Number.isFinite(samples[i].elapsed_seconds) || samples[i].elapsed_seconds < 0 || (i && samples[i].elapsed_seconds <= samples[i-1].elapsed_seconds)) throw new Error('Invalid monotonic sample time'); }
function summarizeGroup(samples, expected) {
  const ws = [], pb = [], threads = [], handles = [], cpu = [];
  let invalid = 0, weighted = 0, weight = 0, previousElapsed = null;
  for (const sample of samples) {
    const delta = previousElapsed == null ? 0 : sample.elapsed_seconds - previousElapsed;
    previousElapsed = sample.elapsed_seconds;
    let sampleInvalid = false;
    const sums = { ws: 0, pb: 0, th: 0, hd: 0, cpu: 0 };
    let allCpuOk = true;
    for (const exp of expected) {
      const rows = sample.processes.filter(r => r.pid === exp.Id && r.group === exp.Group);
      if (rows.length !== 1) { sampleInvalid = true; break; }
      const row = rows[0];
      if (row.status !== 'ok') { sampleInvalid = true; break; }
      if (typeof exp.StartTicks === 'string' && row.started_ticks !== exp.StartTicks) { sampleInvalid = true; break; }
      const mem = [row.private_working_set_bytes, row.private_bytes, row.threads, row.handles];
      if (!mem.every(v => Number.isFinite(v) && v >= 0)) { sampleInvalid = true; break; }
      sums.ws += row.private_working_set_bytes;
      sums.pb += row.private_bytes;
      sums.th += row.threads;
      sums.hd += row.handles;
      if (Number.isFinite(row.cpu_machine_pct) && row.cpu_machine_pct >= 0) {
        sums.cpu += row.cpu_machine_pct;
      } else {
        allCpuOk = false;
      }
    }
    if (sampleInvalid) { invalid++; continue; }
    ws.push(sums.ws); pb.push(sums.pb); threads.push(sums.th); handles.push(sums.hd);
    if (allCpuOk) {
      cpu.push(sums.cpu);
      if (delta > 0) { weighted += sums.cpu * delta; weight += delta; }
    }
  }
  const cpuStats = stats(cpu);
  return {
    processes: expected.length,
    valid_samples: ws.length,
    invalid_samples: invalid,
    cpu_machine_pct: { ...cpuStats, weighted_mean: weight ? weighted / weight : null },
    private_working_set_bytes: stats(ws),
    private_bytes: stats(pb),
    threads: stats(threads),
    handles: stats(handles)
  };
}
const groups = {};
for (const group of new Set([...meta.identities.map(i=>i.Group), 'tracked_total'])) {
  groups[group]=summarizeGroup(samples,group==='tracked_total'?meta.identities:meta.identities.filter(i=>i.Group===group));
}
const output={source:inPath,definition:{cpu:'whole machine percent',memory:'sum of private working set; private bytes separately',cpu_average:'weighted by monotonic sample intervals',scope:'Only explicitly tracked process identities; absent groups are not zero'},requested_seconds:meta.duration_seconds,last_elapsed_seconds:samples.at(-1).elapsed_seconds,sample_count:samples.length,identity_metadata_legacy:meta.identities.some(i=>typeof i.StartTicks!=='string'),groups};
writeFileSync(outPath,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
