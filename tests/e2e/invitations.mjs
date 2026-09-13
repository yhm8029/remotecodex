import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createServer } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
const {default: playwright}=await import(process.env.RC_PLAYWRIGHT_MODULE);
const root=process.cwd(), dir=await fs.mkdtemp(path.join(root,'runtime','invite-ui-'));
const host=path.join(root,'apps/web/src/HostInvite.svelte').replaceAll('\\','/');
const client=path.join(root,'apps/web/src/InvitationConnect.svelte').replaceAll('\\','/');
await fs.writeFile(path.join(dir,'index.html'),`<script type="module">
import {mount,unmount} from 'svelte';
import Host from '/@fs/${host}'; import Client from '/@fs/${client}';
window.calls=[]; window.serve={ownership:'owned',https_ready:true,restart_required:false,public_origin:'https://office.ts.net'};
window.inviteState={pending:true,transient:false,unauthOnce:false,authCalls:0,statusGate:false,resolveStatus:null};
const mode=new URLSearchParams(location.search).get('mode');
const api={base:'http://127.0.0.1:3847',authenticate:async()=>{window.inviteState.authCalls++;},request:async(path,body)=>{window.calls.push({path,body});if(path==='/pair-tickets')return {ticket:'a'.repeat(43),expires_in:18000};if(path==='/pair-tickets/status'){if(window.inviteState.statusGate)return await new Promise((resolve,reject)=>{window.inviteState.resolveStatus=()=>resolve({pending:false});window.inviteState.rejectStatus=()=>reject({code:"UNAUTHENTICATED"});});if(window.inviteState.unauthOnce){window.inviteState.unauthOnce=false;throw {code:'UNAUTHENTICATED'};}if(window.inviteState.transient)throw new Error('temporary');return {pending:window.inviteState.pending};}throw new Error('unexpected path')}};
const scopes=['terminal.read','terminal.write','terminal.create','terminal.close','preview.read','admin.devices','admin.settings','desktop.control'];
const app=mount(mode==='host'?Host:Client,{target:document.body,props:mode==='host'?{api,scopes}:{native:true,oninvite:x=>window.imported=x,onselect:x=>window.selected=x,disabled:false}});
window.destroy=()=>unmount(app);
</script>`);
const mock=`export async function invoke(c,a){window.calls.push({command:c,args:a});if(c==='tailscale_status')return structuredClone(window.serve);if(c==='local_admin')return {public_origin:'https://office.ts.net'};throw Error('Unexpected command')}`;
const server=await createServer({root:dir,configFile:false,plugins:[{name:'mock-tauri',enforce:'pre',resolveId:id=>id==='@tauri-apps/api/core'?'\0mock-tauri':undefined,load:id=>id==='\0mock-tauri'?mock:undefined},svelte()],server:{host:'127.0.0.1',port:0,fs:{allow:[root,dir]}}});
await server.listen(); const browser=await playwright.chromium.launch({headless:true}); const results=[];
const base=`http://127.0.0.1:${server.httpServer.address().port}`;
try {
 const p=await browser.newPage();p.setDefaultTimeout(5000);await p.goto(base+'?mode=host');await verifyHost(p);results.push('host readiness and scoped five-hour ticket');await p.close();
 const expiry=await browser.newPage();expiry.setDefaultTimeout(5000);await expiry.clock.install();await expiry.goto(base+'?mode=host');await verifyExpiry(expiry);results.push('local QR clipboard and five-hour expiry without renewal');await expiry.close();
 const status=await browser.newPage();status.setDefaultTimeout(5000);await status.clock.install();await status.goto(base+'?mode=host');await verifyStatus(status);results.push('one-use status polling and transient retention');await status.close();
 const disposed=await browser.newPage();disposed.setDefaultTimeout(5000);await disposed.clock.install();await disposed.goto(base+'?mode=host');await disposed.getByTestId('invite-create').click();await disposed.getByTestId('invite-qr').waitFor();const statusCalls=await disposed.evaluate(()=>window.calls.filter(c=>c.path==='/pair-tickets/status').length);await disposed.evaluate(()=>window.destroy());await disposed.clock.fastForward(6000);assert.equal(await disposed.evaluate(()=>window.calls.filter(c=>c.path==='/pair-tickets/status').length),statusCalls);results.push('dispose stops status polling');await disposed.close();
 const authDisposed=await browser.newPage();authDisposed.setDefaultTimeout(5000);await authDisposed.clock.install();await authDisposed.goto(base+'?mode=host');
 await authDisposed.getByTestId('invite-create').click();await authDisposed.getByTestId('invite-qr').waitFor();
 await authDisposed.evaluate(()=>{window.inviteState.statusGate=true;});await authDisposed.clock.fastForward(3000);
 await authDisposed.waitForFunction(()=>typeof window.inviteState.rejectStatus==='function');
 await authDisposed.evaluate(()=>window.destroy());await authDisposed.evaluate(()=>window.inviteState.rejectStatus());
 assert.equal(await authDisposed.evaluate(()=>window.inviteState.authCalls),0);results.push('disposed unauthenticated response never triggers fresh authentication');await authDisposed.close();
 const c=await browser.newPage();c.setDefaultTimeout(5000);await c.goto(base+'?mode=client');
 const value={v:1,origin:'https://office.ts.net',label:'Office',ticket:'a'.repeat(43)};
 const link=value.origin+'/#rc-invite='+Buffer.from(JSON.stringify(value)).toString('base64url');
 await c.getByTestId('invite-paste').fill(link);await c.getByTestId('invite-import').click();await c.waitForFunction(()=>!!window.imported);
 assert.equal(await c.evaluate(()=>window.imported.origin),value.origin);assert.equal(await c.evaluate(()=>window.calls.length),0);
 results.push('client imports without network');await c.close();
} finally {await browser.close();await server.close();await fs.rm(dir,{recursive:true,force:true});}
await fs.writeFile('runtime/invite-20260914/ui.json',JSON.stringify({status:'PASS',groups:results},null,2));console.log(JSON.stringify(results));

async function verifyHost(p) {
  await p.getByTestId('invite-create').click();
  await p.getByTestId('invite-qr').waitFor();
  let pairs = await p.evaluate(() => window.calls.filter(c => c.path === '/pair-tickets'));
  assert.equal(pairs.length, 1);
  assert.equal(pairs[0].body.expires_in, 18000);
  assert.equal(await p.evaluate(() => location.href.includes('a'.repeat(43))), false);
  const inviteLink = await p.getByRole('textbox', { name: '초대 링크', exact: true }).inputValue();
  assert.equal(inviteLink.includes('ticket='), false);
  assert.ok(pairs[0].body.scopes.includes('terminal.write'));
  assert.ok(!pairs[0].body.scopes.includes('desktop.control'));
  await p.evaluate(() => { window.serve.https_ready = false; });
  await p.getByTestId('invite-create').click();
  await p.getByRole('alert').waitFor();
  pairs = await p.evaluate(() => window.calls.filter(c => c.path === '/pair-tickets'));
  assert.equal(pairs.length, 1);
}

async function verifyExpiry(p){
await p.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { configurable:true, value:{writeText: async (text) => { window.copiedInvite=text; }} }); });
await p.getByTestId('invite-create').click();
await p.getByTestId('invite-qr').waitFor();
assert.ok((await p.getByTestId('invite-qr').getAttribute('src')).startsWith('data:image/png;base64,'));
const link=await p.getByRole('textbox',{name:'초대 링크',exact:true}).inputValue();
assert.ok(link.startsWith('https://office.ts.net/#rc-invite='));
await p.getByTestId('invite-copy').click();
assert.equal(await p.evaluate(()=>window.copiedInvite),link);
assert.match(await p.getByTestId('invite-remaining').textContent(), /[0-9]+시간/);
await p.clock.fastForward(18000001);
assert.equal(await p.getByTestId('invite-qr').count(),0);
assert.ok((await p.getByTestId('invite-message').textContent()).includes('만료'));
assert.equal(await p.evaluate(()=>window.calls.filter(c=>c.path==='/pair-tickets').length),1);
}

async function verifyStatus(p){
  await p.getByTestId('invite-create').click();
  await p.getByTestId('invite-qr').waitFor();
  await p.evaluate(() => { window.inviteState.transient = true; });
  await p.clock.fastForward(3000);
  assert.equal(await p.getByTestId('invite-qr').count(), 1);
  await p.evaluate(() => { window.inviteState.transient = false; window.inviteState.unauthOnce = true; });
  await p.clock.fastForward(3000);
  assert.equal(await p.evaluate(() => window.inviteState.authCalls), 1);
  await p.evaluate(() => { window.inviteState.statusGate = true; });
  await p.clock.fastForward(3000);
  await p.getByTestId('invite-create').click();
  await p.getByTestId('invite-qr').waitFor();
  await p.evaluate(() => { window.inviteState.statusGate = false; window.inviteState.resolveStatus?.(); });
  await p.waitForTimeout(0);
  assert.equal(await p.getByTestId('invite-qr').count(), 1);
  await p.evaluate(() => { window.inviteState.pending = false; });
  await p.clock.fastForward(3000);
  assert.equal(await p.getByTestId('invite-qr').count(), 0);
  assert.match(await p.getByTestId('invite-message').textContent(), /등록|만료/);
  assert.equal(await p.evaluate(() => window.calls.filter(c => c.path === '/pair-tickets').length), 2);
}
