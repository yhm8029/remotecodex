import {spawnSync}from'node:child_process';import{readdir,readFile,writeFile}from'node:fs/promises';import path from'node:path';
const result=spawnSync(process.platform==='win32'?'tsc.cmd':'tsc',['-p','tsconfig.client-portable.json'],{stdio:'inherit',shell:process.platform==='win32'});
if(result.status!==0)process.exit(result.status??1);
// Only test output is rewritten: Node otherwise cannot resolve workspace .ts exports without a loader.
const root=path.resolve('.test-build/client');const target=path.join(root,'packages/core/src/index.js');
async function walk(dir){for(const ent of await readdir(dir,{withFileTypes:true})){const full=path.join(dir,ent.name);if(ent.isDirectory())await walk(full);else if(ent.name.endsWith('.js')){let rel=path.relative(path.dirname(full),target).replaceAll('\\','/');if(!rel.startsWith('.'))rel='./'+rel;const text=await readFile(full,'utf8');await writeFile(full,text.replaceAll("'@remotecodex/core'",JSON.stringify(rel)).replaceAll('"@remotecodex/core"',JSON.stringify(rel)));}}}await walk(root);
