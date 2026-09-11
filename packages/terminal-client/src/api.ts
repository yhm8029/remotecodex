/** Device keys live in IndexedDB as CryptoKey objects. Bearer tokens stay only in memory. */
import { ProtocolError } from '@remotecodex/core';
interface DeviceRecord { origin: string; clientId: string; audience: string; keys: CryptoKeyPair }
const encoder = new TextEncoder();
export function base64url(bytes: ArrayBuffer | Uint8Array): string {
  const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let text = ''; for (const n of data) text += String.fromCharCode(n);
  return btoa(text).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => { const r = indexedDB.open('remotecodex-device-v1', 1);
    r.onupgradeneeded = () => { r.result.createObjectStore('devices', { keyPath: 'origin' }); };
    r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
}
async function readDevice(origin: string): Promise<DeviceRecord | undefined> {
  const db = await database(); try { return await new Promise((resolve, reject) => {
    const r = db.transaction('devices', 'readonly').objectStore('devices').get(origin);
    r.onsuccess = () => resolve(r.result as DeviceRecord | undefined); r.onerror = () => reject(r.error);
  }); } finally { db.close(); }
}
async function saveDevice(record: DeviceRecord): Promise<void> {
  const db = await database(); try { await new Promise<void>((resolve, reject) => {
    const tx = db.transaction('devices', 'readwrite'); tx.objectStore('devices').put(record);
    tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error);
  }); } finally { db.close(); }
}
export class AgentApi {
  token = ''; private refreshPending: Promise<void> | null = null;
  readonly base: string;
  constructor(base: string) {
    const u = new URL(base);
    if (u.username || u.password || u.pathname !== '/' || u.search || u.hash ||
      (u.protocol !== 'https:' && !(u.protocol === 'http:' && u.hostname === '127.0.0.1' && u.port === '3847'))) throw new ProtocolError('UNSAFE_AGENT_URL');
    this.base = u.origin;
  }
  async request<T>(path: string, body?: unknown, method?: string, authenticated = true): Promise<T> {
    if (!path.startsWith('/') || path.startsWith('//')) throw new ProtocolError('INVALID_API_PATH');
    const headers = new Headers();
    if (body !== undefined) headers.set('Content-Type', 'application/json');
    if (authenticated) { if (!this.token) throw new ProtocolError('PAIRING_REQUIRED'); headers.set('Authorization', `Bearer ${this.token}`); }
    const response = await fetch(`${this.base}/api/v1${path}`, {
      method: method ?? (body === undefined ? 'GET' : 'POST'), headers, cache: 'no-store', credentials: 'omit',
      ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(15000)
    });
    if (!response.ok) { const error = await response.json().catch(() => ({})) as { code?: string }; throw new ProtocolError(error.code ?? `HTTP_${response.status}`); }
    if (response.status === 204) return undefined as T;
    return await response.json() as T;
  }
  async pair(ticket: string, label: string): Promise<void> {
    if (!crypto.subtle) throw new ProtocolError('SECURE_CONTEXT_REQUIRED');
    const keys = await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
    const publicKey = base64url(await crypto.subtle.exportKey('raw', keys.publicKey));
    const paired = await this.request<{ client_id: string; audience: string }>('/auth/pair', { ticket: ticket.trim(), public_key: publicKey, label }, 'POST', false);
    await saveDevice({ origin: this.base, clientId: paired.client_id, audience: paired.audience, keys });
    await this.authenticate();
  }
  authenticate(): Promise<void> {
    if (this.refreshPending) return this.refreshPending;
    this.refreshPending = this.performAuth().finally(() => { this.refreshPending = null; }); return this.refreshPending;
  }
  private async performAuth(): Promise<void> {
    const device = await readDevice(this.base); if (!device) throw new ProtocolError('PAIRING_REQUIRED');
    const c = await this.request<{ challenge_id: string; message: string; audience: string }>('/auth/challenge', { client_id: device.clientId }, 'POST', false);
    if (c.audience !== device.audience || !c.message.startsWith(`RemoteCodex/v1\n${device.audience}\n${device.clientId}\n`)) throw new ProtocolError('AUTH_AUDIENCE_MISMATCH');
    const signature = await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, device.keys.privateKey, encoder.encode(c.message));
    const result = await this.request<{ access_token: string }>('/auth/verify', { challenge_id: c.challenge_id, signature: base64url(signature) }, 'POST', false);
    this.token = result.access_token;
  }
  async websocket(channel: 'control' | 'terminal' | 'media', sessionId?: string): Promise<WebSocket> {
    const ticket = await this.wsTicket(channel, sessionId);
    const url = new URL(`/api/v1/ws/${channel}`, this.base); url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    const socket = new WebSocket(url, ['rctm.v1', `rc-ticket.${ticket}`]); socket.binaryType = 'arraybuffer'; return socket;
  }
  async wsTicket(channel: 'control' | 'terminal' | 'media', sessionId?: string): Promise<string> {
    const t = await this.request<{ ticket: string }>('/tickets/ws', { channel, session_id: sessionId ?? null }); return t.ticket;
  }
}
