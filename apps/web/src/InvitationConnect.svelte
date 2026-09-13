<script lang="ts">
  import { onMount } from 'svelte';
  import { parseInvitation, type Invitation } from './invitations';
  import { loadHosts, removeHost, type SavedHost } from './saved-hosts';

  export let native: boolean = false;
  export let disabled: boolean = false;
  export let oninvite: (value: Invitation) => void;
  export let onselect: (value: SavedHost) => void;
  export let onclear: () => void = () => {};
  export let currentOrigin: string | undefined = undefined;

  let draft: string = '';
  let error: string = '';
  let hosts: SavedHost[] = [];

  onMount(() => {
    try { hosts = loadHosts(localStorage); } catch { hosts = []; }
  });

  function importLink(): void {
    if (disabled) return;
    onclear();
    try {
      const parsed = parseInvitation(draft.trim(), native ? undefined : currentOrigin);
      draft = '';
      error = '';
      oninvite(parsed);
    } catch {
      draft = '';
      error = '초대 링크가 올바르지 않습니다. 회사 PC에서 새 링크를 받아 주세요.';
    }
  }

  function select(host: SavedHost): void {
    if (disabled) return;
    onclear();
    onselect(host);
  }

  function remove(host: SavedHost): void {
    try { hosts = removeHost(localStorage, host.origin); }
    catch { error = '저장된 연결 정보를 삭제하지 못했습니다.'; }
  }
</script>

<section aria-label="간편 연결">
  <h3>초대 링크로 연결</h3>
  <small>회사 PC에서 받은 초대 링크를 붙여넣으세요. 모바일에서는 QR을 스캔해도 됩니다.</small>
  <textarea
    aria-label="초대 링크 붙여넣기"
    bind:value={draft}
    data-testid="invite-paste"
    disabled={disabled}
    autocomplete="off"
    rows="3"
  ></textarea>
  <button data-testid="invite-import" on:click={importLink} disabled={!draft.trim() || disabled}>
    초대 가져오기
  </button>
  {#if error}<p role="alert">{error}</p>{/if}

  {#if hosts.length}
    <h4>저장된 연결</h4>
    <ul>
      {#each hosts as host (host.origin)}
        <li>
          <button type="button" on:click={() => select(host)} disabled={disabled}>
            <strong>{host.label}</strong>
            <span>{host.origin}</span>
          </button>
          <button type="button" aria-label={`저장된 ${host.label} 삭제`} on:click={() => remove(host)} disabled={disabled}>
            삭제
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 0.5rem; width: 100%; }
  textarea { width: 100%; resize: vertical; font-family: monospace; }
  ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0.25rem; }
  li { display: flex; gap: 0.25rem; }
  li button:first-child { flex: 1; display: flex; flex-direction: column; align-items: flex-start; }
  small { color: #a9b9cb; }
</style>