import { ProtocolError } from './types.js';

/** Wire IDs are strings, including Windows handles: never round an HWND through JS Number. */
export interface MediaSource {
  id: string; generation: number; geometry_version: string; label: string;
  kind: 'window' | 'monitor'; control_allowed: boolean;
  rect: { left: number; top: number; width: number; height: number };
}
export interface VideoProfile { width: number; height: number; fps: number; bitrate_kbps: number }
export function validateVideoProfile(p: VideoProfile): VideoProfile {
  for (const n of Object.values(p)) if (!Number.isInteger(n)) throw new ProtocolError('INVALID_PROFILE');
  if (p.width < 64 || p.width > 1920 || p.height < 64 || p.height > 1080 ||
      p.width % 2 || p.height % 2 || p.fps < 1 || p.fps > 30 ||
      p.bitrate_kbps < 128 || p.bitrate_kbps > 12000) throw new ProtocolError('INVALID_PROFILE');
  return { ...p };
}
export function fitVideo(width: number, height: number, maxWidth = 1280, maxHeight = 720) {
  if (![width, height, maxWidth, maxHeight].every(n => Number.isFinite(n) && n >= 64)) throw new ProtocolError('INVALID_SIZE');
  const scale = Math.min(1, maxWidth / width, maxHeight / height);
  return { width: Math.max(64, Math.floor(width * scale / 2) * 2), height: Math.max(64, Math.floor(height * scale / 2) * 2) };
}
export type MediaState = 'idle' | 'connecting' | 'live' | 'stalled' | 'stopped' | 'failed';
/** Presentation age and source geometry are BOTH required; a WS heartbeat is not a video frame. */
export class PresentationGate {
  private lastPresented = -Infinity;
  private generation = 0;
  private geometry = '';
  private connected = false;
  state: MediaState = 'idle';
  reset(generation: number, geometry: string): void {
    this.generation = generation; this.geometry = geometry; this.lastPresented = -Infinity;
    this.connected = false; this.state = 'connecting';
  }
  transport(connected: boolean): void { this.connected = connected; if (!connected) { this.lastPresented = -Infinity; this.state = 'stalled'; } }
  geometryChanged(generation: number, geometry: string): void {
    if (generation !== this.generation || geometry !== this.geometry) {
      this.generation = generation; this.geometry = geometry; this.lastPresented = -Infinity; this.state = 'stalled';
    }
  }
  presented(now: number): void { if (this.connected && Number.isFinite(now)) { this.lastPresented = now; this.state = 'live'; } }
  canControl(now: number, generation: number, geometry: string): boolean {
    const age = now - this.lastPresented;
    return this.connected && this.state === 'live' && generation === this.generation && geometry === this.geometry && age >= 0 && age <= 500;
  }
  stop(): void { this.connected = false; this.lastPresented = -Infinity; this.state = 'stopped'; }
}
/** Only numeric Tailscale IPv4 host candidates; no mDNS resolution, STUN or TURN by accident. */
export function tailnetIpv4(ip: string): boolean {
  const parts = ip.split('.');
  if (parts.length !== 4 || parts.some(s => !/^(0|[1-9][0-9]{0,2})$/.test(s))) return false;
  const n = parts.map(Number);
  return n.every(x => x <= 255) && n[0] === 100 && n[1]! >= 64 && n[1]! <= 127;
}
export function candidateAllowed(candidate: string, allowedIp?: string): boolean {
  if (candidate.length > 2048 || /[\r\n\0]/.test(candidate)) return false;
  const p = candidate.trim().split(/\s+/);
  return p.length >= 8 && /^candidate:[^\s:]+$/.test(p[0]!) && p[1] === '1' &&
    p[2]!.toLowerCase() === 'udp' && /^\d+$/.test(p[3]!) && Number(p[3]) <= 4294967295 &&
    tailnetIpv4(p[4]!) && (!allowedIp || p[4] === allowedIp) && /^\d+$/.test(p[5]!) &&
    Number(p[5]) > 0 && Number(p[5]) <= 65535 && p[6] === 'typ' && p[7] === 'host';
}
/** Browser offers are stripped of all embedded candidates; addIceCandidate is separately checked. */
export function signalSdp(sdp: string): string {
  if (sdp.length > 65536 || sdp.includes('\0')) throw new ProtocolError('INVALID_SDP');
  const lines = sdp.split(/\r?\n/);
  if (lines.filter(l => l.startsWith('m=')).length !== 1 || !lines.some(l => l.startsWith('m=video '))) throw new ProtocolError('VIDEO_ONLY');
  if (!lines.some(l => /^a=fingerprint:sha-256 (?:[\dA-F]{2}:){31}[\dA-F]{2}$/i.test(l))) throw new ProtocolError('DTLS_REQUIRED');
  if (!lines.some(l => /^a=rtpmap:\d+ H264\/90000$/i.test(l))) throw new ProtocolError('H264_REQUIRED');
  return lines.filter(l => !l.startsWith('a=candidate:') && l !== 'a=end-of-candidates').join('\r\n');
}
/** Native keyboard scan codes are explicit, not browser keyCode values. */
const scans: Record<string, readonly [number, boolean]> = {
 Escape:[1,false],Digit1:[2,false],Digit2:[3,false],Digit3:[4,false],Digit4:[5,false],Digit5:[6,false],Digit6:[7,false],Digit7:[8,false],Digit8:[9,false],Digit9:[10,false],Digit0:[11,false],Minus:[12,false],Equal:[13,false],Backspace:[14,false],Tab:[15,false],
 KeyQ:[16,false],KeyW:[17,false],KeyE:[18,false],KeyR:[19,false],KeyT:[20,false],KeyY:[21,false],KeyU:[22,false],KeyI:[23,false],KeyO:[24,false],KeyP:[25,false],BracketLeft:[26,false],BracketRight:[27,false],Enter:[28,false],ControlLeft:[29,false],
 KeyA:[30,false],KeyS:[31,false],KeyD:[32,false],KeyF:[33,false],KeyG:[34,false],KeyH:[35,false],KeyJ:[36,false],KeyK:[37,false],KeyL:[38,false],Semicolon:[39,false],Quote:[40,false],Backquote:[41,false],ShiftLeft:[42,false],Backslash:[43,false],
 KeyZ:[44,false],KeyX:[45,false],KeyC:[46,false],KeyV:[47,false],KeyB:[48,false],KeyN:[49,false],KeyM:[50,false],Comma:[51,false],Period:[52,false],Slash:[53,false],ShiftRight:[54,false],AltLeft:[56,false],Space:[57,false],CapsLock:[58,false],
 F1:[59,false],F2:[60,false],F3:[61,false],F4:[62,false],F5:[63,false],F6:[64,false],F7:[65,false],F8:[66,false],F9:[67,false],F10:[68,false],F11:[87,false],F12:[88,false],
 ControlRight:[29,true],AltRight:[56,true],ArrowUp:[72,true],ArrowDown:[80,true],ArrowLeft:[75,true],ArrowRight:[77,true],Home:[71,true],End:[79,true],PageUp:[73,true],PageDown:[81,true],Insert:[82,true],Delete:[83,true],NumpadEnter:[28,true]
};
export function scanCode(code: string): readonly [number, boolean] | null { return scans[code] ?? null; }
/** Coalesce moves ONLY. Discrete buttons/keys are never silently discarded or reordered. */
export class PointerCoalescer<T> {
  private pending: T | null = null;
  move(value: T): void { this.pending = value; }
  take(): T | null { const out = this.pending; this.pending = null; return out; }
  clear(): void { this.pending = null; }
}
