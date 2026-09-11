/** Real Windows host E2E, deliberately NOT part of the portable npm test pass.
 * Run only against an Agent you already started. Does not start Codex, alter projects,
 * touch other PTYs, publish ports or save credentials. Creates/revokes its test device.
 */
import assert from 'node:assert/strict';
import { parseArgs, promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { randomUUID, webcrypto } from 'node:crypto';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { writeFile, mkdir } from 'node:fs/promises';
import WebSocket from 'ws';
import headless from '@xterm/headless';
import { decodeFrames, Kind, StreamGuard } from '../../.test-build/core/index.js';
const {Terminal}=headless;
const {values}=parseArgs({options:{agent:{type:'string',default:'target/debug/rc-agent.exe'},origin:{type:'string',default:'http://127.0.0.1:3847'}}});
if(process.platform!=='win32')throw new Error('NOT_RUN: production Agent E2E requires Windows. Portable fixtures are not a substitute.');
const base=new URL(values.origin);if(base.origin!==values.origin||(base.protocol!=='https:'&&base.origin!=='http://127.0.0.1:3847'))throw new Error('Use an exact approved HTTPS origin or http://127.0.0.1:3847');
const report={started:new Date().toISOString(),environment:{node:process.version,platform:process.platform},checks:[],cleanup:[]};
const made=[],streams=[];let token='',device,control,renewal;const leases=new Map();let sequence=0;
async function api(path,body,method,auth=true,origin=base.origin){
 const h={'Origin':origin,...(body===undefined?{}:{'Content-Type':'application/json'}),...(auth?{Authorization:`Bearer ${token}`}:{})};
 const r=await fetch(`${base.origin}/api/v1${path}`,{method:method??(body===undefined?'GET':'POST'),headers:h,...(body===undefined?{}:{body:JSON.stringify(body)}),signal:AbortSignal.timeout(12000)});
 if(!r.ok){const e=new Error(`HTTP_${r.status}`);e.status=r.status;throw e;}return r.status===204?null:r.json();
}
function waitUntil(f,timeout=10000){const end=Date.now()+timeout;return new Promise((resolve,reject)=>{const tick=()=>{try{const v=f();if(v){resolve(v);return;}if(Date.now()>end){reject(new Error('Timeout waiting for real host result'));return;}setTimeout(tick,20);}catch(e){reject(e);}};tick();});}
async function socket(channel,id){const t=await api('/tickets/ws',{channel,session_id:id??null});const url=new URL(`/api/v1/ws/${channel}`,base);url.protocol=base.protocol==='https:'?'wss:':'ws:';const ws=new WebSocket(url,['rctm.v1',`rc-ticket.${t.ticket}`],{origin:base.origin,perMessageDeflate:false,maxPayload:256*1024});return ws;}
async function controlSocket(){const ws=await socket('control');const messages=[];let failure;ws.on('message',b=>{messages.push(JSON.parse(b.toString()));if(messages.length>1024)messages.shift();});ws.on('error',e=>failure=e);const hello=await waitUntil(()=>{if(failure)throw failure;return messages.find(m=>m.type==='hello');});return{ws,hello,messages};}
async function rpc(c,m,kind){const request_id=randomUUID();c.ws.send(JSON.stringify({...m,request_id}));return waitUntil(()=>c.messages.find(x=>x.request_id===request_id&&(x.type===kind||x.type==='rejected'||x.type==='delivery_failed'||x.type==='delivery_unknown')));}
async function terminal(session){
 const ws=await socket('terminal',session.session_id);const term=new Terminal({cols:session.cols,rows:session.rows,scrollback:2000});const guard=new StreamGuard(session.session_id,session.generation,session.agent_epoch);let ready=false,error,queue=Promise.resolve();
 ws.on('error',e=>error=e);ws.on('message',(b,binary)=>{queue=queue.then(async()=>{assert.ok(binary);for(const frame of decodeFrames(new Uint8Array(b))){guard.accept(frame);if(frame.kind===Kind.SnapshotMeta){term.reset();term.resize(guard.meta.cols,guard.meta.rows);}else if(frame.kind===Kind.Output||frame.kind===Kind.SnapshotChunk){await new Promise(r=>term.write(frame.payload,r));}else if(frame.kind===Kind.Resize){const d=new DataView(frame.payload.buffer,frame.payload.byteOffset,4);term.resize(d.getUint16(0),d.getUint16(2));}else if(frame.kind===Kind.SnapshotEnd)ready=true;if(guard.phase==='live'&&ws.readyState===1)ws.send(JSON.stringify({type:'applied',sequence:guard.sequence.toString(),bytes:frame.payload.length+40}));}}).catch(e=>{error=e;ws.close();});});
 const t={ws,term,get ready(){if(error)throw error;return ready;},text(){if(error)throw error;const b=term.buffer.active;return Array.from({length:b.length},(_,i)=>b.getLine(i)?.translateToString(true)??'').join('\n');},close(){ws.close();term.dispose();}};streams.push(t);await waitUntil(()=>t.ready);return t;
}
async function check(name,fn){await fn();report.checks.push({name,status:'PASS'});console.log(`PASS ${name}`);}
async function input(session,text,id=randomUUID(),seq=++sequence){const l=leases.get(session.session_id);return rpc(control,{type:'input',session_id:session.session_id,agent_epoch:session.agent_epoch,generation:session.generation,lease_epoch:l.epoch,input_id:id,input_seq:seq,payload:{kind:'utf8',text}},'written');}
let failure;
try{
 await check('same-user CLI pairing and P-256 authentication',async()=>{
  const{stdout}=await promisify(execFile)(resolve(values.agent),['pair'],{timeout:5000,windowsHide:false,maxBuffer:16384});const ticket=JSON.parse(stdout).ticket;assert.equal(typeof ticket,'string');
  const keys=await webcrypto.subtle.generateKey({name:'ECDSA',namedCurve:'P-256'},false,['sign','verify']);const pub=Buffer.from(await webcrypto.subtle.exportKey('raw',keys.publicKey)).toString('base64url');
  device=await api('/auth/pair',{ticket,public_key:pub,label:`E2E-${randomUUID()}`},'POST',false);
  const c=await api('/auth/challenge',{client_id:device.client_id},'POST',false);assert.equal(c.audience,device.audience);assert.ok(c.message.startsWith(`RemoteCodex/v1
${device.audience}
${device.client_id}
`));
  const sig=Buffer.from(await webcrypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},keys.privateKey,new TextEncoder().encode(c.message))).toString('base64url');token=(await api('/auth/verify',{challenge_id:c.challenge_id,signature:sig},'POST',false)).access_token;
 });
 await check('unapproved Origin rejected even with authenticated bearer',async()=>{await assert.rejects(api('/sessions',undefined,'GET',true,'https://unapproved.invalid'),e=>e.status===403);});
 control=await controlSocket();assert.equal(control.hello.client_id,device.client_id);
 await check('create two isolated real CMD PTYs',async()=>{for(let i=0;i<2;i++){made.push(await api('/sessions',{label:`E2E-CMD-${i}-${randomUUID()}`,project_id:null,cwd:tmpdir(),profile:'cmd',cols:100,rows:24}));}assert.notEqual(made[0].session_id,made[1].session_id);assert.notEqual(made[0].pid,made[1].pid);});
 for(const s of made){const r=await rpc(control,{type:'lease_acquire',session_id:s.session_id,takeover:false},'lease');assert.equal(r.type,'lease');leases.set(s.session_id,r.lease);}
 renewal=setInterval(()=>{for(const [id,l]of leases)if(control?.ws.readyState===1)control.ws.send(JSON.stringify({type:'lease_renew',request_id:randomUUID(),session_id:id,lease_epoch:l.epoch}));},5000);
 const a=await terminal(made[0]),b=await terminal(made[1]);const nonce=randomUUID().replaceAll('-','');const marker=`RC_${nonce}_RESULT`;
 await check('selected PTY receives real output, not echoed command text',async()=>{const r=await input(made[0],`@set "RC_E2E=RC_${nonce}"\r\n@echo %RC_E2E%_RESULT\r\n`);assert.equal(r.type,'written');await waitUntil(()=>a.text().includes(marker));assert.equal(b.text().includes(marker),false);});
 await check('second writer connection cannot interleave without taking lease',async()=>{const other=await controlSocket();try{const r=await rpc(other,{type:'lease_acquire',session_id:made[0].session_id,takeover:false},'lease');assert.equal(r.type,'rejected');}finally{other.ws.close();}});
 await check('duplicate input identifier cannot execute twice',async()=>{const id=randomUUID();const first=await input(made[1],'@echo safe-e2e\r\n',id);assert.equal(first.type,'written');const next=await input(made[1],'@echo must-not-execute\r\n',id);assert.equal(next.type,'rejected');});
 await check('closing terminal view keeps PID and snapshot reconnect restores output',async()=>{a.close();const list=await api('/sessions');assert.equal(list.find(s=>s.session_id===made[0].session_id).pid,made[0].pid);const restored=await terminal(made[0]);await waitUntil(()=>restored.text().includes(marker));});
 await check('resize affects only chosen PTY',async()=>{const s=made[0];const r=await rpc(control,{type:'resize',session_id:s.session_id,agent_epoch:s.agent_epoch,generation:s.generation,lease_epoch:leases.get(s.session_id).epoch,cols:110,rows:26},'resized');assert.equal(r.type,'resized');const list=await api('/sessions');assert.equal(list.find(x=>x.session_id===s.session_id).cols,110);assert.equal(list.find(x=>x.session_id===made[1].session_id).cols,100);});
 await check('control disconnect leaves both processes running',async()=>{clearInterval(renewal);renewal=undefined;control.ws.close();await waitUntil(()=>control.ws.readyState===3);const list=await api('/sessions');for(const s of made)assert.equal(list.find(x=>x.session_id===s.session_id).state,'running');});
}catch(e){failure=e;report.checks.push({name:'run',status:'FAIL',error:e.message});}
finally{
 clearInterval(renewal);for(const s of streams){try{s.close();}catch{}}control?.ws.close();
 for(const s of made){try{await api(`/sessions/${s.session_id}`,undefined,'DELETE');report.cleanup.push({type:'test_session',status:'removed'});}catch(e){report.cleanup.push({type:'test_session',id:s.session_id,status:'FAILED',error:e.message});failure??=e;}}
 if(device&&token){try{await api(`/devices/${device.client_id}`,undefined,'DELETE');report.cleanup.push({type:'test_device',status:'revoked'});}catch(e){report.cleanup.push({type:'test_device',id:device.client_id,status:'FAILED',error:e.message});failure??=e;}}
 else if(device){report.cleanup.push({type:'test_device',id:device.client_id,status:'MANUAL_REVOKE_REQUIRED',error:'Pairing succeeded but no access token was obtained. Revoke this test device from the company local admin UI.'});failure??=new Error('Test device cleanup requires local approval/revocation.');}
 token='';report.finished=new Date().toISOString();report.overall=failure?'FAIL':'PASS';
 await mkdir('docs/test-results/local-e2e',{recursive:true});await writeFile('docs/test-results/local-e2e/windows-agent.json',JSON.stringify(report,null,2));
}
if(failure){console.error(`E2E FAILED: ${failure.message}. See the sanitized report; do not count an incomplete cleanup as PASS.`);process.exitCode=1;}
else console.log('Real host E2E passed. This does not certify GUI, mobile Safari, network performance or 24h soak.');
