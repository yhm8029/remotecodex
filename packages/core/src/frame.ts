import { ProtocolError, type TerminalFrame } from './types.js';
export const HEADER_SIZE = 40;
export const MAX_PAYLOAD = 32 * 1024;
export const MAX_MESSAGE = 256 * 1024;
export const Kind = Object.freeze({ Output: 1, Resize: 2, SnapshotMeta: 3, SnapshotChunk: 4, SnapshotEnd: 5, Exit: 6 });
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
export function uuidBytes(id: string): Uint8Array {
  if (!UUID.test(id)) throw new ProtocolError('INVALID_UUID');
  const hex = id.replaceAll('-', '');
  return Uint8Array.from({ length: 16 }, (_, i) => Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16));
}
export function bytesUuid(bytes: Uint8Array): string {
  if (bytes.length !== 16) throw new ProtocolError('INVALID_UUID');
  const h = Array.from(bytes, n => n.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}
export function decodeFrames(data: ArrayBuffer | Uint8Array): TerminalFrame[] {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  if (!bytes.length || bytes.length > MAX_MESSAGE) throw new ProtocolError('FRAME_TOO_LARGE');
  const result: TerminalFrame[] = [];
  for (let offset = 0; offset < bytes.length;) {
    if (bytes.length - offset < HEADER_SIZE) throw new ProtocolError('TRUNCATED_HEADER');
    const view = new DataView(bytes.buffer, bytes.byteOffset + offset, HEADER_SIZE);
    if (view.getUint32(0) !== 0x5243544d || view.getUint8(4) !== 1) throw new ProtocolError('PROTOCOL_VERSION');
    const kind = view.getUint8(5);
    if (kind < 1 || kind > 6 || view.getUint16(6) !== 0) throw new ProtocolError('INVALID_FLAGS_OR_KIND');
    const size = view.getUint32(36);
    if (size > MAX_PAYLOAD) throw new ProtocolError('FRAME_TOO_LARGE');
    const end = offset + HEADER_SIZE + size;
    if (end > bytes.length) throw new ProtocolError('TRUNCATED_PAYLOAD');
    result.push({ kind, sessionId: bytesUuid(bytes.subarray(offset + 8, offset + 24)),
      generation: view.getUint32(24), sequence: view.getBigUint64(28), payload: bytes.subarray(offset + HEADER_SIZE, end) });
    offset = end;
  }
  return result;
}
export function encodeFrame(frame: TerminalFrame): Uint8Array {
  if (frame.payload.length > MAX_PAYLOAD || !Number.isInteger(frame.kind) || frame.kind < 1 || frame.kind > 6 ||
      !Number.isInteger(frame.generation) || frame.generation < 0 || frame.generation > 0xffffffff ||
      frame.sequence < 0n || frame.sequence > 0xffffffffffffffffn) throw new ProtocolError('INVALID_FRAME');
  const bytes = new Uint8Array(HEADER_SIZE + frame.payload.length); const v = new DataView(bytes.buffer);
  v.setUint32(0, 0x5243544d); v.setUint8(4, 1); v.setUint8(5, frame.kind);
  bytes.set(uuidBytes(frame.sessionId), 8); v.setUint32(24, frame.generation); v.setBigUint64(28, frame.sequence);
  v.setUint32(36, frame.payload.length); bytes.set(frame.payload, HEADER_SIZE); return bytes;
}
