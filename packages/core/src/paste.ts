import { ProtocolError } from './types.js';
export const MAX_PASTE_BYTES = 256 * 1024;
export function validatePaste(text: string): number {
  if (!text || text.includes('\0')) throw new ProtocolError('INVALID_PASTE');
  // Do not let pasted VT sequences terminate bracketed paste or become live controls.
  if (/[\x01-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(text)) throw new ProtocolError('PASTE_CONTROL_CHARACTERS');
  const bytes = new TextEncoder().encode(text).length;
  if (bytes > MAX_PASTE_BYTES) throw new ProtocolError('PASTE_TOO_LARGE');
  return bytes;
}
/** Session-specific exclusion: a paste in A must not disable terminal B. */
export class PasteGate {
  private active = new Set<string>();
  begin(session: string, text: string): void { validatePaste(text); if (this.active.has(session)) throw new ProtocolError('PASTE_IN_PROGRESS'); this.active.add(session); }
  end(session: string): void { this.active.delete(session); }
  allowed(session: string, data: string): boolean { return !this.active.has(session) || data === '\x03'; }
  clear(): void { this.active.clear(); }
  has(session: string): boolean { return this.active.has(session); }
}
