import { ProtocolError } from './types.js';
export interface Rect { x: number; y: number; width: number; height: number }
function validRect(r: Rect): void {
  if (![r.x, r.y, r.width, r.height].every(Number.isFinite) || r.width <= 0 || r.height <= 0 || r.width > 32768 || r.height > 32768) throw new ProtocolError('INVALID_GEOMETRY');
}
export function unletterbox(x: number, y: number, viewport: Rect, source: Rect): [number, number] | null {
  validRect(viewport); validRect(source);
  if (!Number.isFinite(x) || !Number.isFinite(y)) throw new ProtocolError('INVALID_POINT');
  const scale = Math.min(viewport.width / source.width, viewport.height / source.height);
  const w = source.width * scale, h = source.height * scale;
  const left = viewport.x + (viewport.width - w) / 2, top = viewport.y + (viewport.height - h) / 2;
  const nx = (x - left) / w, ny = (y - top) / h;
  return nx < 0 || nx > 1 || ny < 0 || ny > 1 ? null : [nx, ny];
}
export function toPhysical(nx: number, ny: number, rect: Rect): [number, number] {
  validRect(rect);
  if (![nx, ny].every(Number.isFinite) || nx < 0 || nx > 1 || ny < 0 || ny > 1) throw new ProtocolError('INVALID_POINT');
  return [rect.x + Math.round(nx * (rect.width - 1)), rect.y + Math.round(ny * (rect.height - 1))];
}
export function acceptFrameProof(input: { source: string; generation: number; geometry: number; frameAt: number }, expected: { source: string; generation: number; geometry: number }, now: number): boolean {
  return input.source === expected.source && input.generation === expected.generation && input.geometry === expected.geometry &&
    Number.isFinite(input.frameAt) && Number.isFinite(now) && input.frameAt <= now && now - input.frameAt <= 500;
}
