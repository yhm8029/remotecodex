import { normalizeHostOrigin } from './invitations.ts';

export type SavedHost = { origin: string; label: string };

const KEY = 'remotecodex-hosts-v1';
const MAX_VALUE_CHARS = 32000;
const MAX_ENTRIES = 32;
const PARSE_LIMIT = 128;
const MAX_LABEL = 80;

function isStorageLike(s: any): s is Pick<Storage, 'getItem' | 'setItem'> {
  return !!s && typeof s.getItem === 'function' && typeof s.setItem === 'function';
}

function hasControlChars(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x20 || c === 0x7f) return true;
  }
  return false;
}

function validLabel(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_LABEL) return null;
  if (hasControlChars(trimmed)) return null;
  return trimmed;
}

export function loadHosts(storage: Pick<Storage, 'getItem'>): SavedHost[] {
  let raw: string | null;
  try {
    raw = storage.getItem(KEY);
  } catch {
    return [];
  }
  if (typeof raw !== 'string' || raw.length === 0) return [];
  if (raw.length > MAX_VALUE_CHARS) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];

  const out: SavedHost[] = [];
  const seen = new Set<string>();
  const limit = Math.min(parsed.length, PARSE_LIMIT);
  for (let i = 0; i < limit; i++) {
    const item = parsed[i];
    if (!item || typeof item !== 'object') continue;
    const rec = item as Record<string, unknown>;
    if (typeof rec.origin !== 'string') continue;
    const label = validLabel(rec.label);
    if (label === null) continue;
    let canonical: string;
    try {
      canonical = normalizeHostOrigin(rec.origin);
    } catch {
      continue;
    }
    if (seen.has(canonical)) continue;
    seen.add(canonical);
    out.push({ origin: canonical, label });
    if (out.length >= MAX_ENTRIES) break;
  }
  return out;
}

export function saveHost(
  storage: Pick<Storage, 'getItem' | 'setItem'>,
  host: SavedHost,
): SavedHost[] {
  if (!isStorageLike(storage)) throw new Error('INVALID_STORAGE');
  if (!host || typeof host !== 'object') throw new Error('INVALID_HOST_NAME');
  let origin: string;
  try {
    origin = normalizeHostOrigin(host.origin);
  } catch {
    throw new Error('INVALID_HOST_NAME');
  }
  const label = validLabel(host.label);
  if (label === null) throw new Error('INVALID_HOST_NAME');
  const record: SavedHost = { origin, label };
  const list = [record, ...loadHosts(storage).filter(x => x.origin !== origin)].slice(0, MAX_ENTRIES);
  storage.setItem(KEY, JSON.stringify(list));
  return list;
}

export function removeHost(
  storage: Pick<Storage, 'getItem' | 'setItem'>,
  origin: string,
): SavedHost[] {
  if (!isStorageLike(storage)) throw new Error('INVALID_STORAGE');
  let canonical: string;
  try {
    canonical = normalizeHostOrigin(origin);
  } catch {
    return loadHosts(storage);
  }
  const list = loadHosts(storage).filter(x => x.origin !== canonical);
  storage.setItem(KEY, JSON.stringify(list));
  return list;
}