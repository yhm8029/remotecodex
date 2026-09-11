<script lang="ts">
import { onMount } from 'svelte';
import { AgentApi } from '@remotecodex/terminal-client';
export let api: AgentApi;
export let currentClientId: string;
export let onSelfRevoked: () => void;

interface Device { client_id: string; label: string; scopes: string[]; revoked: boolean; fingerprint: string }

let devices: Device[] = [];
let busyId: string | null = null;
let loading = false;
let error: string | null = null;
let open = false;
let loadToken = 0;

async function load(): Promise<void> {
  if (loading || busyId) return;
  loading = true;
  error = null;
  const token = ++loadToken;
  try {
    const list = await api.request<Device[]>('/devices');
    if (token !== loadToken) return;
    devices = Array.isArray(list) ? list : [];
  } catch (e) {
    if (token !== loadToken) return;
    error = (e instanceof Error && e.message) ? e.message : '기기 관리 요청 실패';
    devices = [];
  } finally {
    if (token === loadToken) loading = false;
  }
}

function confirmRevoke(d: Device): boolean {
  const base = '기기 "' + d.label + '" 승인을 철회합니다.';
  const body = '기기의 원격 연결과 제어 권한을 철회합니다. 실행 중인 터미널 프로세스는 유지됩니다.';
  if (d.client_id === currentClientId) {
    return window.confirm(base + '\n현재 기기입니다. ' + body + '\n계속하면 연결이 종료됩니다.');
  }
  return window.confirm(base + '\n' + body);
}

async function revoke(d: Device): Promise<void> {
  if (d.revoked || busyId || loading) return;
  if (!confirmRevoke(d)) return;
  busyId = d.client_id;
  error = null;
  try {
    await api.request<void>('/devices/' + encodeURIComponent(d.client_id), undefined, 'DELETE');
    devices = devices.map(it => it.client_id === d.client_id ? { ...it, revoked: true } : it);
    if (d.client_id === currentClientId) { onSelfRevoked(); return; }
    busyId = null; await load();
  } catch (e) {
    error = (e instanceof Error && e.message) ? e.message : '기기 관리 요청 실패';
  } finally {
    busyId = null;
  }
}

onMount(load);
</script>

<section class="device-admin">
  <details bind:open>
    <summary>
      <span>등록 기기 관리</span>
      <button type="button" on:click={load} disabled={loading || busyId !== null}>새로고침</button>
    </summary>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if loading && devices.length === 0}<p class="muted">불러오는 중…</p>{/if}
    {#if !loading && devices.length === 0 && !error}<p class="muted">등록된 기기가 없습니다.</p>{/if}
    {#if devices.length > 0}
      <ul>
        {#each devices as d (d.client_id)}
          <li class:revoked={d.revoked}>
            <div class="row">
              <span class="label">{d.label}</span>
              {#if d.client_id === currentClientId}<span class="badge self">현재 기기</span>{/if}
              {#if d.revoked}<span class="badge revoked">철회됨</span>{/if}
            </div>
            <div class="meta">권한: {d.scopes.join(', ') || '없음'}</div>
            <div class="meta">지문: {d.fingerprint}</div>
            <button type="button" on:click={() => revoke(d)} disabled={d.revoked || busyId !== null || loading}>
              {busyId === d.client_id ? '처리 중…' : '기기 승인 철회'}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </details>
</section>

<style>
.device-admin { color: #b8c4d1; font-size: 0.9rem; }
.device-admin details { border-top: 1px solid #2a3543; padding: 0.5rem 0; }
.device-admin summary { cursor: pointer; display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
.device-admin summary button { background: #243140; color: #b8c4d1; border: 1px solid #3a4a5e; border-radius: 4px; padding: 0.2rem 0.5rem; }
.device-admin summary button:disabled { opacity: 0.5; cursor: not-allowed; }
.device-admin ul { list-style: none; padding: 0; margin: 0.5rem 0 0 0; }
.device-admin li { background: #1b2531; border: 1px solid #2a3543; border-radius: 4px; padding: 0.5rem; margin-bottom: 0.5rem; word-break: break-all; }
.device-admin li.revoked { opacity: 0.6; }
.device-admin .row { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
.device-admin .label { font-weight: 600; }
.device-admin .badge { font-size: 0.7rem; padding: 0.1rem 0.4rem; border-radius: 3px; }
.device-admin .badge.self { background: #2a4a6e; color: #b8c4d1; }
.device-admin .badge.revoked { background: #5e2a2a; color: #ffb8b8; }
.device-admin .meta { font-size: 0.78rem; color: #8a99aa; margin: 0.2rem 0; word-break: break-all; }
.device-admin button { background: #4a2a2a; color: #ffb8b8; border: 1px solid #6e3a3a; border-radius: 4px; padding: 0.3rem 0.6rem; margin-top: 0.3rem; }
.device-admin button:disabled { opacity: 0.5; cursor: not-allowed; }
.device-admin .err { color: #ffb8b8; margin: 0.4rem 0; }
.device-admin .muted { color: #8a99aa; margin: 0.4rem 0; }
</style>