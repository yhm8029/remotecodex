<script lang="ts">
  import type { SessionInfo } from '@remotecodex/core';
  export let activeSessions: SessionInfo[] = [];
  export let historySessions: SessionInfo[] = [];
  export let focused: string | null = null;
  export let onselect: (id: string) => void;
  export let onsplit: (id: string) => void;
  export let onrename: (id: string, label: string) => Promise<void>;
  export let onrestart: (session: SessionInfo) => Promise<void>;
  export let onclose: (id: string) => Promise<void>;
  export let oncreate: () => void;
  let editing = ''; let value = '';
  $: liveIds = new Set(activeSessions.filter(s => s.state === 'starting' || s.state === 'running' || s.state === 'closing').map(s => s.session_id));
  $: historyIds = new Set(historySessions.map(s => s.session_id));
  $: closedSessions = activeSessions.filter(s => !liveIds.has(s.session_id) && !historyIds.has(s.session_id));
  $: rows = [...activeSessions.filter(s => liveIds.has(s.session_id)), ...historySessions.filter(s => !liveIds.has(s.session_id)), ...closedSessions];
  function stateLabel(state: SessionInfo['state']): string {
    return ({ starting: '시작 중', running: '실행 중', closing: '종료 중', exited: '종료됨', lost: '연결 끊김' } as Record<SessionInfo['state'], string>)[state];
  }
  $: groups = rows.reduce((map, s) => { const key = s.initial_cwd || 'History'; (map.get(key) ?? map.set(key, []).get(key)!).push(s); return map; }, new Map<string, SessionInfo[]>());
  async function save(id: string) { const label = value.trim(); if (label) { await onrename(id, label); editing = ''; } }
</script>
<aside class="sidebar" aria-label="터미널 목록"><button on:click={oncreate}>새 터미널</button>
{#each [...groups] as [project, sessions] (project)}<section><h3>{project}</h3>
{#each sessions as s (s.session_id)}{@const history = !liveIds.has(s.session_id)}
<article class:focused={focused === s.session_id}><span>{stateLabel(s.state)} · {s.profile}</span>
{#if editing === s.session_id}<input aria-label="터미널 이름" maxlength="160" bind:value><button on:click={() => save(s.session_id)}>저장</button><button on:click={() => editing = ''}>취소</button>
{:else}{#if history}<span class="history-label">{s.label}</span>{:else}<button on:click={() => onselect(s.session_id)}>{s.label}</button>{/if}
{#if history}<button on:click={() => onrestart(s)}>새 프로세스로 다시 시작</button><small>기존 화면을 복원하지 않고 새 프로세스를 시작합니다.</small>
{:else}<button on:click={() => { editing = s.session_id; value = s.label; }}>이름 변경</button><button on:click={() => onsplit(s.session_id)}>분할</button><button on:click={() => onclose(s.session_id)}>종료</button>{/if}{/if}
</article>{/each}</section>{/each}</aside>
<style>.sidebar{padding:.7rem;overflow:auto}.sidebar article{display:grid;gap:.25rem;padding:.4rem;border-bottom:1px solid #334}.focused{background:#223}.history-label{padding:.3rem;color:#ccd}button,input{max-width:100%}small{color:#9ab}</style>
