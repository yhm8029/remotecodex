<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  type LocalSource = { native: { kind: 'window' | 'monitor'; handle: string }; label: string; rect: {width:number;height:number} };
  let sources: LocalSource[] = [], chosen = '', control = false, notice = '', ticket = '', busy = false;
  async function refresh() { busy = true; try { sources = await invoke('local_admin',{operation:'media_sources'}); chosen = ''; ticket = ''; } catch (e) { notice = String(e); } finally { busy = false; } }
  async function approve() {
    const source = sources.find(s => `${s.native.kind}:${s.native.handle}` === chosen); if (!source) return;
    const description = `${source.label} — ${control ? '화면 및 실제 마우스·키보드 제어' : '보기 전용'}를 승인합니다. 다른 업무 정보가 노출될 수 있습니다.`;
    if (!confirm(description)) return;
    busy = true; try { await invoke('local_admin',{operation:'media_approve',request:{kind:source.native.kind,handle:source.native.handle,control}}); notice = '이 소스를 승인했습니다. 원격 Apps/Desktop에서 목록을 새로 고치세요.'; } catch (e) { notice = String(e); } finally { busy = false; }
  }
  async function clear() { if (!confirm('모든 화면 승인을 취소하고 현재 영상·GUI 입력을 중지할까요? 터미널은 유지됩니다.')) return; try { await invoke('local_admin',{operation:'media_clear'}); notice = '화면 승인과 연결을 모두 해제했습니다.'; } catch (e) { notice = String(e); } }
  async function pair() { if (!confirm('GUI 권한이 포함된 새 기기 등록 티켓을 발행합니다. 신뢰하는 자신의 기기에만 전달하세요.')) return; try { const r = await invoke<{ticket:string}>('local_admin',{operation:'pair_gui'}); ticket = r.ticket; notice = '1회용 · 5분 만료 · 로그/URL에 붙이지 마세요.'; } catch (e) { notice = String(e); } }
</script>
<details class="local-media">
  <summary>회사 PC 로컬 화면 공유 승인</summary>
  <p>같은 Windows 사용자로 연결된 로컬 UI에서만 처리됩니다. 승인하지 않은 창 목록은 원격 클라이언트에 보내지 않습니다.</p>
  <button on:click={refresh} disabled={busy}>회사 창·모니터 목록 읽기</button>
  <label>대상<select bind:value={chosen}><option value="">선택하세요</option>{#each sources as source}<option value={`${source.native.kind}:${source.native.handle}`}>{source.native.kind} · {source.label} · {source.rect.width}×{source.rect.height}</option>{/each}</select></label>
  <label><input type="checkbox" bind:checked={control}> 이 대상의 실제 마우스·키보드 제어도 허용</label>
  <button on:click={approve} disabled={busy || !chosen}>선택한 대상 승인</button>
  <button on:click={clear}>전체 화면 승인 취소</button>
  <button on:click={pair}>GUI 권한 기기 등록 티켓</button>
  {#if ticket}<label>일회용 티켓<input type="password" value={ticket} readonly autocomplete="off" on:focus={event => event.currentTarget.select()}></label><button on:click={() => ticket = ''}>티켓 숨기기</button>{/if}
  <p role="status">{notice}</p>
</details>
<style>.local-media{margin:1rem;padding:.8rem;border:1px solid #354255;border-radius:10px;max-width:900px}.local-media p{font-size:.85rem}.local-media label{display:block;margin:.6rem 0}.local-media select{max-width:100%}.local-media button{margin:.25rem}</style>
