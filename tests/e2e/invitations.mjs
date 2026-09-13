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
const mode=new URLSearchParams(location.search).get('mode');
const api={base:'http://127.0.0.1:3847',request:async(path,body)=>{window.calls.push({path,body});return {ticket:'a'.repeat(43),expires_in:300}}};
const scopes=['terminal.read','terminal.write','terminal.create','terminal.close','preview.read','admin.devices','admin.settings','desktop.control'];
const app=mount(mode==='host'?Host:Client,{target:document.body,props:mode==='host'?{api,scopes}:{native:true,oninvite:x=>window.imported=x,onselect:x=>window.selected=x,disabled:false}});
window.destroy=()=>unmount(app);
</script>`);
const mock=`export async function invoke(c,a){window.calls.push({command:c,args:a});if(c==='tailscale_status')return structuredClone(window.serve);if(c==='local_admin')return {public_origin:'https://office.ts.net'};throw Error('Unexpected command')}`;
const server=await createServer({root:dir,configFile:false,plugins:[{name:'mock-tauri',enforce:'pre',resolveId:id=>id==='@tauri-apps/api/core'?'\0mock-tauri':undefined,load:id=>id==='\0mock-tauri'?mock:undefined},svelte()],server:{host:'127.0.0.1',port:0,fs:{allow:[root,dir]}}});
await server.listen(); const browser=await playwright.chromium.launch({headless:true}); const results=[];
const base=`http://127.0.0.1:${server.httpServer.address().port}`;
try {
 const p=await browser.newPage();p.setDefaultTimeout(5000);await p.goto(base+'?mode=host');await verifyHost(p);results.push('host readiness and scoped ticket');await p.close();
 const expiry=await browser.newPage();expiry.setDefaultTimeout(5000);await expiry.clock.install();await expiry.goto(base+'?mode=host');await verifyExpiry(expiry);results.push('local QR clipboard and five-minute expiry without renewal');await expiry.close();
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
await p.clock.fastForward(301000);
assert.equal(await p.getByTestId('invite-qr').count(),0);
assert.ok((await p.getByTestId('invite-message').textContent()).includes('만료'));
assert.equal(await p.evaluate(()=>window.calls.filter(c=>c.path==='/pair-tickets').length),1);
}
