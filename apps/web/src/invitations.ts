export type Invitation = {
  v: 1;
  origin: string;
  label: string;
  ticket: string;
};

type InvitationFields = Omit<Invitation, 'v'>;
const INVITE_ERROR = 'Invalid invitation';
const HOST_LABEL = /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)$/i;
const RAW_ORIGIN = /^https:\/\/([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*\.ts\.net)(?::443)?\/?$/i;
const TICKET = /^[A-Za-z0-9_-]{32,128}$/;

function invalid(): never {
  throw new Error(INVITE_ERROR);
}

export function normalizeHostOrigin(value: string): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 300 || !RAW_ORIGIN.test(value)) invalid();
  let url: URL;
  try { url = new URL(value); } catch { invalid(); }
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash || (url.pathname !== '' && url.pathname !== '/')) invalid();
  if (url.port && url.port !== '443') invalid();
  const hostname = url.hostname;
  if (hostname.length > 253 || !hostname.toLowerCase().endsWith('.ts.net')) invalid();
  const labels = hostname.split('.');
  if (labels.length < 3 || labels[labels.length - 2] !== 'ts' || labels[labels.length - 1] !== 'net' || labels.slice(0, -2).some((label) => !HOST_LABEL.test(label))) invalid();
  return `https://${hostname.toLowerCase()}`;
}

function validateFields(fields: InvitationFields): InvitationFields {
  const origin = normalizeHostOrigin(fields.origin);
  if (typeof fields.label !== 'string' || typeof fields.ticket !== 'string') invalid();
  const label = fields.label.trim();
  if ([...label].length < 1 || [...label].length > 80 || /[\u0000-\u001f\u007f-\u009f]/.test(label)) invalid();
  if (!TICKET.test(fields.ticket)) invalid();
  return { origin, label, ticket: fields.ticket };
}

function encodeBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function decodeBase64Url(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(value) || value.length % 4 === 1) invalid();
  let binary: string;
  try {
    const padded = value.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - value.length % 4) % 4);
    binary = atob(padded);
  } catch { invalid(); }
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  if (encodeBase64Url(bytes) !== value) invalid();
  return bytes;
}

export function createInvitation(fields: InvitationFields): string {
  const validated = validateFields(fields);
  const label = encodeBase64Url(new TextEncoder().encode(validated.label));
  const link = `${validated.origin}/#rc-invite=2.${validated.ticket}.${label}`;
  if (link.length > 2048) invalid();
  return link;
}

export function parseInvitation(link: string, currentOrigin?: string): Invitation {
  if (typeof link !== 'string' || link.length > 2048 || [...link].some((c) => c.charCodeAt(0) > 127) || /[\s\\]/.test(link)) invalid();
  const parts = link.split('/#rc-invite=');
  if (parts.length !== 2 || !parts[1] || parts[1].includes('#')) invalid();
  const envelopeOrigin = normalizeHostOrigin(parts[0]);
  if (parts[0] !== envelopeOrigin) invalid();
  const token = parts[1];
  if (token.startsWith('2.')) {
    const compact = token.split('.');
    if (compact.length !== 3 || compact[0] !== '2' || !TICKET.test(compact[1]) || compact[1].length < 32 || compact[1].length > 128 || !compact[2]) invalid();
    let label: string;
    try {
      const bytes = decodeBase64Url(compact[2]);
      label = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      if (encodeBase64Url(bytes) !== compact[2]) invalid();
    } catch { invalid(); }
    if (label !== label.trim()) invalid();
    const fields = validateFields({ origin: envelopeOrigin, label, ticket: compact[1] });
    if (currentOrigin !== undefined && normalizeHostOrigin(currentOrigin) !== envelopeOrigin) invalid();
    return { v: 1, ...fields };
  }
  let decoded: string;
  try { decoded = new TextDecoder('utf-8', { fatal: true }).decode(decodeBase64Url(token)); } catch { invalid(); }
  let payload: unknown;
  try { payload = JSON.parse(decoded); } catch { invalid(); }
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) invalid();
  const keys = Object.keys(payload);
  if (keys.length !== 4 || !['v', 'origin', 'label', 'ticket'].every((key) => keys.includes(key))) invalid();
  const candidate = payload as Invitation;
  if (candidate.v !== 1) invalid();
  if (typeof candidate.label !== 'string' || candidate.label !== candidate.label.trim()) invalid();
  const fields = validateFields(candidate);
  if (candidate.origin !== fields.origin || fields.origin !== envelopeOrigin) invalid();
  if (currentOrigin !== undefined && normalizeHostOrigin(currentOrigin) !== envelopeOrigin) invalid();
  return { v: 1, ...fields };
}
