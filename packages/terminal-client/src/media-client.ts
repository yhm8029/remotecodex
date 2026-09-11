import { candidateAllowed, signalSdp, PresentationGate, ProtocolError, type MediaSource } from '@remotecodex/core';
import { AgentApi } from './api.js';

export type GuiAction =
  | { kind: 'move'; x: number; y: number }
  | { kind: 'button'; x: number; y: number; button: number; down: boolean }
  | { kind: 'wheel'; x: number; y: number; delta: number }
  | { kind: 'key'; scan: number; extended: boolean; down: boolean }
  | { kind: 'text'; text: string };

/** Independent of terminal control: closing video never closes a PTY or its writer socket. */
export class MediaClient extends EventTarget {
  source: MediaSource | null = null;
  notice = ''; state = 'idle'; lease: string | null = null;
  private socket: WebSocket | null = null;
  private peer: RTCPeerConnection | null = null;
  private attempt = 0; private seq = 0n;
  private readonly gate = new PresentationGate();
  private heartbeat: ReturnType<typeof setInterval> | undefined;
  private stale: ReturnType<typeof setInterval> | undefined;
  private frameCallback = 0; private lastPresentedSent = -Infinity;
  private pendingIce: RTCIceCandidateInit[] = [];
  private signalChain: Promise<void> = Promise.resolve();
  constructor(readonly api: AgentApi, private readonly video: HTMLVideoElement) { super(); }
  private changed(): void { this.dispatchEvent(new Event('change')); }
  async open(source: MediaSource): Promise<void> {
    this.stop(); const attempt = ++this.attempt; this.source = source; this.state = 'connecting';
    this.notice = ''; this.gate.reset(source.generation, source.geometry_version); this.changed();
    try {
      const ws = await this.api.websocket('media', source.id);
      if (attempt !== this.attempt) { ws.close(); return; }
      const pc = new RTCPeerConnection({ iceServers: [], bundlePolicy: 'max-bundle', iceCandidatePoolSize: 0 });
      this.socket = ws; this.peer = pc;
      pc.onicecandidate = event => {
        if (attempt !== this.attempt || !event.candidate || !candidateAllowed(event.candidate.candidate)) return;
        try { this.send({ type: 'ice', candidate: event.candidate.candidate, mline: event.candidate.sdpMLineIndex ?? 0 }); }
        catch (e) { this.fail(e); }
      };
      pc.onconnectionstatechange = () => {
        if (attempt !== this.attempt) return;
        const connected = pc.connectionState === 'connected'; this.gate.transport(connected);
        if (['failed', 'closed', 'disconnected'].includes(pc.connectionState)) {
          this.release(); this.state = 'stalled'; this.notice = '영상 연결이 중단되어 원격 입력을 해제했습니다.';
        }
        this.changed();
      };
      pc.ontrack = event => {
        if (attempt !== this.attempt || event.track.kind !== 'video') return;
        this.video.srcObject = new MediaStream([event.track]);
        event.track.onended = () => { if (attempt === this.attempt) this.fail(new Error('회사 영상이 종료됐습니다.')); };
        void this.video.play().catch(() => { this.notice = '재생 버튼을 눌러 영상을 시작하세요.'; this.changed(); });
        this.watchFrames(attempt);
      };
      ws.onmessage = event => {
        // setRemoteDescription/createAnswer/addIceCandidate must execute in wire order.
        this.signalChain = this.signalChain.then(async () => {
          if (attempt !== this.attempt) return;
          if (typeof event.data !== 'string' || event.data.length > 96 * 1024) throw new ProtocolError('INVALID_SIGNAL');
          await this.message(JSON.parse(event.data) as Record<string, unknown>, pc, attempt);
        }).catch(e => { if (attempt === this.attempt) this.fail(e); });
      };
      ws.onclose = () => { if (attempt === this.attempt) this.fail(new Error('영상 연결 종료. 터미널은 계속 유지됩니다.')); };
      ws.onerror = () => { if (attempt === this.attempt) this.fail(new Error('미디어 연결 오류')); };
      this.heartbeat = setInterval(() => {
        try { if (this.lease) this.send({ type: 'renew', lease_epoch: this.lease }); else this.send({ type: 'ping' }); }
        catch (e) { this.fail(e); }
      }, 5000);
      this.stale = setInterval(() => {
        if (this.lease && !this.canControl()) { this.release(); this.state = 'stalled'; this.notice = '새 프레임을 확인할 수 없어 입력을 해제했습니다.'; this.changed(); }
      }, 100);
    } catch (e) { if (attempt === this.attempt) this.fail(e); throw e; }
  }
  private async message(m: Record<string, unknown>, pc: RTCPeerConnection, attempt: number): Promise<void> {
    switch (m.type) {
      case 'hello': {
        const s = m.source as MediaSource;
        if (m.protocol !== 1 || s?.id !== this.source?.id || s.generation !== this.source.generation || s.geometry_version !== this.source.geometry_version) throw new ProtocolError('SOURCE_CHANGED');
        break;
      }
      case 'offer': {
        await pc.setRemoteDescription({ type: 'offer', sdp: signalSdp(String(m.sdp)) });
        if (attempt !== this.attempt) return;
        for (const c of this.pendingIce.splice(0)) await pc.addIceCandidate(c);
        const answer = await pc.createAnswer(); if (!answer.sdp) throw new ProtocolError('NO_SDP');
        await pc.setLocalDescription(answer); if (attempt !== this.attempt) return;
        this.send({ type: 'answer', sdp: signalSdp(answer.sdp) }); break;
      }
      case 'ice': {
        const candidate = String(m.candidate);
        if (m.mline !== 0 || !candidateAllowed(candidate)) throw new ProtocolError('ICE_REJECTED');
        const c = { candidate, sdpMLineIndex: 0 };
        if (pc.remoteDescription) await pc.addIceCandidate(c);
        else { if (this.pendingIce.length >= 32) throw new ProtocolError('ICE_LIMIT'); this.pendingIce.push(c); }
        break;
      }
      case 'lease':
        if (m.lease_epoch !== null && (typeof m.lease_epoch !== 'string' || !/^[1-9][0-9]*$/.test(m.lease_epoch))) throw new ProtocolError('INVALID_LEASE');
        this.lease = m.lease_epoch as string | null; this.seq = 0n;
        this.notice = this.lease ? '회사 PC 원격 입력 중. 회사의 마우스·키보드 사용 시 해제됩니다.' : String(m.reason ?? '보기 전용'); this.changed(); break;
      case 'error':
        // A rejected acquire is recoverable; pipeline/source failures require a fresh view.
        if (['GUI_CONTROL_NOT_READY_OR_FORBIDDEN', 'FOREGROUND_OR_NATIVE_SAFETY_DENIED'].includes(String(m.code))) {
          this.lease = null; this.notice = String(m.code); this.changed();
        } else throw new ProtocolError(String(m.code));
        break;
      case 'pong': break;
      default: throw new ProtocolError('UNKNOWN_MEDIA_MESSAGE');
    }
  }
  private watchFrames(attempt: number): void {
    if (typeof this.video.requestVideoFrameCallback !== 'function') {
      this.notice = '이 브라우저는 프레임 확인 API가 없어 보기 전용으로 동작합니다.'; this.changed(); return;
    }
    const next: VideoFrameRequestCallback = now => {
      if (attempt !== this.attempt || !this.source) return;
      this.gate.presented(now); this.state = this.gate.state;
      if (now - this.lastPresentedSent >= 100) {
        try { this.send({ type: 'presented', generation: this.source.generation, geometry_version: this.source.geometry_version }); }
        catch (e) { this.fail(e); return; }
        this.lastPresentedSent = now; this.changed();
      }
      this.frameCallback = this.video.requestVideoFrameCallback(next);
    };
    this.frameCallback = this.video.requestVideoFrameCallback(next);
  }
  canControl(): boolean {
    const s = this.source;
    return !!s && this.gate.canControl(performance.now(), s.generation, s.geometry_version);
  }
  acquire(): void {
    if (!this.source?.control_allowed || !this.canControl()) throw new ProtocolError('GUI_FRAME_NOT_READY');
    this.send({ type: 'acquire', generation: this.source.generation, geometry_version: this.source.geometry_version });
  }
  action(action: GuiAction): void {
    if (!this.lease || !this.source || !this.canControl()) { this.release(); throw new ProtocolError('GUI_INPUT_NOT_READY'); }
    try {
      this.send({ type: 'input', lease_epoch: this.lease, generation: this.source.generation, geometry_version: this.source.geometry_version, seq: (++this.seq).toString(), action });
    } catch (error) {
      // A lost key-up/click under congestion must not leave an apparently active lease.
      // Closing the media channel makes the host release all this stream's injected keys.
      this.fail(error); throw error;
    }
  }
  release(): void {
    const owned = this.lease !== null; this.lease = null; this.seq = 0n;
    if (owned && this.socket?.readyState === 1 && this.socket.bufferedAmount < 64 * 1024) this.socket.send(JSON.stringify({ type: 'release' }));
    this.changed();
  }
  async refreshCredentials(): Promise<void> {
    const s = this.source; if (!s || !this.socket) return;
    const attempt = this.attempt; const ticket = await this.api.wsTicket('media', s.id);
    if (attempt === this.attempt) this.send({ type: 'refresh_auth', ticket });
  }
  private send(message: Record<string, unknown>): void {
    if (!this.socket || this.socket.readyState !== 1) throw new ProtocolError('MEDIA_DISCONNECTED');
    if (this.socket.bufferedAmount > 64 * 1024) throw new ProtocolError('MEDIA_INPUT_BACKPRESSURE');
    this.socket.send(JSON.stringify(message));
  }
  private fail(e: unknown): void { const message = e instanceof Error ? e.message : '영상 오류'; this.stop(); this.state = 'failed'; this.notice = message; this.changed(); }
  stop(): void {
    ++this.attempt; this.release(); clearInterval(this.heartbeat); clearInterval(this.stale);
    if (this.frameCallback && this.video.cancelVideoFrameCallback) this.video.cancelVideoFrameCallback(this.frameCallback);
    this.frameCallback = 0; this.lastPresentedSent = -Infinity; this.pendingIce = []; this.signalChain = Promise.resolve();
    const ws = this.socket; this.socket = null;
    if (ws?.readyState === 1 && ws.bufferedAmount < 64 * 1024) ws.send(JSON.stringify({ type: 'stop' }));
    ws?.close(); const pc = this.peer; this.peer = null; pc?.close();
    const stream = this.video.srcObject;
    if (stream && typeof (stream as MediaStream).getTracks === 'function') for (const t of (stream as MediaStream).getTracks()) t.stop();
    this.video.srcObject = null; this.gate.stop(); this.state = 'stopped'; this.source = null; this.changed();
  }
}
