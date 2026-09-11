import { AgentApi } from '../../packages/terminal-client/src/api.js';
import { Controller } from '../../packages/terminal-client/src/controller.js';
import { TerminalView } from '../../packages/terminal-client/src/terminal.js';
import { PerfBuffer, type PerfEvent } from '../../packages/terminal-client/src/perf.js';
import type { SessionInfo } from '@remotecodex/core';

export const api = new AgentApi(location.origin);
export const perf = new PerfBuffer(1000000);
export const backgroundOutput = new Map<string, { events: number; bytes: number }>();

let pendingEcho: {
  sessionId: string;
  token: string;
  started: number;
  resolve: (ms: number) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
} | null = null;

export const views = new Map<string, TerminalView>();
export let activeId = '';
export const controller = new Controller(api, onPerf);

function onPerf(event: PerfEvent): void {
if (event.session_id === activeId) {
  perf.record(event);
} else if (event.stage === 'client_output_parse_apply') {
  const stats = backgroundOutput.get(event.session_id) ?? { events: 0, bytes: 0 };
  stats.events += 1;
  stats.bytes += event.bytes ?? 0;
  backgroundOutput.set(event.session_id, stats);
}
  if (event.stage !== 'client_output_parse_apply' || !pendingEcho || event.session_id !== pendingEcho.sessionId) return;
  const view = views.get(pendingEcho.sessionId);
  if (!view) return;
  const buf = view.terminal.buffer.active;
  const lines: string[] = [];
  for (let i = Math.max(0, buf.baseY + buf.cursorY - 7); i <= buf.baseY + buf.cursorY && i < buf.length; i++) {
    const line = buf.getLine(i);
    if (line) lines.push(line.translateToString(true));
  }
  const text = lines.join('\n');
  if (text.includes('RC_ECHO:' + pendingEcho.token)) {
    clearTimeout(pendingEcho.timer);
    const started = pendingEcho.started;
    const resolve = pendingEcho.resolve;
    pendingEcho = null;
    resolve(performance.now() - started);
  }
}

export function waitFor(predicate: () => boolean, timeout = 10000): Promise<void> {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const tick = () => {
      if (predicate()) return resolve();
      if (performance.now() - start > timeout) return reject(new Error('waitFor timed out'));
      setTimeout(tick, 10);
    };
    tick();
  });
}

export async function init(ticket: string, sessions: SessionInfo[]): Promise<{ clientId: string }> {
  await api.pair(ticket, 'PERF-browser');
  await controller.connect();
  await waitFor(() => controller.connected);
  activeId = sessions[0].session_id;
  for (const s of sessions) {
    const el = document.createElement('div');
    el.style.width = '1000px';
    el.style.height = '420px';
    document.body.appendChild(el);
    const view = new TerminalView(el, s, controller, () => activeId === s.session_id, () => {}, onPerf);
    views.set(s.session_id, view);
    await waitFor(() => view.ready);
    controller.acquire(s.session_id);
    await waitFor(() => controller.owns(s.session_id));
    view.updateInputGate();
  }
  return { clientId: controller.host!.client_id };
}

export function text(id: string): string {
  const view = views.get(id);
  if (!view) throw new Error('view missing');
  const buf = view.terminal.buffer.active;
  const lines: string[] = [];
  for (let i = Math.max(0, buf.length - 32); i < buf.length; i++) {
    lines.push(buf.getLine(i)!.translateToString(true));
  }
  return lines.join('\n');
}

export function input(id: string, data: string): void {
  const view = views.get(id);
  if (!view) throw new Error('view missing');
  if (!view.canInput()) throw new Error('cannot input');
  view.terminal.input(data, true);
}

export async function stop(): Promise<void> {
  if (pendingEcho) {
    clearTimeout(pendingEcho.timer);
    pendingEcho.reject(new Error('stopped'));
    pendingEcho = null;
  }
  for (const view of views.values()) view.dispose();
  controller.stop();
}
export async function echoSamples(id: string, count: number): Promise<number[]> {
  if (!Number.isInteger(count) || count < 1 || count > 10000) throw new Error('count out of range');
  if (!views.has(id)) throw new Error('unknown id');
  if (pendingEcho !== null) throw new Error('busy');
  const results: number[] = [];
  perf.clear();
  for (let i = 0; i < count; i++) {
    const token = 'R' + crypto.randomUUID().replaceAll('-', '') + '_' + i;
    const ms = await new Promise<number>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (pendingEcho && pendingEcho.token === token) {
          const t = pendingEcho; pendingEcho = null;
          reject(new Error('echo timeout'));
        }
      }, 5000);
      pendingEcho = { sessionId: id, token, started: performance.now(), resolve, reject, timer };
      try { input(id, token + '\n'); }
      catch (err) { clearTimeout(timer); pendingEcho = null; reject(err as Error); }
    });
    results.push(ms);
  }
  await waitFor(() => perf.events.filter(e => e.stage === 'agent_input_write' && e.session_id === id).length >= count);
  if (perf.dropped > 0) throw new Error('instrumentation overflow');
  return results;
}

export async function reconnectSamples(id: string, count: number): Promise<number[]> {
  if (!Number.isInteger(count) || count < 1 || count > 100) {
    throw new Error("count must be integer in 1..100");
  }
  const results: number[] = [];
  for (let i = 0; i < count; i++) {
    const oldView = views.get(id);
    if (!oldView) throw new Error(`view ${id} not found`);
    const session = oldView.session;
    const container = oldView.terminal.element?.parentElement;
    if (!container) throw new Error(`container missing for ${id}`);
    const start = performance.now();
    oldView.dispose();
    const newView = new TerminalView(
      container,
      session,
      controller,
      () => activeId === id,
      () => {},
      onPerf,
    );
    views.set(id, newView);
    await waitFor(() => newView.ready);
    results.push(performance.now() - start);
  }
  return results;
}

export { openCreditProbe } from './credit-probe.js';

export async function ctrlCSamples(id: string, count: number): Promise<number[]> {
  if (!Number.isInteger(count) || count < 1 || count > 10000) throw new RangeError('invalid sample count');
  const results: number[] = [];
  for (let i = 0; i < count; i++) {
    const startIndex = perf.events.length;
    const startTime = performance.now();
    input(id, '\u0003');
    await new Promise<number>((resolve, reject) => {
      const check = () => {
        for (let j = startIndex; j < perf.events.length; j++) {
          const ev = perf.events[j];
          if (ev.stage === 'agent_input_write' && ev.session_id === id) {
            resolve(ev.elapsed_ms);
            return;
          }
        }
        if (performance.now() - startTime > 5000) {
          reject(new Error('timeout'));
          return;
        }
        setTimeout(check, 1);
      };
      check();
    }).then((ms) => { results.push(ms); });
  }
  return results;
}
