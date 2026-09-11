import { Kind } from './frame.js';
import { ProtocolError, type SnapshotMeta, type TerminalFrame } from './types.js';
export class StreamGuard {
  phase: 'awaiting' | 'snapshot' | 'live' | 'failed' = 'awaiting';
  sequence = 0n;
  meta: SnapshotMeta | null = null;
  private received = 0;
  constructor(readonly sessionId: string, readonly generation: number, readonly agentEpoch: string) {}
  accept(frame: TerminalFrame): void {
    try { this.validate(frame); } catch (error) { this.phase = 'failed'; throw error; }
  }
  private validate(f: TerminalFrame): void {
    if (this.phase === 'failed') throw new ProtocolError('STREAM_ALREADY_FAILED');
    if (f.sessionId !== this.sessionId || f.generation !== this.generation) throw new ProtocolError('WRONG_SESSION');
    if (f.kind === Kind.SnapshotMeta) {
      if (this.phase !== 'awaiting' || f.payload.length > 16384) throw new ProtocolError('UNEXPECTED_SNAPSHOT');
      const m = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(f.payload)) as SnapshotMeta;
      if (m.agent_epoch !== this.agentEpoch || m.generation !== this.generation || BigInt(m.sequence) !== f.sequence ||
          !Number.isInteger(m.cols) || m.cols < 2 || m.cols > 400 || !Number.isInteger(m.rows) || m.rows < 1 || m.rows > 150 ||
          !Number.isInteger(m.bytes) || m.bytes < 0 || m.bytes > 4 * 1024 * 1024 || typeof m.fidelity !== 'string' || !Array.isArray(m.warnings)) throw new ProtocolError('INVALID_SNAPSHOT');
      this.meta = m; this.sequence = f.sequence; this.phase = 'snapshot'; return;
    }
    if (this.phase === 'snapshot') {
      if (f.sequence !== this.sequence) throw new ProtocolError('SNAPSHOT_SEQUENCE');
      if (f.kind === Kind.SnapshotChunk) {
        this.received += f.payload.length;
        if (this.received > (this.meta?.bytes ?? 0)) throw new ProtocolError('SNAPSHOT_TOO_LARGE');
        return;
      }
      if (f.kind === Kind.SnapshotEnd && f.payload.length === 0 && this.received === this.meta?.bytes) { this.phase = 'live'; return; }
      throw new ProtocolError('INCOMPLETE_SNAPSHOT');
    }
    if (this.phase !== 'live' || ![Kind.Output, Kind.Resize, Kind.Exit].includes(f.kind as 1 | 2 | 6) || f.sequence !== this.sequence + 1n) throw new ProtocolError('OUTPUT_GAP');
    if (f.kind === Kind.Resize) {
      if (f.payload.length !== 4) throw new ProtocolError('INVALID_RESIZE');
      const v = new DataView(f.payload.buffer, f.payload.byteOffset, 4);
      if (v.getUint16(0) < 2 || v.getUint16(0) > 400 || v.getUint16(2) < 1 || v.getUint16(2) > 150) throw new ProtocolError('INVALID_RESIZE');
    }
    this.sequence = f.sequence;
  }
}
/** Separate render backlog from server byte credit. Overflow reconnects; never drops VT bytes. */
export class RenderBudget {
  private bytes = 0;
  constructor(readonly limit = 1024 * 1024) {}
  reserve(n: number): void {
    if (!Number.isSafeInteger(n) || n < 0 || this.bytes + n > this.limit) throw new ProtocolError('RESYNC_REQUIRED');
    this.bytes += n;
  }
  applied(n: number): void {
    if (!Number.isSafeInteger(n) || n < 0 || n > this.bytes) throw new ProtocolError('INVALID_CREDIT'); this.bytes -= n;
  }
  get pending(): number { return this.bytes; }
}
