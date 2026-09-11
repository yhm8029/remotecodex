import test from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { encodeFrame, decodeFrames, Kind, StreamGuard, RenderBudget } from '../../.test-build/core/index.js';
const id='00000000-0000-4000-8000-000000000001';
const epoch='10000000-0000-4000-8000-000000000001';
const enc=new TextEncoder();
function frame(kind=1,payload=enc.encode('한글\x1b[31mRED'),sequence=1n){return {kind,sessionId:id,generation:1,sequence,payload};}
function metadata(n,sequence=0n){return frame(3,enc.encode(JSON.stringify({cols:120,rows:32,sequence:String(sequence),generation:1,agent_epoch:epoch,fidelity:'test',warnings:[],bytes:n})),sequence);}
test('WIRE-01 40-byte network-order header and UTF-8 survive round trip',()=>{const f=frame();const bytes=encodeFrame(f);assert.equal(bytes.length,40+f.payload.length);assert.deepEqual(decodeFrames(bytes)[0],f);});
test('WIRE-02 sequence beyond Number.MAX_SAFE_INTEGER stays exact BigInt',()=>{const f=frame(1,enc.encode('x'),(1n<<63n)+99n);assert.equal(decodeFrames(encodeFrame(f))[0].sequence,f.sequence);});
test('WIRE-03 decoder supports multiple complete records in one message',()=>{const a=encodeFrame(frame()),b=encodeFrame(frame(1,enc.encode('둘'),2n));assert.equal(decodeFrames(new Uint8Array([...a,...b])).length,2);});
for(const [name,mutate]of[
 ['magic',b=>b[0]=0],['version',b=>b[4]=2],['kind',b=>b[5]=9],['reserved_flags',b=>b[7]=1],
 ['length_overflow',b=>new DataView(b.buffer).setUint32(36,0xffffffff)]
])test(`WIRE-04 ${name} rejected`,()=>{const bytes=encodeFrame(frame());mutate(bytes);assert.throws(()=>decodeFrames(bytes));});
test('WIRE-05 every truncation boundary rejects, without partial success',()=>{const bytes=encodeFrame(frame());for(let n=0;n<bytes.length;n++)assert.throws(()=>decodeFrames(bytes.subarray(0,n)),`offset ${n}`);});
test('WIRE-06 byteOffset views are decoded without reading adjacent memory',()=>{const encoded=encodeFrame(frame());const wrapped=new Uint8Array(encoded.length+16);wrapped.set(encoded,7);assert.deepEqual(decodeFrames(wrapped.subarray(7,7+encoded.length))[0],frame());});
test('WIRE-07 maximum payload works; oversized payload and message reject',()=>{const f=frame(1,new Uint8Array(32768));assert.equal(decodeFrames(encodeFrame(f))[0].payload.length,32768);assert.throws(()=>encodeFrame(frame(1,new Uint8Array(32769))));assert.throws(()=>decodeFrames(new Uint8Array(262145)));});
test('WIRE-08 deterministic randomized round trips (1,000 records)',()=>{let seed=8191;for(let i=0;i<1000;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;const f=frame(1,Uint8Array.from({length:seed%512},(_,j)=>(j+seed)%256),BigInt(seed)<<24n);f.sessionId=randomUUID();assert.deepEqual(decodeFrames(encodeFrame(f))[0],f);}});
test('WIRE-09 Python/Rust/TypeScript common golden fixture',()=>{const vectors=JSON.parse(readFileSync(new URL('../fixtures/wire.json',import.meta.url)));for(const v of vectors){const f=decodeFrames(Buffer.from(v.hex,'hex'))[0];assert.equal(f.sessionId,v.session_id);assert.equal(f.sequence,BigInt(v.sequence));assert.equal(new TextDecoder().decode(f.payload),v.text);assert.equal(Buffer.from(encodeFrame(f)).toString('hex'),v.hex);}});
test('RESTORE-01 snapshot must finish before live output is accepted',()=>{const g=new StreamGuard(id,1,epoch);g.accept(metadata(3));g.accept(frame(4,enc.encode('abc'),0n));assert.equal(g.phase,'snapshot');g.accept(frame(5,new Uint8Array(),0n));assert.equal(g.phase,'live');g.accept(frame(1,enc.encode('next'),1n));assert.equal(g.sequence,1n);});
test('RESTORE-02 cross-session output rejected',()=>{const g=new StreamGuard(id,1,epoch);const f=metadata(0);f.sessionId=randomUUID();assert.throws(()=>g.accept(f));assert.equal(g.phase,'failed');});
test('RESTORE-03 missing chunk/end does not become live',()=>{const g=new StreamGuard(id,1,epoch);g.accept(metadata(10));assert.throws(()=>g.accept(frame(5,new Uint8Array(),0n)));assert.equal(g.phase,'failed');});
test('RESTORE-04 output gaps and duplicates require resync',()=>{for(const seq of[0n,2n]){const g=new StreamGuard(id,1,epoch);g.accept(metadata(0));g.accept(frame(5,new Uint8Array(),0n));assert.throws(()=>g.accept(frame(1,enc.encode('bad'),seq)));}});
test('RESTORE-05 restart epoch and changed generation reject restoration',()=>{assert.throws(()=>new StreamGuard(id,2,epoch).accept(metadata(0)));assert.throws(()=>new StreamGuard(id,1,randomUUID()).accept(metadata(0)));});
test('RESTORE-06 oversize snapshot refuses allocation',()=>{const g=new StreamGuard(id,1,epoch);assert.throws(()=>g.accept(metadata(4194305)));});
test('FLOW-01 render credit is bounded, rejects forged negative acknowledgement',()=>{const b=new RenderBudget(100);b.reserve(90);assert.throws(()=>b.reserve(11));assert.equal(b.pending,90);assert.throws(()=>b.applied(91));assert.throws(()=>b.applied(-1));b.applied(90);assert.equal(b.pending,0);});
test('FLOW-02 slow peer cannot silently discard bytes and continue',()=>{const b=new RenderBudget(50);b.reserve(50);assert.throws(()=>b.reserve(1),/RESYNC_REQUIRED/);assert.equal(b.pending,50);});
