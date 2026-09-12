<script lang="ts">
  import { onMount } from 'svelte';
  import TailscaleSetup from './TailscaleSetup.svelte';
  type Status = 'unknown' | 'disabled' | 'enabled' | 'conflict';
  let status: Status = 'unknown', consent = false, busy = false, error = '';
  type Serve = { installed: boolean; ownership: 'absent' | 'owned' | 'conflict'; detail: string; dns_name?: string; public_origin?: string; restart_required?: boolean; preview_ports?: Record<string, number> };
  let serve: Serve | null = null, serveConsent = false;
  $: statusText = status === 'enabled' ? '자동 시작 사용 중' : status === 'disabled' ? '자동 시작 꺼짐' : status === 'conflict' ? '다른 자동 시작 값과 충돌' : '자동 시작 상태 확인 중…';
  async function refresh() {
    busy = true; error = '';
    try { const { invoke } = await import('@tauri-apps/api/core'); const result = await invoke<{status: Status}>('autostart_status'); status = result.status; }
    catch (e) { status = 'unknown'; error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
  async function setEnabled(enabled: boolean) {
    if (busy || status === 'conflict' || (enabled && !consent)) return;
    busy = true; error = '';
    try { const { invoke } = await import('@tauri-apps/api/core'); const result = await invoke<{status: Status}>('set_autostart', { enabled }); status = result.status; }
    catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
  async function refreshServe() { try { const { invoke } = await import('@tauri-apps/api/core'); serve = await invoke<Serve>('tailscale_status'); } catch (e) { error = e instanceof Error ? e.message : String(e); } }
  async function setServe(enabled: boolean) { if (busy || (enabled && !serveConsent) || serve?.ownership === 'conflict') return; busy=true;error='';try{const{invoke}=await import('@tauri-apps/api/core');serve=await invoke<Serve>('set_tailscale_serve',{request:{enabled,consent:enabled ? serveConsent : true}});}catch(e){error=e instanceof Error?e.message:String(e);}finally{busy=false;} }
  onMount(() => { void refresh(); void refreshServe(); });
</script>

<section class="host-settings" aria-label="호스트 설정">
  <h2>호스트 설정</h2>
  <p role="status">{statusText}</p>
  <p class="hint">현재 사용자 로그인 후 Agent를 시작합니다. 로그인 전 시스템 서비스로 등록하지 않으며 UI를 닫아도 Agent와 PTY는 유지됩니다.</p>
  {#if status === 'conflict'}<p class="warning" role="alert">다른 Run 값이 있습니다. RemoteCodex는 이 값을 덮어쓰거나 삭제하지 않습니다.</p>{/if}
  <label><input type="checkbox" bind:checked={consent} disabled={busy || status === 'conflict'}> Windows 로그인 시 Agent 시작에 동의합니다.</label>
  <div class="actions">
    <button on:click={() => setEnabled(true)} disabled={busy || !consent || status === 'conflict'}>자동 시작 켜기</button>
    <button on:click={() => setEnabled(false)} disabled={busy || status === 'conflict'}>자동 시작 끄기</button>
    <button on:click={refresh} disabled={busy}>새로 고침</button>
  </div>
  <TailscaleSetup native={true} role="host" canConfigureHost={true} />
  <details><summary>기존 원격 연결 설정 관리</summary>
  <button on:click={refreshServe} disabled={busy}>설정 새로 고침</button>
  {#if serve && !serve.installed}<p class="warning" role="status">Tailscale CLI가 설치되어 있지 않습니다. Tailscale은 별도 구성요소입니다.</p>
  {:else if serve}<p role="status">HTTPS 443: {serve.ownership} · {serve.detail}</p>{/if}
  {#if serve?.public_origin}<p><label>복사할 회사 주소<input readonly value={serve.public_origin} on:focus={(event) => event.currentTarget.select()}></label></p>{/if}
  {#if serve?.restart_required}<p class="warning" role="status">Agent 설정이 바뀌어 재시작이 필요합니다. 실행 중인 PTY는 중지하지 않습니다.</p>{/if}
  {#if serve?.ownership === 'conflict'}<p class="warning" role="alert">기존 443 설정을 보존했습니다. RemoteCodex가 변경하거나 제거하지 않습니다.</p>{/if}
  {#if serve?.ownership === 'owned'}<div class="actions"><button on:click={() => setServe(false)} disabled={busy}>RemoteCodex Serve 제거</button><button on:click={refreshServe} disabled={busy}>Serve 새로 고침</button></div>{/if}
  </details>
  {#if error}<p class="warning" role="alert">{error}</p>{/if}
</section>

<style>
  .host-settings{margin:12px 14px;padding:14px;border:1px solid #273447;border-radius:9px;background:#131c28}.host-settings h2{font-size:15px;margin:0 0 8px}.host-settings p{margin:6px 0}.hint{font-size:11px;color:#92a3b8}.warning{color:#ddc392}.host-settings label{flex-direction:row;align-items:center;margin:12px 0}.actions{display:flex;gap:8px;flex-wrap:wrap}
</style>
