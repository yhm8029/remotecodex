import test from'node:test';import assert from'node:assert/strict';import{webcrypto,randomUUID}from'node:crypto';
import{AgentApi}from'../../.test-build/client/packages/terminal-client/src/api.js';
const records=new Map();
globalThis.indexedDB={open(){const req={};queueMicrotask(()=>{req.result={createObjectStore(){},close(){},transaction(){const tx={};tx.objectStore=()=>({get(key){const r={};queueMicrotask(()=>{r.result=records.get(key);r.onsuccess?.();});return r;},put(value){records.set(value.origin,structuredClone(value));queueMicrotask(()=>tx.oncomplete?.());}});return tx;}};req.onupgradeneeded?.();req.onsuccess?.();});return req;}};
const origin='http://127.0.0.1:3847',clientId=randomUUID();let publicKey,challengeId,message;
const calls=[];globalThis.fetch=async(url,options)=>{
  const path=new URL(url).pathname;const body=options.body?JSON.parse(options.body):{};calls.push({path,options});
  if(path.endsWith('/auth/pair')){assert.equal(body.ticket,'one-time-test-ticket');publicKey=await webcrypto.subtle.importKey('raw',Buffer.from(body.public_key,'base64url'),{name:'ECDSA',namedCurve:'P-256'},false,['verify']);return Response.json({client_id:clientId,audience:origin});}
  if(path.endsWith('/auth/challenge')){challengeId=randomUUID();message=`RemoteCodex/v1\n${origin}\n${clientId}\n${randomUUID()}\n${Date.now()+30000}`;return Response.json({challenge_id:challengeId,message,audience:origin});}
  if(path.endsWith('/auth/verify')){assert.equal(body.challenge_id,challengeId);assert.equal(Buffer.from(body.signature,'base64url').length,64);assert.equal(await webcrypto.subtle.verify({name:'ECDSA',hash:'SHA-256'},publicKey,Buffer.from(body.signature,'base64url'),new TextEncoder().encode(message)),true);return Response.json({access_token:'memory-only-access-token',expires_in:600});}
  if(path.endsWith('/tickets/ws'))return Response.json({ticket:'single-use-scoped-ticket'});
  return Response.json({ok:true});
};
test('AUTH-CLIENT-01 actual WebCrypto P-256 signs the exact challenge, private key nonextractable',async()=>{
  const api=new AgentApi(origin);await api.pair('one-time-test-ticket','Home');assert.equal(api.token,'memory-only-access-token');
  const record=records.get(origin);assert.equal(record.keys.privateKey.extractable,false);assert.equal(record.keys.privateKey.type,'private');await assert.rejects(webcrypto.subtle.exportKey('pkcs8',record.keys.privateKey));
  assert.equal('token'in record,false);assert.equal('ticket'in record,false);assert.equal(JSON.stringify(record).includes('memory-only'),false);
});
test('AUTH-CLIENT-02 HTTP uses Authorization, not cookie or query secret',async()=>{const api=new AgentApi(origin);await api.authenticate();await api.request('/host');const c=calls.at(-1);assert.equal(c.options.headers.get('Authorization'),'Bearer memory-only-access-token');assert.equal(c.options.credentials,'omit');assert.equal(c.path,'/api/v1/host');});
test('AUTH-CLIENT-03 WS ticket is subprotocol scoped, no URL query',async()=>{let seen;globalThis.WebSocket=class{constructor(url,protocols){seen={url:new URL(url),protocols};}};const api=new AgentApi(origin);await api.authenticate();await api.websocket('terminal',randomUUID());assert.equal(seen.url.search,'');assert.equal(seen.url.protocol,'ws:');assert.deepEqual(seen.protocols,['rctm.v1','rc-ticket.single-use-scoped-ticket']);});
test('AUTH-CLIENT-04 unsafe agent addresses rejected',()=>{for(const url of['http://example.com','http://0.0.0.0:3847','https://a.test/path','https://user:secret@a.test','https://a.test/#token','file:///tmp'])assert.throws(()=>new AgentApi(url));assert.doesNotThrow(()=>new AgentApi('https://office.tail.ts.net'));});
test('AUTH-CLIENT-05 device key is audience-bound and missing origin needs pairing',async()=>{const api=new AgentApi('https://unpaired.example.test');await assert.rejects(api.authenticate(),/PAIRING_REQUIRED/);const record=records.get(origin);const before=record.audience;record.audience='https://wrong.example.test';await assert.rejects(new AgentApi(origin).authenticate(),/AUTH_AUDIENCE_MISMATCH/);record.audience=before;});
