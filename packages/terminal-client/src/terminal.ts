import { emitPerf, type PerfSink } from './perf.js';
import { Terminal, type IDisposable } from '@xterm/xterm';
import { decodeFrames, Kind, StreamGuard, RenderBudget, type SessionInfo } from '@remotecodex/core';
import { Controller } from './controller.js';
import { configureUnicode } from './unicode.js';

export class TerminalView {
  readonly terminal: Terminal;
  ready = false; warnings: string[] = []; unread = 0; atBottom = true;
  private socket: WebSocket | null = null; private stopped = false;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private retry = 0; private epoch = 0;
  private disposables: IDisposable[] = [];
  private subscriptions: (() => void)[] = [];
  constructor(private element: HTMLElement, readonly session: SessionInfo, readonly controller: Controller,
    private focused: () => boolean, private notify: () => void, readonly perf?: PerfSink) {
    this.terminal = new Terminal({ cols: session.cols, rows: session.rows, scrollback: 2000,
      cursorBlink: false, convertEol: false, allowProposedApi: true, allowTransparency: false,
      fontFamily: 'Cascadia Mono, Consolas, monospace', fontSize: 14,
      disableStdin: true, theme: { background: '#0b1017', foreground: '#d8e1ed', cursor: '#70dab2' } });
    configureUnicode(this.terminal);
      const t = this.terminal;
    // The server is the single terminal-query responder. These hooks prevent N viewers replying N times.
    for (const prefix of ['', '?', '>', '=']) for (const final of ['n', 'c', 't']) {
      this.disposables.push(t.parser.registerCsiHandler(prefix ? { prefix, final } : { final }, () => true));
    }
    this.disposables.push(t.parser.registerCsiHandler({ prefix: '?', intermediates: '$', final: 'p' }, () => true));
    this.disposables.push(t.parser.registerCsiHandler({ intermediates: '$', final: 'p' }, () => true));
    this.disposables.push(t.parser.registerOscHandler(4, data => data.split(';').includes('?')));
    for (const id of [10, 11, 12]) this.disposables.push(t.parser.registerOscHandler(id, data => data === '?'));
    for (const id of [8, 52]) this.disposables.push(t.parser.registerOscHandler(id, () => true));
    this.disposables.push(t.onData(data => { const started = this.perf ? performance.now() : 0; if (this.canInput()) try { controller.input(session.session_id,data); if(this.perf)emitPerf(this.perf,{stage:'client_input_enqueue',session_id:session.session_id,elapsed_ms:performance.now()-started}); } catch(e){controller.fail(e);} }));
    this.disposables.push(t.onBinary(data => { const started = this.perf ? performance.now() : 0; if (this.canInput()) try { controller.binaryInput(session.session_id,data); if(this.perf)emitPerf(this.perf,{stage:'client_input_enqueue',session_id:session.session_id,elapsed_ms:performance.now()-started}); } catch(e){controller.fail(e);} }));
    const paste = (event: ClipboardEvent) => {
      event.preventDefault(); event.stopImmediatePropagation();
      if (!this.canInput()) return;
      const value = event.clipboardData?.getData('text/plain') ?? '';
      if (value) void this.paste(value, false).catch(e => controller.fail(e));
    };
    element.addEventListener('paste', paste, true);
    this.subscriptions.push(() => element.removeEventListener('paste', paste, true));
    t.open(element);
    this.disposables.push(t.onScroll(() => {
      const bottom = this.isAtBottom();
      this.atBottom = bottom;
      if (bottom) this.unread = 0;
      this.notify();
    }));
    const change = () => { t.options.disableStdin = !this.canInput(); };
    const credentials = () => { void this.refreshAuth().catch(() => this.socket?.close()); };
    controller.addEventListener('change', change); controller.addEventListener('credentials', credentials);
    this.subscriptions.push(() => controller.removeEventListener('change', change), () => controller.removeEventListener('credentials', credentials));
    void this.connect();
  }
  canInput(): boolean { return this.ready && this.focused() && this.controller.owns(this.session.session_id); }
  updateInputGate(): void { this.terminal.options.disableStdin = !this.canInput(); }
  private isAtBottom(): boolean {
    const buffer = this.terminal.buffer.active;
    return buffer.viewportY >= buffer.baseY;
  }
  scrollToBottom(): void {
    this.terminal.scrollToBottom();
    this.atBottom = true;
    this.unread = 0;
    this.notify();
  }
  focus(): void { this.updateInputGate(); this.terminal.focus(); }
  async paste(text: string, enter: boolean): Promise<void> {
    if (!this.canInput()) throw new Error('제어권과 활성 터미널이 필요합니다.');
    await this.controller.paste(this.session.session_id, text, enter);
  }
  private async connect(): Promise<void> {
    const epoch = ++this.epoch;
    try {
      const socket = await this.controller.api.websocket('terminal', this.session.session_id);
      if (this.stopped || epoch !== this.epoch) { socket.close(); return; }
      this.socket = socket; this.ready = false; this.notify();
      const guard = new StreamGuard(this.session.session_id, this.session.generation, this.session.agent_epoch);
      const budget = new RenderBudget(5 * 1024 * 1024); // Bounded 4MiB initial snapshot + live backlog.
      let queue = Promise.resolve();
      socket.onmessage = event => {
        const received = this.perf ? performance.now() : 0;
        if (!(event.data instanceof ArrayBuffer)) { socket.close(); return; }
        const bytes = event.data as ArrayBuffer;
        try { budget.reserve(bytes.byteLength); } catch { socket.close(); return; }
        queue = queue.then(async () => {
          if (this.stopped || epoch !== this.epoch) return;
          for (const frame of decodeFrames(bytes)) {
            guard.accept(frame);
            if (frame.kind === Kind.SnapshotMeta) {
              const m = guard.meta!; this.warnings = m.warnings; this.terminal.reset(); this.terminal.resize(m.cols, m.rows);
            } else if (frame.kind === Kind.Output || frame.kind === Kind.SnapshotChunk) {
              const liveOutput = frame.kind === Kind.Output;
              const shouldCount = liveOutput && (!this.atBottom || !this.focused());
              await new Promise<void>(resolve => this.terminal.write(frame.payload, resolve));
              // Note: this callback fires on parse/apply completion, not paint
              if (liveOutput && this.perf) emitPerf(this.perf, { stage: 'client_output_parse_apply', session_id: this.session.session_id, elapsed_ms: performance.now() - received, bytes: frame.payload.length, sequence: frame.sequence.toString() });
              this.atBottom = this.isAtBottom();
              if (shouldCount) this.unread += 1;
              if (liveOutput && this.atBottom && this.focused()) this.unread = 0;
              if (liveOutput) this.notify();
            } else if (frame.kind === Kind.Resize) {
              const d = new DataView(frame.payload.buffer, frame.payload.byteOffset, 4); this.terminal.resize(d.getUint16(0), d.getUint16(2));
            } else if (frame.kind === Kind.SnapshotEnd) {
              this.ready = true; this.retry = 0; this.terminal.options.disableStdin = !this.canInput(); this.notify();
            }
            if (guard.phase === 'live' && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'applied', sequence: guard.sequence.toString(), bytes: frame.payload.length + 40 }));
          }
          budget.applied(bytes.byteLength);
        }).catch(error => { this.controller.fail(error); socket.close(); });
      };
      socket.onclose = () => {
        if (this.stopped || epoch !== this.epoch) return;
        this.ready = false; this.terminal.options.disableStdin = true; this.notify();
        this.retryTimer = setTimeout(() => { void this.connect(); }, Math.min(500 * 2 ** this.retry++, 10000));
      };
    } catch (error) {
      if (!this.stopped && epoch === this.epoch) { this.controller.fail(error); this.retryTimer = setTimeout(() => { void this.connect(); }, 3000); }
    }
  }
  private async refreshAuth(): Promise<void> {
    const socket = this.socket; if (socket?.readyState !== WebSocket.OPEN) return;
    const ticket = await this.controller.api.wsTicket('terminal', this.session.session_id);
    if (socket === this.socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'refresh_auth', ticket }));
  }
  dispose(): void {
    this.stopped = true; ++this.epoch; clearTimeout(this.retryTimer); this.socket?.close();
    for (const off of this.subscriptions) off(); for (const d of this.disposables) d.dispose(); this.terminal.dispose(); this.element.replaceChildren();
  }
}
