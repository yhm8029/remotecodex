<script lang="ts">
  import { onMount } from 'svelte';
  import { TerminalView, Controller } from '@remotecodex/terminal-client';
  import { submitByEnter, type SessionInfo, type Drafts } from '@remotecodex/core';
  export let session: SessionInfo;
  export let controller: Controller;
  export let focused = false;
  export let isFocused: () => boolean;
  export let mobile = false;
  export let drafts: Drafts;
  export let onfocus: (id: string) => void;
  export let onhide: (id: string) => void;
  let element: HTMLDivElement;
  let view: TerminalView | undefined;
  let sending = false;
  let ready = false, hasControl = false, composing = false, draft = '', warning = '';
  let cols = session.cols, rows = session.rows;
  function refresh() { ready = view?.ready ?? false; hasControl = controller.owns(session.session_id); warning = view?.warnings.join(' · ') ?? ''; }
  onMount(() => {
    draft = drafts.get(session.session_id);
    view = new TerminalView(element, session, controller, () => isFocused(), refresh);
    controller.addEventListener('change', refresh);
    return () => { drafts.set(session.session_id, draft); controller.removeEventListener('change', refresh); view?.dispose(); };
  });
  $: if (view) { view.terminal.options.disableStdin = !(focused && ready && hasControl); }
  function take() {
    try {
      const owned = controller.sessions.find(s => s.session_id === session.session_id)?.lease;
      const takeover = !!owned && !controller.owns(session.session_id);
      if (takeover && !confirm('다른 연결의 제어권을 회수하고 이 화면에서 입력할까요?')) return;
      controller.acquire(session.session_id, takeover); onfocus(session.session_id);
    } catch (e) { controller.fail(e); }
  }
  async function send() {
    if (sending || composing || !draft.trim()) return;
    try {
      onfocus(session.session_id);
      if (!confirm(`“${session.label}”의 현재 터미널에 원문과 Enter를 보냅니다. Codex가 아니라 일반 셸이면 명령으로 처리될 수 있습니다.\n\n전송할까요?`)) return;
      sending = true; await view?.paste(draft, true); draft = ''; drafts.clear(session.session_id);
    } catch (e) { controller.fail(e); } finally { sending = false; }
  }
  function key(event: KeyboardEvent) { if (submitByEnter(event, composing)) { event.preventDefault(); send(); } }
  function special(value: string) { try { onfocus(session.session_id); controller.input(session.session_id, value); } catch (e) { controller.fail(e); } }
  function resize() {
    if (mobile && !confirm('회사와 집에서 보는 이 PTY의 실제 크기도 함께 바뀝니다. 적용할까요?')) return;
    try { controller.resize(session.session_id, Number(cols), Number(rows)); } catch (e) { controller.fail(e); }
  }
</script>
<section class:focused class="terminal-pane" aria-label={session.label} role="group" on:pointerdown|capture={() => { onfocus(session.session_id); view?.updateInputGate(); }} on:focusin={() => { onfocus(session.session_id); view?.updateInputGate(); }}>
  <header class="pane-toolbar">
    <button class="pane-title" on:click={() => { onfocus(session.session_id); view?.focus(); }}>{session.label}</button>
    <span class:live={ready} class="status">{ready ? hasControl ? '제어 중' : '보기 전용' : '화면 복원 중'}</span>
    {#if !hasControl}<button on:click={take} disabled={!ready || !controller.connected}>제어권 가져오기</button>
    {:else}<button on:click={() => controller.release(session.session_id)}>제어권 해제</button>{/if}
    <button class="icon" title="화면만 닫기 — 회사 프로세스는 유지" on:click={() => onhide(session.session_id)}>×</button>
  </header>
  <div class="terminal-scroll" on:pointerdown={() => onfocus(session.session_id)} role="presentation"><div class="terminal-mount" bind:this={element}></div></div>
  <div class="terminal-tools">
    <button on:click={() => special('\x03')} disabled={!hasControl}>Ctrl+C</button>
    <button on:click={() => special('\x1b')} disabled={!hasControl}>Esc</button>
    <button on:click={() => special('\t')} disabled={!hasControl}>Tab</button>
    <button on:click={() => special('\x1b[A')} disabled={!hasControl}>↑</button>
    <button on:click={() => special('\x1b[B')} disabled={!hasControl}>↓</button>
    <details><summary>화면 크기</summary><label>열 <input type="number" min="2" max="400" bind:value={cols}></label><label>행 <input type="number" min="1" max="150" bind:value={rows}></label><button on:click={resize} disabled={!hasControl}>실제 PTY에 적용</button></details>
  </div>
  <div class="composer">
    <textarea aria-label={`${session.label} 터미널 입력 초안`} placeholder="이 터미널로 보낼 내용 · Shift+Enter 줄바꿈" bind:value={draft}
      on:input={() => drafts.set(session.session_id, draft)} on:compositionstart={() => composing = true} on:compositionend={() => composing = false} on:keydown={key}></textarea>
    <button class="primary" on:click={send} disabled={sending || !ready || !hasControl || composing || !draft.trim()}>{sending ? '전송 중 · Ctrl+C로 중단' : '전송 ↵'}</button>
  </div>
  {#if warning}<details class="compatibility"><summary>실험적 터미널 복원 어댑터 · 검증 제한</summary><p>{warning}</p></details>{/if}
</section>
