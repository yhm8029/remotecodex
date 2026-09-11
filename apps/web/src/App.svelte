<script lang="ts">
  import { onMount } from 'svelte';
  import { AgentApi, Controller } from '@remotecodex/terminal-client';
  import { Drafts, PaneLayout, type SessionInfo } from '@remotecodex/core';
  import TerminalPane from './TerminalPane.svelte';
  import MediaPane from './MediaPane.svelte';
  import LocalMediaAdmin from './LocalMediaAdmin.svelte';
  const isNative = location.hostname === 'tauri.localhost' || location.protocol === 'tauri:';
  let base = isNative || location.port === '1420' ? 'http://127.0.0.1:3847' : location.origin;
  let ticket = '', label = '내 기기', error = '', busy = false, connected = false;
  let controller: Controller | null = null, sessions: SessionInfo[] = [];
  const layout = new PaneLayout(); const drafts = new Drafts();
  let visible: readonly string[] = [], focused: string | null = null, mobile = false, pageVisible = true;
  let mode: 'terminal' | 'web' | 'apps' | 'desktop' = 'terminal';
  let createOpen = false, newLabel = '', cwd = '', profile = 'powershell';
  let profiles: { id: string; label: string }[] = [];
  let previews: { id: string; origin: string; port: number; project_id: string }[] = [];
  let previewPort = 3000, publicPort = 8444, pairOutput = '';
  let experimentalAccepted = false;
  function applyLayout() { visible = layout.visible; focused = layout.focused; }
  function change() { if (!controller) return; sessions = controller.sessions; connected = controller.connected; error = controller.notice; }
  async function connect(pair = false) {
    busy = true; error = '';
    try {
      controller?.stop(); const api = new AgentApi(base);
      if (pair) { await api.pair(ticket, label); ticket = ''; }
      const next = new Controller(api); controller = next; next.addEventListener('change', change);
      await next.connect(); profiles = await api.request('/profiles');
      if (sessions.length && !visible.length) { layout.show(sessions[0]!.session_id); applyLayout(); }
    } catch (e) { error = e instanceof Error ? e.message : '연결 실패'; } finally { busy = false; }
  }
  onMount(() => {
    const query = matchMedia('(max-width: 760px)');
    const size = () => { mobile = query.matches; layout.setMobile(mobile); applyLayout(); };
    const viewport = () => document.documentElement.style.setProperty('--visual-height', `${visualViewport?.height ?? innerHeight}px`);
    const visibility = () => { pageVisible = document.visibilityState === 'visible'; if (!pageVisible) controller?.stop(); else if (controller && experimentalAccepted) void controller.connect().catch(e => controller?.fail(e)); };
    size(); viewport(); query.addEventListener('change', size); visualViewport?.addEventListener('resize', viewport); document.addEventListener('visibilitychange', visibility);
    return () => { controller?.stop(); query.removeEventListener('change', size); visualViewport?.removeEventListener('resize', viewport); document.removeEventListener('visibilitychange', visibility); };
  });
  function show(id: string, split = false) { layout.show(id, split); applyLayout(); mode = 'terminal'; }
  function focus(id: string) { layout.focus(id); applyLayout(); }
  function hide(id: string) { layout.closeView(id); applyLayout(); }
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
</script>
<svelte:head><title>RemoteCodex · 회사 PC 개발 콘솔</title></svelte:head>
<div class="app-shell">
  <header class="topbar"><div class="brand"><span class="brand-icon">RC</span><div><strong>RemoteCodex</strong><small>한 대의 회사 PC · 어디서든 같은 작업</small></div></div>
    <div class="host-state"><span class:live={connected} class="dot"></span>{connected ? '회사 Agent 연결됨' : '연결 안 됨'}<span class="alpha">SOURCE ALPHA</span></div></header>
  {#if !controller || !connected}
    <main class="onboarding"><section class="connect-card"><p class="eyebrow">PERSISTENT TERMINALS</p><h1>회사에서 하던 그대로.</h1><p>집 PC는 화면과 키보드입니다. 코드와 Codex는 회사 PC에서 계속 실행됩니다.</p>
      <label>회사 Agent 주소 <input type="url" bind:value={base} placeholder="https://office-pc.your-tailnet.ts.net"></label>
      <div class="notice">이 소스는 Windows 실기기 검증 전 개발 버전입니다. 터미널 복원·IME·성능이 아직 출시 기준을 통과하지 않았습니다.</div>
      <label class="checkbox"><input type="checkbox" bind:checked={experimentalAccepted}> 개발 버전의 검증 제한을 확인했습니다.</label>
      <button class="primary" on:click={() => connect()} disabled={busy || !experimentalAccepted}>{busy ? '연결 중…' : '등록된 기기로 연결'}</button>
      <details open><summary>처음 연결하는 기기 등록</summary><label>기기 이름<input bind:value={label}></label><label>회사 PC에서 발급한 일회용 티켓<input type="password" autocomplete="off" bind:value={ticket} placeholder="rc-agent pair 결과의 ticket"></label>
        {#if isNative}<button on:click={localPair}>회사 로컬 사용자로 티켓 발급</button>{/if}
        <button on:click={() => connect(true)} disabled={busy || !ticket || !experimentalAccepted}>이 브라우저 등록</button><p>{pairOutput}</p>
      </details><p class="hint">집/폰에 Codex를 설치할 필요는 없습니다. 회사 Agent와 Tailscale 연결은 먼저 실행되어 있어야 합니다.</p>
    </section></main>
  {:else}
    <div class="workspace">
      <aside class="sidebar"><div class="sidebar-heading">터미널 <button on:click={() => createOpen = !createOpen}>＋ 새로 만들기</button></div>
        {#each sessions as s (s.session_id)}<div class:active={visible.includes(s.session_id)} class="session-item">
          <button class="session-main" on:click={() => show(s.session_id)}><span class:live={s.state === 'running'} class="dot"></span><span><strong>{s.label}</strong><small>{s.profile} · {s.state}</small></span></button>
          <div class="session-actions">{#if !mobile}<button title="분할 화면으로 보기" on:click={() => show(s.session_id, true)}>분할</button>{/if}<button title="회사 프로세스 종료" on:click={() => closeSession(s.session_id)}>종료</button></div>
        </div>{/each}
        <p class="hint">탭 전환·화면 닫기는 회사의 터미널을 종료하지 않습니다.</p>
      </aside>
      <main class="main-panel">{#if isNative}<LocalMediaAdmin/>{/if}<nav class="mode-tabs"><button class:selected={mode === 'terminal'} on:click={() => mode = 'terminal'}>Terminal</button><button class:selected={mode === 'web'} on:click={web}>Web Preview</button>
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
        </section>
        {:else if (mode === 'apps' || mode === 'desktop') && pageVisible}{#key mode}<MediaPane {controller} kind={mode === 'apps' ? 'window' : 'monitor'}/>{/key}{/if}
      </main>
    </div>
  {/if}
  {#if error}<div class="global-notice" role="status">{error}<button aria-label="알림 닫기" on:click={() => error = ''}>×</button></div>{/if}
  <footer>실행 위치: 회사 PC <span>UI 탭/선택/스크롤은 로컬 · 터미널 입력은 선택된 PTY에만 전달</span></footer>
</div>
