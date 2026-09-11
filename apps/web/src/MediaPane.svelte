<script lang="ts">
  import { onMount } from 'svelte';
  import { MediaClient, type Controller, type GuiAction } from '@remotecodex/terminal-client';
  import { unletterbox, scanCode, PointerCoalescer, type MediaSource } from '@remotecodex/core';
  export let controller: Controller;
  export let kind: 'window' | 'monitor';
  let video: HTMLVideoElement;
  let client: MediaClient;
  let sources: MediaSource[] = [], selected = '', notice = '', state = 'idle', lease: string | null = null;
  let allowed = false, loading = false, text = '', composing = false, source: MediaSource | null = null;
  let pendingFrame = 0;
  const moves = new PointerCoalescer<GuiAction>();
  const keys = new Set<string>();
  function change() { notice = client.notice; state = client.state; lease = client.lease; source = client.source; if (!lease) { keys.clear(); moves.clear(); } }
  async function refresh() {
    loading = true;
    try { const c = await controller.api.request<{ available: boolean; reason?: string }>('/media/capabilities'); allowed = c.available;
      sources = (await controller.api.request<MediaSource[]>('/media/sources')).filter(s => s.kind === kind);
      if (!c.available) notice = c.reason ?? '회사 rc-media 네이티브 빌드·설정을 확인하세요.';
    } catch (e) { notice = String(e); } finally { loading = false; }
  }
  async function open() { const s = sources.find(s => s.id === selected); if (!s) return; try { await client.open(s); } catch (e) { notice = String(e); } }
  function release() { moves.clear(); keys.clear(); client.release(); }
  function action(a: GuiAction): boolean { try { client.action(a); return true; } catch (e) { release(); notice = String(e); return false; } }
  function point(e: MouseEvent | PointerEvent) { if (!source) return null; const r = video.getBoundingClientRect(); return unletterbox(e.clientX, e.clientY, { x: r.left, y: r.top, width: r.width, height: r.height }, { x: 0, y: 0, width: video.videoWidth || source.rect.width, height: video.videoHeight || source.rect.height }); }
  function flush() { if (pendingFrame) cancelAnimationFrame(pendingFrame); pendingFrame = 0; const m = moves.take(); if (m) action(m); }
  function move(e: PointerEvent) { if (!lease) return; const p = point(e); if (!p) return; moves.move({ kind: 'move', x: p[0], y: p[1] }); if (!pendingFrame) pendingFrame = requestAnimationFrame(flush); }
  function button(e: PointerEvent, down: boolean) {
    if (!lease) return; e.preventDefault(); flush(); const p = point(e);
    if (!p || e.button > 2) { release(); return; }
    if (down) { video.focus(); video.setPointerCapture(e.pointerId); }
    action({ kind: 'button', x: p[0], y: p[1], button: e.button, down });
    if (!down && video.hasPointerCapture(e.pointerId)) video.releasePointerCapture(e.pointerId);
  }
  function wheel(e: WheelEvent) { if (!lease) return; const p = point(e); if (!p) return; e.preventDefault(); flush(); action({ kind: 'wheel', x: p[0], y: p[1], delta: Math.max(-1200, Math.min(1200, Math.round(-e.deltaY / 40) * 120)) }); }
  function key(e: KeyboardEvent, down: boolean) {
    if (!lease || e.isComposing || e.key === 'Process' || e.metaKey) return;
    const scan = scanCode(e.code); if (!scan) return;
    if (!down && !keys.has(e.code)) return;
    e.preventDefault(); flush(); if (down) keys.add(e.code); else keys.delete(e.code);
    action({ kind: 'key', scan: scan[0], extended: scan[1], down });
  }
  function sendText() { if (!text || composing) return; if (action({ kind: 'text', text })) text = ''; }
  onMount(() => {
    client = new MediaClient(controller.api, video); client.addEventListener('change', change);
    const creds = () => { void client.refreshCredentials().catch(e => { release(); notice = String(e); }); };
    const hidden = () => { if (document.hidden) client.stop(); };
    const blur = () => release();
    controller.addEventListener('credentials', creds); document.addEventListener('visibilitychange', hidden); window.addEventListener('blur', blur);
    void refresh();
    return () => { if (pendingFrame) cancelAnimationFrame(pendingFrame); client.stop(); client.removeEventListener('change', change); controller.removeEventListener('credentials', creds); document.removeEventListener('visibilitychange', hidden); window.removeEventListener('blur', blur); };
  });
</script>
<section class="media-panel">
  <h2>{kind === 'window' ? '승인한 Windows 창' : '승인한 회사 모니터'}</h2>
  <p>기본은 보기 전용입니다. 회사의 물리 입력, 화면 잠금, 오래된 프레임은 원격 제어를 해제합니다.</p>
  <div class="media-controls">
    <select bind:value={selected} aria-label="승인된 영상 소스"><option value="">회사에서 승인한 대상 선택</option>{#each sources as s}<option value={s.id}>{s.label}</option>{/each}</select>
    <button on:click={refresh} disabled={loading}>목록 갱신</button><button on:click={open} disabled={!allowed || !selected}>보기 시작</button>
    <button on:click={() => client.stop()} disabled={!source}>보기 종료</button>
    {#if lease}<button class="danger" on:click={release}>원격 입력 해제</button>{:else}<button on:click={() => { try { client.acquire(); } catch (e) { notice = String(e); } }} disabled={!source?.control_allowed || state !== 'live'}>마우스·키보드 제어</button>{/if}
  </div>
  <div class="media-state" role="status">{state} · {lease ? '회사 PC 입력 제어 중' : '보기 전용'} · {notice}</div>
  <video bind:this={video} autoplay muted playsinline tabindex="0" aria-label="회사 프로그램 또는 모니터 영상"
    on:pointermove={move} on:pointerdown={e => button(e, true)} on:pointerup={e => button(e, false)} on:pointercancel={release}
    on:wheel|nonpassive={wheel} on:keydown={e => key(e, true)} on:keyup={e => key(e, false)} on:contextmenu={e => { if (lease) e.preventDefault(); }}></video>
  <div class="media-controls"><button on:click={() => video.play()}>영상 재생</button><button on:click={() => video.requestFullscreen()}>영상 확대</button></div>
  <label>한글·긴 텍스트 입력 (명시적 전송)<textarea bind:value={text} maxlength="1024" on:compositionstart={() => composing = true} on:compositionend={() => composing = false}></textarea></label>
  <button on:click={sendText} disabled={!lease || !text || composing}>회사 창에 텍스트 전송</button>
  {#if !sources.length}<p class="hint">회사 로컬에서 대상을 승인하고 GUI 권한으로 기기를 등록해야 목록이 보입니다. 터미널 권한만으로 다른 업무 창을 자동 공개하지 않습니다.</p>{/if}
  <p class="hint">회사 표시창을 닫거나 Ctrl+Alt+F12를 누르면 영상·제어가 중단됩니다. 창 이동·크기 변경 후에는 입력을 중지하고 새 승인으로 다시 엽니다. UAC·잠금 해제는 지원하지 않습니다.</p>
</section>
<style>
 .media-panel{padding:20px;overflow:auto;height:100%}.media-controls{display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin:10px 0}.media-controls select{min-width:220px;max-width:480px}.media-state{font-size:13px;min-height:24px}video{display:block;width:100%;height:min(62vh,720px);background:#080b12;object-fit:contain;touch-action:none;border:1px solid #334155;border-radius:8px}video:focus{outline:2px solid #2dd4bf}textarea{width:100%;min-height:60px}.danger{background:#702c39}
</style>
