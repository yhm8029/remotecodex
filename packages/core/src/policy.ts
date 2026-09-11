import { ProtocolError, type SessionInfo, type LeaseView } from './types.js';
export type InputSurface = 'ui' | 'terminal' | 'window' | 'desktop';
export function routePointer(surface: InputSurface, mouseReporting: boolean, hasLease: boolean): 'local' | 'pty' | 'gui' | 'denied' {
  if (surface === 'ui') return 'local';
  if (surface === 'terminal') return !mouseReporting ? 'local' : hasLease ? 'pty' : 'denied';
  return hasLease ? 'gui' : 'denied';
}
export function ownsLease(lease: LeaseView | null | undefined, client: string, connection: string): boolean {
  return !!lease && lease.client_id === client && lease.connection_id === connection && lease.remaining_ms > 0;
}
export function canResize(options: { ownLease: boolean; mobile: boolean; explicit: boolean; visible: boolean }): boolean {
  return options.ownLease && options.visible && (!options.mobile || options.explicit);
}
export class PaneLayout {
  private ids: string[] = [];
  focused: string | null = null;
  constructor(public mobile = false) {}
  get visible(): readonly string[] { return [...this.ids]; }
  show(id: string, split = false): void {
    if (!id) throw new ProtocolError('INVALID_SESSION');
    if (this.mobile || !split) this.ids = [id];
    else if (!this.ids.includes(id)) this.ids = [...this.ids.slice(0, 1), id];
    this.focused = id;
  }
  focus(id: string): void { if (!this.ids.includes(id)) throw new ProtocolError('PANE_NOT_VISIBLE'); this.focused = id; }
  closeView(id: string): void { this.ids = this.ids.filter(x => x !== id); if (this.focused === id) this.focused = this.ids[0] ?? null; }
  setMobile(mobile: boolean): void { this.mobile = mobile; if (mobile && this.ids.length > 1) this.ids = [this.focused ?? this.ids[0]!]; }
  // Closing a view NEVER generates a terminal.close API operation.
}
export class InputSequencer {
  private sequence = 0;
  private pending = new Map<string, { session: string; state: 'sent' | 'accepted' }>();
  create(session: SessionInfo, clientId: string, connection: string, payload: string, id: string, maxBytes = 8192) {
    if (!ownsLease(session.lease, clientId, connection)) throw new ProtocolError('LEASE_REQUIRED');
    if (session.state !== 'running') throw new ProtocolError('SESSION_LOST');
    if (!payload || new TextEncoder().encode(payload).length > maxBytes) throw new ProtocolError('INPUT_TOO_LARGE');
    if (this.pending.size >= 128) throw new ProtocolError('INPUT_BACKPRESSURE');
    if (this.pending.has(id) || this.sequence >= Number.MAX_SAFE_INTEGER) throw new ProtocolError('INVALID_SEQUENCE');
    this.pending.set(id, { session: session.session_id, state: 'sent' });
    return { type: 'input', request_id: id, session_id: session.session_id, agent_epoch: session.agent_epoch,
      generation: session.generation, lease_epoch: session.lease!.epoch, input_id: id,
      input_seq: ++this.sequence, payload: { kind: 'utf8', text: payload } };
  }
  accepted(id: string): void { const p = this.pending.get(id); if (p) p.state = 'accepted'; }
  settled(id: string): void { this.pending.delete(id); }
  disconnected(): { inputId: string; sessionId: string; state: 'delivery_unknown' }[] {
    const unknown = Array.from(this.pending, ([id, p]) => ({ inputId: id, sessionId: p.session, state: 'delivery_unknown' as const }));
    this.pending.clear(); return unknown; // No retry payload is retained.
  }
  get inflight(): number { return this.pending.size; }
}
export function submitByEnter(event: { key: string; isComposing: boolean; shiftKey: boolean; keyCode?: number }, composing: boolean): boolean {
  return event.key === 'Enter' && !event.shiftKey && !event.isComposing && !composing && event.keyCode !== 229;
}
export class Drafts {
  private drafts = new Map<string, string>();
  set(session: string, text: string): void { if (text.length > 65536) throw new ProtocolError('DRAFT_TOO_LARGE'); this.drafts.set(session, text); }
  get(session: string): string { return this.drafts.get(session) ?? ''; }
  clear(session: string): void { this.drafts.delete(session); }
  // In-memory only. No secrets in localStorage, telemetry or filesystem logs.
}
