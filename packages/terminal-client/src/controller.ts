import { InputSequencer, PasteGate, validatePaste, ownsLease, ProtocolError, type HostInfo, type SessionInfo, type LeaseView } from '@remotecodex/core';
import { AgentApi } from './api.js';
export class Controller extends EventTarget {
  sessions: SessionInfo[] = []; host: HostInfo | null = null; connectionId = ''; connected = false;
  notice = ''; private socket: WebSocket | null = null; private stopped = false;
  private retry = 0; private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private heartbeat: ReturnType<typeof setInterval> | undefined; private refreshTimer: ReturnType<typeof setInterval> | undefined;
  private readonly inputs = new InputSequencer();
  private reload: Promise<void> | null = null;
  private attempt = 0; private reloadEpoch = 0;
  private readonly pastes = new PasteGate();
  constructor(readonly api: AgentApi) { super(); }
  changed(): void { this.dispatchEvent(new Event('change')); }
  async connect(): Promise<void> {
    this.stopped = false; const attempt = ++this.attempt; ++this.reloadEpoch; this.reload = null; clearTimeout(this.retryTimer);
    await this.api.authenticate(); if (this.stopped || attempt !== this.attempt) return;
    const host = await this.api.request<HostInfo>('/host');
    if (this.stopped || attempt !== this.attempt) return;
    this.host = host; await this.refreshSessions();
    if (this.stopped || attempt !== this.attempt) return;
    const socket = await this.api.websocket('control');
    if (this.stopped || attempt !== this.attempt) { socket.close(); return; }
    const previous = this.socket; this.socket = socket; previous?.close();
    socket.onmessage = event => { if (this.socket === socket && attempt === this.attempt && !this.stopped) this.onMessage(String(event.data)); };
    socket.onclose = () => { if (this.socket !== socket) return; this.disconnected(); };
    socket.onerror = () => { if (this.socket !== socket || attempt !== this.attempt || this.stopped) return; this.notice = '연결 오류: 회사 작업은 계속 유지됩니다.'; this.changed(); };
  }
  private onMessage(raw: string): void {
    let m: Record<string, unknown>;
    try { m = JSON.parse(raw) as Record<string, unknown>; } catch { this.socket?.close(); return; }
    if (m.type === 'hello') {
      if (m.protocol !== 1 || m.agent_epoch !== this.host?.agent_epoch) { this.socket?.close(); return; }
      this.connectionId = String(m.connection_id); this.connected = true; this.retry = 0;
      clearInterval(this.heartbeat); clearInterval(this.refreshTimer);
      this.heartbeat = setInterval(() => {
        try { for (const s of this.sessions) if (this.owns(s.session_id)) this.send({ type: 'lease_renew', session_id: s.session_id, lease_epoch: s.lease!.epoch }); } catch (error) { this.fail(error); }
      }, 5000);
      this.refreshTimer = setInterval(() => { void this.refreshCredentials().catch(e => this.fail(e)); }, 8 * 60 * 1000);
    } else if (m.type === 'lease') {
      this.sessions = this.sessions.map(s => s.session_id === m.session_id ? { ...s, lease: m.lease as LeaseView } : s);
    } else if (m.type === 'session_changed' || m.type === 'sessions_resync' || m.type === 'lease_released') {
      void this.refreshSessions().catch(e => this.fail(e));
    } else if (m.type === 'accepted') this.inputs.accepted(String(m.input_id));
    else if (m.type === 'written') this.inputs.settled(String(m.input_id));
    else if (['rejected', 'delivery_failed', 'delivery_unknown'].includes(String(m.type))) {
      this.inputs.settled(String(m.input_id ?? m.request_id)); this.notice = String(m.code ?? m.type);
      if (String(m.type).includes('delivery')) this.notice += ' — 자동 재전송하지 않았습니다.';
    }
    this.changed();
  }
  private async refreshCredentials(): Promise<void> {
    if (!this.connected || this.stopped) return;
    await this.api.authenticate(); const ticket = await this.api.wsTicket('control');
    this.send({ type: 'refresh_auth', ticket }); this.dispatchEvent(new Event('credentials'));
  }
  private disconnected(): void {
    this.connected = false; this.connectionId = ''; this.pastes.clear();
    clearInterval(this.heartbeat); clearInterval(this.refreshTimer);
    const unknown = this.inputs.disconnected();
    this.sessions = this.sessions.map(s => ({ ...s, lease: null }));
    this.notice = unknown.length ? `전달 여부가 불명확한 입력 ${unknown.length}개. 자동 재전송하지 않았습니다.` : '연결 복구 중. 회사의 터미널은 종료하지 않았습니다.';
    this.changed(); if (this.stopped) return;
    const wait = Math.min(1000 * 2 ** this.retry++, 15000) + Math.floor(Math.random() * 250);
    this.retryTimer = setTimeout(() => { void this.connect().catch(e => { this.fail(e); this.disconnected(); }); }, wait);
  }
  fail(error: unknown): void { this.notice = error instanceof Error ? error.message : '요청 실패'; this.changed(); }
  refreshSessions(): Promise<void> {
    if (this.reload) return this.reload;
    const epoch = this.reloadEpoch;
    const pending = this.api.request<SessionInfo[]>('/sessions').then(s => { if (epoch === this.reloadEpoch && !this.stopped) { this.sessions = s; this.changed(); } }).finally(() => { if (this.reload === pending) this.reload = null; });
    this.reload = pending; return pending;
  }
  owns(id: string): boolean {
    const s = this.sessions.find(s => s.session_id === id);
    return this.connected && ownsLease(s?.lease, this.host?.client_id ?? '', this.connectionId);
  }
  send(message: Record<string, unknown>): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN || !this.connected) throw new ProtocolError('DISCONNECTED');
    if (this.socket.bufferedAmount > 64 * 1024) throw new ProtocolError('INPUT_BACKPRESSURE');
    this.socket.send(JSON.stringify({ request_id: crypto.randomUUID(), ...message })); // No input debounce.
  }
  acquire(id: string, takeover = false): void { this.send({ type: 'lease_acquire', session_id: id, takeover }); }
  release(id: string): void { const s = this.sessions.find(s => s.session_id === id); if (this.owns(id) && s?.lease) this.send({ type: 'lease_release', session_id: id, lease_epoch: s.lease.epoch }); }
  input(id: string, data: string): void {
    if (!this.pastes.allowed(id, data)) throw new ProtocolError('PASTE_IN_PROGRESS');
    const s = this.sessions.find(s => s.session_id === id); if (!s) throw new ProtocolError('SESSION_LOST');
    const message = this.inputs.create(s, this.host?.client_id ?? '', this.connectionId, data, crypto.randomUUID());
    try { this.send(message); } catch (error) { this.inputs.settled(message.input_id); throw error; }
  }
  binaryInput(id: string, binary: string): void {
    if (this.pastes.has(id)) throw new ProtocolError('PASTE_IN_PROGRESS');
    // xterm onBinary is Latin-1 mouse data, not UTF-8 text. Keep it lossless.
    const s = this.sessions.find(s => s.session_id === id); if (!s) throw new ProtocolError('SESSION_LOST');
    const message = this.inputs.create(s, this.host?.client_id ?? '', this.connectionId, 'x'.repeat(binary.length), crypto.randomUUID());
    try { this.send({ ...message, payload: { kind: 'binary', base64: btoa(binary) } }); }
    catch (error) { this.inputs.settled(message.input_id); throw error; }
  }
  resize(id: string, cols: number, rows: number): void {
    const s = this.sessions.find(s => s.session_id === id); if (!s?.lease || !this.owns(id)) throw new ProtocolError('LEASE_REQUIRED');
    this.send({ type: 'resize', session_id: id, agent_epoch: s.agent_epoch, generation: s.generation, lease_epoch: s.lease.epoch, cols, rows });
  }
  async paste(id: string, text: string, appendEnter: boolean): Promise<void> {
    validatePaste(text);
    const s = this.sessions.find(s => s.session_id === id);
    if (!s) throw new ProtocolError('SESSION_LOST');
    this.pastes.begin(id, text);
    let inputId: string | undefined;
    try {
      const m = this.inputs.create(s, this.host?.client_id ?? '', this.connectionId, text, crypto.randomUUID(), 256 * 1024);
      inputId = m.input_id;
      // Separate HTTP upload does not block the keyboard WebSocket with a 256 KiB paste.
      await this.api.request(`/sessions/${id}/paste`, {
        agent_epoch: m.agent_epoch, generation: m.generation, connection_id: this.connectionId,
        lease_epoch: m.lease_epoch, input_id: m.input_id, input_seq: m.input_seq,
        text, append_enter: appendEnter
      }, 'POST');
    } catch (error) {
      this.notice = '붙여넣기를 완료하지 못했습니다. 일부가 전달됐을 수 있어 자동 재전송하지 않습니다.';
      throw error;
    } finally {
      if (inputId) this.inputs.settled(inputId);
      this.pastes.end(id); this.changed();
    }
  }
  stop(): void {
    this.stopped = true; ++this.attempt; ++this.reloadEpoch; this.reload = null; this.pastes.clear(); clearTimeout(this.retryTimer); clearInterval(this.heartbeat); clearInterval(this.refreshTimer);
    this.socket?.close(); this.socket = null; this.connected = false; this.connectionId = ''; this.sessions = this.sessions.map(s => ({ ...s, lease: null })); this.inputs.disconnected(); this.changed();
    // No DELETE sessions and no Agent shutdown here.
  }
}
