import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

const source = await fs.readFile('apps/web/src/App.svelte', 'utf8');
assert.match(source, /<details class="host-management" open>/, 'connected native hosts must expose Tailscale setup without an extra disclosure click');
console.log('PASS host settings default open');
