<script lang="ts">
  import { onMount } from 'svelte';
  import { AgentApi, Controller } from '@remotecodex/terminal-client';
  import { Drafts, PaneLayout, type SessionInfo } from '@remotecodex/core';
  import TerminalPane from './TerminalPane.svelte';
  import MediaPane from './MediaPane.svelte';
  import LocalMediaAdmin from './LocalMediaAdmin.svelte';
  import HostSettings from './HostSettings.svelte';
  import DeviceAdmin from './DeviceAdmin.svelte';
  import SessionSidebar from './SessionSidebar.svelte';
  import { previewAdapter, type PreviewFramework } from './preview-adapters';
  const isNative = location.hostname === 'tauri.localhost' || location.protocol === 'tauri:';
  let base = isNative || location.port === '1420' ? 'http://127.0.0.1:3847' : location.origin;
  let setupMode: 'choose' | 'host' | 'client' = isNative ? 'choose' : 'client';
  let ticket = '', label = '내 기기', error = '', busy = false, connected = false;
  let controller: Controller | null = null, sessions: SessionInfo[] = [], history: SessionInfo[] = [];
  const layout = new PaneLayout(); const drafts = new Drafts();
  let visible: readonly string[] = [], focused: string | null = null, mobile = false, pageVisible = true;
  let mode: 'terminal' | 'web' | 'apps' | 'desktop' = 'terminal';
  let createOpen = false, newLabel = '', cwd = '', profile = 'powershell';
  let profiles: { id: string; label: string }[] = [];
  let previews: { id: string; origin: string; port: number; project_id: string }[] = [];
  let previewPort = 3000, publicPort = 8444, pairOutput = '', previewFramework: PreviewFramework = 'vite';
  let experimentalAccepted = false;
  let historyTimer: ReturnType<typeof setTimeout> | undefined;
  let historyReload: Promise<void> | null = null;
  let historyReloadAgain = false;
  let activeSessionSnapshot = '';
  function applyLayout() { visible = layout.visible; focused = layout.focused; }
  function change() { if (!controller) return; sessions = controller.sessions; connected = controller.connected; error = controller.notice; }
  function scheduleHistoryRefresh(delay = 180) {
    clearTimeout(historyTimer);
    historyTimer = setTimeout(() => { historyTimer = undefined; void refreshHistory(); }, delay);
  }
  async function refreshHistory() {
    const current = controller;
    if (!current || !connected) return;
    if (historyReload) { historyReloadAgain = true; return historyReload; }
    historyReload = current.api.request<SessionInfo[]>('/history').then(next => {
      if (controller === current) history = next;
    }).catch(error => current.fail(error)).finally(() => {
      historyReload = null;
      if (historyReloadAgain) { historyReloadAgain = false; scheduleHistoryRefresh(0); }
    });
    return historyReload;
  }
  $: {
    const nextSnapshot = sessions.map(s => `${s.session_id}:${s.state}`).join('\u001f');
    if (nextSnapshot !== activeSessionSnapshot) {
      activeSessionSnapshot = nextSnapshot;
      if (connected) scheduleHistoryRefresh();
    }
  }
  async function connect(pair = false) {
    busy = true; error = '';
    try {
      controller?.stop(); const api = new AgentApi(base);
      if (pair) { await api.pair(ticket, label); ticket = ''; }
      const next = new Controller(api); controller = next; next.addEventListener('change', change);
      await next.connect(); profiles = await api.request('/profiles'); await refreshHistory();
      const firstLive = sessions.find(session => session.state === 'starting' || session.state === 'running');
      if (firstLive && !visible.length) { layout.show(firstLive.session_id); applyLayout(); }
    } catch (e) { error = e instanceof Error ? e.message : '연결 실패'; } finally { busy = false; }
  }
  onMount(() => {
    const query = matchMedia('(max-width: 760px)');
    const size = () => { mobile = query.matches; layout.setMobile(mobile); applyLayout(); };
    const viewport = () => document.documentElement.style.setProperty('--visual-height', `${visualViewport?.height ?? innerHeight}px`);
    const visibility = () => { pageVisible = document.visibilityState === 'visible'; if (!pageVisible) controller?.stop(); else if (controller && experimentalAccepted) void controller.connect().catch(e => controller?.fail(e)); };
    size(); viewport(); query.addEventListener('change', size); visualViewport?.addEventListener('resize', viewport); document.addEventListener('visibilitychange', visibility);
    return () => { controller?.stop(); clearTimeout(historyTimer); query.removeEventListener('change', size); visualViewport?.removeEventListener('resize', viewport); document.removeEventListener('visibilitychange', visibility); };
  });
  function show(id: string, split = false) { layout.show(id, split); applyLayout(); mode = 'terminal'; }
  function focus(id: string) { layout.focus(id); applyLayout(); }
  function hide(id: string) { layout.closeView(id); applyLayout(); }
  function onSelfRevoked() {
    const revoked = controller;
    revoked?.stop();
    if (revoked) revoked.api.token = '';
    controller = null; connected = false; error = 'This device access was revoked.';
  }
  async function create() {
    if (!controller || !newLabel.trim() || !cwd.trim()) return;
    try { const s = await controller.api.request<SessionInfo>('/sessions', { label: newLabel, project_id: null, cwd, profile, cols: 120, rows: 32 });
      await controller.refreshSessions(); show(s.session_id); createOpen = false; newLabel = ''; }
    catch (e) { controller.fail(e); }
  }
  async function closeSession(id: string) {
    if (!controller || !confirm('이 터미널의 회사 PC 프로세스를 실제로 종료합니다. 계속할까요?')) return;
    try { await controller.api.request(`/sessions/${id}`, undefined, 'DELETE'); hide(id); await controller.refreshSessions(); } catch (e) { controller.fail(e); }
  }
  async function renameSession(id: string, label: string) { if (!controller) return; await controller.api.request(`/sessions/${id}/rename`, { label }); await controller.refreshSessions(); await refreshHistory(); }
  async function restartSession(old: SessionInfo) { if (!controller) return; const created = await controller.api.request<SessionInfo>('/sessions', { label: old.label, project_id: old.project_id, cwd: old.initial_cwd, profile: old.profile, cols: old.cols, rows: old.rows }); await controller.refreshSessions(); await refreshHistory(); show(created.session_id); }
  async function web() { mode = 'web'; if (controller) try { previews = await controller.api.request('/previews'); } catch (e) { controller.fail(e); } }
  async function registerPreview() {
    const session = sessions.find(s => s.session_id === focused); if (!controller || !session) return;
    try { const result = await controller.api.request<{ serve_plan: string }>('/previews', { project_id: session.project_id ?? session.session_id, port: Number(previewPort), public_port: Number(publicPort) }); controller.notice = result.serve_plan; await web(); change(); } catch (e) { controller.fail(e); }
  }
  async function openPreview(id: string) {
    if (!controller) return;
    // Open immediately during gesture to avoid popup blocking; authorization is fetched afterwards.
    const win = isNative ? null : window.open('about:blank', '_blank'); if (win) win.opener = null;
    try { const result = await controller.api.request<{ url: string }>(`/previews/${id}/launch`, {});
      if (isNative) { const { invoke } = await import('@tauri-apps/api/core'); await invoke('open_preview', { url: result.url }); }
      else if (win) win.location.replace(result.url); else error = '팝업을 허용한 뒤 다시 열어 주세요.';
    } catch (e) { win?.close(); controller.fail(e); }
  }
  async function localPair() { try { const { invoke } = await import('@tauri-apps/api/core'); const r = await invoke<{ ticket: string }>('local_admin', { operation: 'pair_owner' }); ticket = r.ticket; pairOutput = '회사 로컬 사용자 확인 완료. 이 브라우저 등록을 누르세요.'; } catch (e) { error = String(e); } }
  async function startHost() {
    if (!isNative || busy || !experimentalAccepted) return;
    busy = true; error = ''; pairOutput = '';
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result = await invoke<'attached' | 'started'>('ensure_host');
      base = 'http://127.0.0.1:3847';
      const pair = await invoke<{ ticket: string }>('local_admin', { operation: 'pair_owner' });
      ticket = pair.ticket;
      busy = false;
      await connect(true);
      if (connected) pairOutput = result === 'started' ? '이 PC의 Agent를 시작하고 연결했습니다.' : '실행 중인 이 PC의 Agent에 연결했습니다.';
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
</script>
<svelte:head><title>RemoteCodex · 회사 PC 개발 콘솔</title></svelte:head>
<div class="app-shell">
  <header class="topbar"><div class="brand"><span class="brand-icon">RC</span><div><strong>RemoteCodex</strong><small>한 대의 회사 PC · 어디서든 같은 작업</small></div></div>
    <div class="host-state"><span class:live={connected} class="dot"></span>{connected ? '회사 Agent 연결됨' : '연결 안 됨'}<span class="alpha">SOURCE ALPHA</span></div></header>
  {#if !controller || !connected}
    <main class="onboarding"><section class="connect-card"><p class="eyebrow">PERSISTENT TERMINALS</p><h1>회사에서 하던 그대로.</h1>
      {#if setupMode === 'choose'}
        <p>이 앱을 실행한 PC의 역할을 선택하세요.</p>
        <button class="primary" on:click={() => setupMode = 'host'}>이 PC를 호스트로 사용</button>
        <button on:click={() => setupMode = 'client'}>다른 PC에 접속</button>
      {:else if setupMode === 'host'}
        <p>Agent는 현재 로그인한 일반 Windows 사용자로 실행됩니다. 원격 셸은 그 사용자의 파일과 명령 실행 권한을 가집니다.</p>
        <div class="notice">Tailscale은 별도로 설치하고 구성해야 합니다. Codex·Git·Node·Python은 호스트 PC에서 사용하는 개발 도구이며 RemoteCodex 클라이언트의 필수 런타임이 아닙니다.</div>
        <p>RemoteCodex UI를 닫아도 Agent와 실행 중인 PTY는 계속 유지됩니다.</p>
        <label class="checkbox"><input type="checkbox" bind:checked={experimentalAccepted}> 소스 알파의 미검증 제한과 원격 셸 권한을 확인했습니다.</label>
        <button class="primary" on:click={startHost} disabled={busy || !experimentalAccepted}>{busy ? 'Agent 확인 중…' : '이 PC에서 Agent 시작 또는 연결'}</button>
        <button on:click={() => setupMode = 'choose'} disabled={busy}>뒤로</button>
        {#if pairOutput}<p>{pairOutput}</p>{/if}
      {:else}
      <p>집 PC는 화면과 키보드입니다. 코드와 Codex는 회사 PC에서 계속 실행됩니다.</p>
      <label>회사 Agent 주소 <input type="url" bind:value={base} placeholder="https://office-pc.your-tailnet.ts.net"></label>
      <div class="notice">이 소스는 Windows 실기기 검증 전 개발 버전입니다. 터미널 복원·IME·성능이 아직 출시 기준을 통과하지 않았습니다.</div>
      <label class="checkbox"><input type="checkbox" bind:checked={experimentalAccepted}> 개발 버전의 검증 제한을 확인했습니다.</label>
      <button class="primary" on:click={() => connect()} disabled={busy || !experimentalAccepted}>{busy ? '연결 중…' : '등록된 기기로 연결'}</button>
      <details open><summary>처음 연결하는 기기 등록</summary><label>기기 이름<input bind:value={label}></label><label>회사 PC에서 발급한 일회용 티켓<input type="password" autocomplete="off" bind:value={ticket} placeholder="rc-agent pair 결과의 ticket"></label>
        {#if isNative}<button on:click={localPair}>회사 로컬 사용자로 티켓 발급</button>{/if}
        <button on:click={() => connect(true)} disabled={busy || !ticket || !experimentalAccepted}>이 브라우저 등록</button><p>{pairOutput}</p>
      </details><p class="hint">집/폰에 Codex를 설치할 필요는 없습니다. 회사 Agent와 Tailscale 연결은 먼저 실행되어 있어야 합니다.</p>
      {#if isNative}<button on:click={() => setupMode = 'choose'} disabled={busy}>뒤로</button>{/if}
      {/if}
    </section></main>
  {:else}
    <div class="workspace"><SessionSidebar activeSessions={sessions} historySessions={history} {focused} onselect={(id) => show(id)} onsplit={(id) => show(id, true)} onrename={renameSession} onrestart={restartSession} onclose={closeSession} oncreate={() => createOpen = !createOpen}/>
      <main class="main-panel">{#if isNative && setupMode === 'host'}<HostSettings/><LocalMediaAdmin/>{/if}{#if controller?.host?.scopes.includes('admin.devices')}{#key controller.api}<DeviceAdmin api={controller.api} currentClientId={controller.host!.client_id} {onSelfRevoked}/>{/key}{/if}<nav class="mode-tabs"><button class:selected={mode === 'terminal'} on:click={() => mode = 'terminal'}>Terminal</button><button class:selected={mode === 'web'} on:click={web}>Web Preview</button>
        {#if !mobile}<button class:selected={mode === 'apps'} on:click={() => mode = 'apps'}>Apps <small>P4–P5</small></button><button class:selected={mode === 'desktop'} on:click={() => mode = 'desktop'}>Desktop <small>P6</small></button>{/if}</nav>
        {#if createOpen}<form class="create-form" on:submit|preventDefault={create}><label>이름<input bind:value={newLabel} placeholder="Sales / Codex" required></label><label>회사 PC 폴더<input bind:value={cwd} placeholder="C:\work\sales" required></label><label>셸<select bind:value={profile}>{#each profiles as p}<option value={p.id}>{p.label}</option>{/each}</select></label><button class="primary" type="submit">터미널 시작</button></form>{/if}
        {#if mode === 'terminal' && pageVisible}
          <div class:split={visible.length > 1 && !mobile} class="panes">{#each visible as id (id)}{@const s = sessions.find(s => s.session_id === id)}{#if s}<TerminalPane session={s} {controller} focused={focused === id} isFocused={() => layout.focused === id} {mobile} {drafts} onfocus={focus} onhide={hide}/>{/if}{/each}
            {#if !visible.length}<div class="empty"><h2>터미널을 선택하세요.</h2><p>여러 CMD·PowerShell을 각각 독립적으로 실행할 수 있습니다.</p></div>{/if}
          </div>
        {:else if mode === 'web'}<section class="preview-panel"><h2>회사 localhost 미리보기</h2><p>승인한 IPv4 loopback 포트만 연결합니다. 관리 화면과 다른 HTTPS origin을 사용합니다.</p>
          <div class="preview-register"><label>회사 개발 포트<input type="number" bind:value={previewPort} min="1024" max="65535"></label><label>프리뷰 HTTPS 포트<input type="number" bind:value={publicPort} min="8444" max="8451"></label><button on:click={registerPreview} disabled={!focused}>등록 승인</button></div>
          {#each previews as p}<article class="preview-item"><span>127.0.0.1:{p.port}</span><code>{p.origin}</code><button on:click={() => openPreview(p.id)}>새 창에서 열기 ↗</button></article>{/each}
          <p class="hint">등록 후 해당 HTTPS 포트의 Tailscale Serve 규칙을 별도로 확인·승인해야 합니다. 기존 규칙은 자동 변경하지 않습니다.</p>
          {#if previews.length}{@const adapter = previewAdapter(previewFramework, previews[0].origin, previews[0].port)}<section class="preview-adapter"><h3>Dev server adapter</h3><label>Framework<select bind:value={previewFramework}><option value="vite">Vite 6.4.3</option><option value="next">Next.js 16.3.3</option></select></label><p>{adapter.file} · {adapter.command}</p><pre>{adapter.snippet}</pre>{#each adapter.notes as note}<p class="hint">{note}</p>{/each}</section>{/if}
        </section>
        {:else if (mode === 'apps' || mode === 'desktop') && pageVisible}{#key mode}<MediaPane {controller} kind={mode === 'apps' ? 'window' : 'monitor'}/>{/key}{/if}
      </main>
    </div>
  {/if}
  {#if error}<div class="global-notice" role="status">{error}<button aria-label="알림 닫기" on:click={() => error = ''}>×</button></div>{/if}
  <footer>실행 위치: 회사 PC <span>UI 탭/선택/스크롤은 로컬 · 터미널 입력은 선택된 PTY에만 전달</span></footer>
</div>
