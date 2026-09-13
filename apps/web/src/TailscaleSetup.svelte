<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  export let native = false;
  export let role: 'host' | 'client' = 'client';
  export let canConfigureHost = false;
  type Setup = { state: string; detail: string };
  type Serve = { ownership: 'absent' | 'owned' | 'conflict'; public_origin?: string; restart_required?: boolean; https_ready?: boolean | null; detail: string };
  let open = false, busy = false, installing = false, disposed = false;
  let setup: Setup | null = null, serve: Serve | null = null;
  let error = '', message = '', consent = false, installBlocked = false, rebootRequired = false, bootstrapAttempted = false, retryAvailable = false;
  let epoch = 0, poll = 0, timer: ReturnType<typeof setTimeout> | undefined;
  const current = (id: number) => !disposed && open && id === epoch;
  function stopPoll() { poll++; clearTimeout(timer); timer = undefined; }
  async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (!native) throw new Error('Windows 앱에서만 사용할 수 있습니다.');
    return (await import('@tauri-apps/api/core')).invoke<T>(command, args);
  }
  const labels: Record<string, string> = {
    not_installed: '설치 필요', installing: '설치 중', login_required: '로그인 필요',
    connecting: '연결 확인 중', connected: '이 PC의 Tailscale 연결됨',
    cli_unavailable: 'Tailscale 실행 확인 필요', service_unavailable: 'Tailscale 서비스 확인 필요'
  };
  async function refresh(id = epoch) {
    if (!native || !current(id) || busy) return;
    let bootstrap = false;
    busy = true; error = ''; setup = null; serve = null;
    try {
      const next = await invoke<Setup>('tailscale_setup_status');
      if (!current(id)) return;
      setup = next; installBlocked = next.state === 'installing';
      bootstrap = next.state === 'not_installed' && !bootstrapAttempted && !installBlocked && !rebootRequired;
      if (role === 'host' && canConfigureHost && next.state === 'connected' && !rebootRequired) {
        try {
          const nextServe = await invoke<Serve>('tailscale_status');
          if (!current(id)) return;
          serve = nextServe;
        } catch (e) {
          if (current(id)) { serve = null; error = String(e); }
        }
      }
    } catch (e) { if (current(id)) { setup = null; serve = null; error = String(e); } }
    finally {
      if (current(id)) {
        busy = false;
        if (bootstrap) void install();
      }
    }
  }
  async function install() {
    if (!native || busy || !open || disposed || installBlocked || rebootRequired || setup?.state !== 'not_installed') return;
    bootstrapAttempted = true;
    retryAvailable = false;
    stopPoll(); const id = ++epoch; let installCode: string | null = null; busy = true; installing = true; message = ''; error = '';
    try {
      const result = await invoke<{ code: string }>('tailscale_setup_install');
      installCode = result.code;
      if (!current(id)) return;
      if (result.code === 'installed' || result.code === 'already_installed') {
        busy = false; installing = false; await refresh(id);
      } else if (result.code === 'reboot_required') {
        rebootRequired = true; serve = null; message = '설치가 완료되었습니다. Windows를 재부팅한 뒤 앱을 다시 여세요.';
      } else if (result.code === 'timeout') {
        installBlocked = true; setup = { state: 'installing', detail: 'Waiting for the existing Windows installer.' };
        message = '설치 창이 아직 열려 있습니다. 설치를 마친 뒤 새로 고침하세요. 상태가 풀리지 않으면 설치 창을 닫고 RemoteCodex를 다시 여세요.';
      } else if (result.code === 'cancelled') message = '설치가 취소되었습니다. 다시 시도할 수 있습니다.';
      else if (result.code === 'approval_denied') message = 'Windows 관리자 승인이 거부되었습니다. 승인 가능한 계정으로 다시 시도하세요.';
      else { message = '설치를 완료하지 못했습니다. 새로 고침 후 다시 시도하세요.'; error = result.code; }
    } catch (e) { if (current(id)) { installCode = 'invoke_failed'; message = '설치를 시작하지 못했습니다.'; error = String(e); } }
    finally { if (current(id)) { retryAvailable = !!installCode && !['installed', 'already_installed', 'reboot_required', 'timeout'].includes(installCode); busy = false; installing = false; } }
  }
  async function login() {
    if (!native || busy || !open || disposed || rebootRequired || !['login_required', 'connecting'].includes(setup?.state ?? '')) return;
    stopPoll(); const id = ++epoch; busy = true; message = ''; error = '';
    try {
      await invoke('tailscale_setup_login');
      if (!current(id)) return;
      message = 'Tailscale 트레이 앱에서 Log in 또는 Connect를 누르고, 열린 브라우저에서 계정 로그인을 완료하세요.';
      busy = false;
      const generation = ++poll, deadline = Date.now() + 120000;
      const tick = async () => {
        if (!current(id) || generation !== poll) return;
        if (Date.now() >= deadline) { message = '연결 확인 시간이 지났습니다. 로그인 상태를 확인한 뒤 새로 고침하세요.'; return; }
        if (!busy) await refresh(id);
        if (!current(id) || generation !== poll) return;
        if (error || ['connected', 'cli_unavailable', 'service_unavailable', 'not_installed'].includes(setup?.state ?? '')) return;
        timer = setTimeout(tick, 3000);
      };
      timer = setTimeout(tick, 3000);
    } catch (e) { if (current(id)) { message = 'Tailscale 로그인 창을 열지 못했습니다.'; error = String(e); } }
    finally { if (current(id)) busy = false; }
  }
  async function enable() {
    if (!native || busy || !open || disposed || !consent || rebootRequired || role !== 'host' || !canConfigureHost || setup?.state !== 'connected' || serve?.ownership !== 'absent' || serve?.https_ready === false) return;
    stopPoll(); const id = ++epoch; busy = true; error = ''; message = '';
    try {
      const next = await invoke<Serve>('set_tailscale_serve', { request: { enabled: true, consent: true } });
      if (current(id)) serve = next;
    } catch (e) { if (current(id)) { serve = null; message = '원격 연결 설정을 완료하지 못했습니다.'; error = String(e); } }
    finally { if (current(id)) busy = false; }
  }
  async function openHttpsSettings() {
    if (!native || busy || !open || disposed || rebootRequired || role !== 'host' || !canConfigureHost || serve?.https_ready !== false) return;
    const id = epoch;
    busy = true; error = '';
    try {
      await invoke('tailscale_https_settings');
      if (current(id)) message = 'Tailscale HTTPS 인증서를 먼저 활성화한 뒤 새로 고침을 눌러 주세요.';
    } catch (e) { if (current(id)) error = String(e); }
    finally { if (current(id)) busy = false; }
  }
  function openPanel() { if (!open && !disposed) { open = true; busy = false; bootstrapAttempted = false; retryAvailable = false; epoch++; void refresh(epoch); } }
  function closePanel() { if (!busy) { stopPoll(); epoch++; open = false; } }
  function manual() { if (!busy) { stopPoll(); epoch++; if (!rebootRequired) message = ''; void refresh(epoch); } }
  function retryBootstrap() { if (!busy && retryAvailable && !rebootRequired && setup?.state === 'not_installed') { bootstrapAttempted = false; void install(); } }
  onMount(() => { if (native) openPanel(); });
  onDestroy(() => { disposed = true; epoch++; stopPoll(); });
  $: host = role === 'host' && setup?.state === 'connected';
  $: ready = host && canConfigureHost && serve?.ownership === 'owned' && !!serve.public_origin && !serve.restart_required && serve?.https_ready !== false && !rebootRequired && !busy && !error;
</script>

{#if !open}
  <button type="button" on:click={openPanel} data-testid="open">원격 연결 준비하기</button>
{:else}
  <section class="setup" aria-label="원격 연결 준비">
    <header><h3>원격 연결 준비</h3><button type="button" on:click={closePanel} disabled={busy} data-testid="close">닫기</button></header>
    {#if !native}
      <p>이 기기에 Tailscale을 설치하고 호스트 PC와 같은 네트워크에 로그인하세요.</p>
      <nav aria-label="Tailscale 설치 안내">
        <a href="https://tailscale.com/docs/install/windows" target="_blank" rel="noopener">Windows 설치 안내</a>
        <a href="https://tailscale.com/docs/install/ios" target="_blank" rel="noopener">iOS 설치 안내</a>
        <a href="https://tailscale.com/docs/install/android" target="_blank" rel="noopener">Android 설치 안내</a>
        <a href="https://tailscale.com/download" target="_blank" rel="noopener">다른 플랫폼</a>
      </nav>
    {:else}
      <p role="status" data-testid="state">{installing ? '설치 중 — Windows 설치 창을 확인하세요.' : labels[setup?.state ?? ''] ?? '설치 확인 중'}</p>
      {#if message}<p role="status" data-testid="message">{message}</p>{/if}
      {#if error}<p role="alert">상태를 확인하지 못했습니다. 고급 진단을 확인한 뒤 다시 시도하세요.</p>{/if}
      {#if retryAvailable && !rebootRequired}<button type="button" on:click={retryBootstrap} disabled={busy} data-testid="retry-bootstrap">설치 다시 시도</button>{/if}
      {#if ['login_required', 'connecting'].includes(setup?.state ?? '') && !rebootRequired}<button type="button" on:click={login} disabled={busy} data-testid="login">Tailscale 로그인·연결</button>{/if}
      {#if host && !canConfigureHost}<p>아래의 Agent 시작 또는 연결 버튼을 먼저 사용하세요. 연결 후 호스트 설정에서 원격 접속을 켤 수 있습니다.</p>{/if}
      {#if role === 'client' && setup?.state === 'connected'}<p>이 PC의 연결이 준비됐습니다. 접속하려는 호스트 PC 주소를 입력하세요.</p>{/if}
      {#if host && canConfigureHost && serve?.ownership === 'conflict'}<p role="alert">기존 다른 서비스의 연결 설정을 보존했습니다. 고급 진단에서 충돌을 확인하세요.</p>{/if}
      {#if host && canConfigureHost && serve?.ownership === 'absent' && !rebootRequired}
        {#if serve?.https_ready === false}
          <p data-testid="https-approval">Tailscale HTTPS 인증서를 먼저 활성화해야 원격 연결을 켤 수 있습니다.</p>
          <button type="button" on:click={openHttpsSettings} disabled={busy} data-testid="https-settings">Tailscale HTTPS 설정 열기</button>
        {/if}
        {#if serve?.https_ready !== false}
        <label><input type="checkbox" bind:checked={consent} disabled={busy}> 승인된 Tailscale 기기에서 이 PC에 원격 접속할 수 있도록 허용합니다.</label>
        <button type="button" on:click={enable} disabled={!consent || busy} data-testid="serve">원격 연결 활성화</button>
        {/if}
      {/if}
      {#if ready}<p role="status" data-testid="ready">원격 연결 준비가 완료되었습니다.</p>{/if}
      {#if serve?.public_origin}<label>호스트 접속 주소<input readonly value={serve.public_origin} on:focus={(event) => event.currentTarget.select()}></label>{/if}
      {#if serve?.restart_required}<p role="status">Agent를 재시작해야 새 설정이 적용됩니다. 실행 중인 터미널은 자동 종료하지 않습니다.</p>{/if}
      <button type="button" on:click={manual} disabled={busy} data-testid="refresh">새로 고침</button>
      <details><summary>고급 진단</summary>{#if setup}<p>{setup.detail}</p>{/if}{#if serve}<p>HTTPS 443 / Serve: {serve.ownership} — {serve.detail}</p>{/if}{#if error}<pre>{error}</pre>{/if}</details>
    {/if}
  </section>
{/if}
<style>
  .setup{margin:12px 0;padding:14px;border:1px solid #273447;border-radius:9px;background:#131c28}
  header{display:flex;align-items:center;justify-content:space-between;gap:12px}h3{margin:0;font-size:15px}
  p{margin:10px 0}button{margin:6px 8px 6px 0}label{display:flex;align-items:center;gap:8px;margin:12px 0}
  nav{display:flex;flex-wrap:wrap;gap:14px}a{color:#9ac8ff}details{margin-top:12px;color:#a9b9cb}pre{white-space:pre-wrap;overflow-wrap:anywhere}
</style>
