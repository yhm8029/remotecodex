import {AgentApi} from '../../packages/terminal-client/src/api.js';
import {decodeFrames, Kind} from '@remotecodex/core';

const APPLIED = (seq: string) =>
  JSON.stringify({type: 'applied', sequence: seq, bytes: 0});

export type ProbeMode = 'normal' | 'delayed' | 'duplicate';

export interface CreditProbe {
  ready: boolean;
  closed: boolean;
  outputCount: number;
  lastSequence: string;
  snapshotSequence: string;
  mode: ProbeMode;
  error?: string;
  socket: WebSocket;
  close: () => void;
}

export async function openCreditProbe(api: AgentApi, sessionId: string, initialMode: ProbeMode = 'normal'): Promise<CreditProbe> {
  const socket = (await api.websocket('terminal', sessionId)) as WebSocket;
  if (socket.binaryType !== 'arraybuffer') socket.binaryType = 'arraybuffer';

  const probe: CreditProbe = {
    ready: false,
    closed: false,
    outputCount: 0,
    lastSequence: '0',
    snapshotSequence: '0',
    mode: initialMode,
    socket,
    close: () => {},
  };

  const timeouts = new Set<ReturnType<typeof setTimeout>>();
  let dupTimer: ReturnType<typeof setInterval> | null = null;

  const sendApplied = (seq: string) => {
    if (socket.readyState === WebSocket.OPEN) socket.send(APPLIED(seq));
  };

  socket.onmessage = (ev: MessageEvent) => {
    if (probe.closed) return;
    if (!(ev.data instanceof ArrayBuffer)) return;
    let frames;
    try {
      frames = decodeFrames(new Uint8Array(ev.data));
    } catch (e) {
      probe.error = String(e);
      cleanup();
      socket.close();
      return;
    }
    for (const frame of frames) {
      const seq = frame.sequence.toString();
      switch (frame.kind) {
        case Kind.SnapshotEnd:
          probe.ready = true;
          probe.snapshotSequence = seq;
          probe.lastSequence = seq;
          sendApplied(seq);
          break;
        case Kind.Output:
          probe.outputCount++;
          probe.lastSequence = seq;
          if (probe.mode === 'normal') sendApplied(seq);
          else if (probe.mode === 'delayed') {
            const timeout = setTimeout(() => {
              timeouts.delete(timeout);
              sendApplied(seq);
            }, 100);
            timeouts.add(timeout);
          }
          break;
      }
    }
  };

  const cleanup = () => {
    if (probe.closed) return;
    probe.closed = true;
    if (dupTimer) clearInterval(dupTimer);
    dupTimer = null;
    for (const t of timeouts) clearTimeout(t);
    timeouts.clear();
  };

  socket.onclose = cleanup;
  socket.onerror = () => {
    cleanup();
    socket.close();
  };

  probe.close = () => {
    cleanup();
    socket.close();
  };

  dupTimer = setInterval(() => {
    if (probe.mode !== 'duplicate' || !probe.ready) return;
    if (socket.readyState !== WebSocket.OPEN) return;
    sendApplied(probe.snapshotSequence);
  }, 100);

  return probe;
}
