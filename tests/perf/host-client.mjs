import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve } from 'node:path';
import { randomUUID, webcrypto } from 'node:crypto';

const pExecFile = promisify(execFile);
const subtle = webcrypto.subtle;
const b64u = b => Buffer.from(b).toString('base64url');

class HostClient {
  constructor(agent, origin = 'http://127.0.0.1:3847') {
    if (origin !== 'http://127.0.0.1:3847')
      throw new RangeError('loopback origin only');
    this.agent = resolve(agent);
    this.origin = origin;
    this.token = '';
    this.deviceId = null;
    this.made = [];
  }

  async api(path, body, method = body ? 'POST' : 'GET', auth = true) {
    if (auth && this.keys && (!this.token || Date.now() >= this.refreshAt)) await this.authenticate();
    const headers = { Origin: this.origin };
    if (auth && this.token) headers.Authorization = `Bearer ${this.token}`;
    if (body) headers['Content-Type'] = 'application/json';
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), 12000);
    try {
      const r = await fetch(this.origin + '/api/v1' + path, {
        method, headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: ctl.signal,
      });
      if (r.status === 204) return null;
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      return await r.json();
    } finally { clearTimeout(t); }
  }

  async pair() {
    const { stdout } = await pExecFile(this.agent, ['pair'], { windowsHide: true, timeout: 5000, maxBuffer: 16384 });
    const ticket = JSON.parse(stdout).ticket;
    const keys = await subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify']);
    const pub = b64u(await subtle.exportKey('raw', keys.publicKey));
    const paired = await this.api('/auth/pair', { ticket, public_key: pub, label: 'PERF-' + randomUUID() }, 'POST', false);
    this.deviceId = paired.client_id;
    this.keys = keys;
    this.audience = paired.audience;
    await this.authenticate();
    return this.deviceId;
}

  async authenticate(){
  if(!this.keys||!this.keys.privateKey)throw new Error('keys missing');
  if(!this.deviceId)throw new Error('deviceId missing');
  const c=await this.api('/auth/challenge',{client_id:this.deviceId},'POST',false);
  if(!c||!c.audience||!c.message||!c.challenge_id)throw new Error('bad challenge');
  if(c.audience!==this.audience)throw new Error('audience mismatch');
  const prefix='RemoteCodex/v1\n'+this.audience+'\n'+this.deviceId+'\n';
  if(!c.message.startsWith(prefix))throw new Error('bad challenge prefix');
  const sig=await subtle.sign({name:'ECDSA',hash:'SHA-256'},this.keys.privateKey,new TextEncoder().encode(c.message));
  const r=await this.api('/auth/verify',{challenge_id:c.challenge_id,signature:b64u(sig)},'POST',false);
  if(!r||!r.access_token)throw new Error('no access_token');
  this.token=r.access_token;
  this.refreshAt=Date.now()+8*60*1000;
}

  async create(label, cwd) {
    const s = await this.api('/sessions',
      { label, project_id: null, cwd, profile: 'cmd', cols: 100, rows: 24 });
    this.made.push(s);
    return s;
  }

  async remove(session) {
  const idx = this.made.findIndex(s => s.session_id === session.session_id);
  if (idx === -1) {
    throw new Error('NOT_OWNED');
  }
  try {
    await this.api('/sessions/' + session.session_id, undefined, 'DELETE');
    this.made.splice(idx, 1);
  } catch (err) {
    this.made[idx] = session;
    throw err;
  }
}

async cleanup() {
  const errs = [];
  const sessions = [...this.made];
  for (const s of sessions) {
    try {
      await this.remove(s);
    } catch (err) {
      errs.push(err);
    }
  }
  if (this.deviceId && this.token) {
    try {
      await this.api('/devices/' + this.deviceId, undefined, 'DELETE');
      this.deviceId = null;
    } catch (err) {
      errs.push(err);
    }
  } else if (this.deviceId && !this.token) {
    errs.push(new Error('Device cleanup required: ' + this.deviceId));
  }
  try {
    this.token = '';
  } catch (e) {
    errs.push(e);
  }
  if (errs.length) {
    throw new AggregateError(errs, 'Cleanup encountered errors');
  }
}
}
export { HostClient };
