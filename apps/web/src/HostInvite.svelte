<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import QRCode from 'qrcode';
  import { createInvitation, normalizeHostOrigin } from './invitations';
  import type { AgentApi } from '@remotecodex/terminal-client';

  export let api: AgentApi;
  export let scopes: string[] = [];

  let hostLabel = '회사 PC';
  let busy = false;
  let error = '';
  let message = '';
  let link = '';
  let qr = '';
  let remaining = 0;
  let deadline = 0;
  let generation = 0;
  let disposed = false;

  let timer: ReturnType<typeof setInterval> | null = null;

  const allowedScopes = [
    'terminal.read',
    'terminal.write',
    'terminal.create',
    'terminal.close',
    'preview.read',
    'admin.devices',
    'admin.settings',
  ];

  $: isAdminDeviceScope = scopes.includes('admin.devices');
  $: isLinkValid = link !== '' && remaining > 0;

  function clearLink() {
    link = '';
    qr = '';
    remaining = 0;
    deadline = 0;
  }

  function stopTimer() {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  onMount(() => {
    timer = setInterval(() => {
      if (disposed) {
        stopTimer();
        return;
      }
      if (deadline > 0) {
        const left = Math.ceil((deadline - Date.now()) / 1000);
        remaining = left > 0 ? left : 0;
        if (remaining <= 0) {
          clearLink();
          message = '초대가 만료되었습니다. 새 초대를 만들어 주세요.';
        }
      }
    }, 1000);
  });

  onDestroy(() => {
    disposed = true;
    generation += 1;
    stopTimer();
    clearLink();
  });

  async function create() {
    if (busy) return;
    busy = true;
    error = '';
    message = '';
    clearLink();
    const snapshotApi = api;
    const snapshotScopes = [...scopes];
    const snapshotLabel = hostLabel;
    const id = ++generation;

    try {
      if (snapshotApi.base !== 'http://127.0.0.1:3847' || !snapshotScopes.includes('admin.devices')) {
        throw new Error('scope');
      }

      const { invoke } = await import('@tauri-apps/api/core');

      const serve = await invoke<{ ownership: string; https_ready?: boolean; restart_required?: boolean; public_origin?: string }>('tailscale_status');
      if (disposed || id !== generation) return;
      if (serve.ownership !== 'owned' || serve.https_ready !== true || serve.restart_required !== false || typeof serve.public_origin !== 'string') {
        throw new Error('serve not ready');
      }

      const running = await invoke<{ public_origin?: string }>('local_admin', { operation: 'status' });
      if (disposed || id !== generation) return;

      if (typeof running.public_origin !== 'string') {
        throw new Error('running origin missing');
      }
      const origin = normalizeHostOrigin(serve.public_origin);
      if (origin !== normalizeHostOrigin(running.public_origin)) {
        throw new Error('origin');
      }
      const allowed = allowedScopes.filter((s) => snapshotScopes.includes(s));
      if (!allowed.includes('admin.devices') || allowed.length === 0) {
        throw new Error('scope');
      }

      const issuedAt = Date.now();
      const result = await snapshotApi.request<{ ticket: string; expires_in: number }>('/pair-tickets', { scopes: allowed });
      if (disposed || id !== generation) return;

      if (!Number.isInteger(result.expires_in) || result.expires_in < 1 || result.expires_in > 300) {
        throw new Error('expires');
      }

      const url = createInvitation({ origin, label: snapshotLabel, ticket: result.ticket });
      const data = await QRCode.toDataURL(url, { width: 320, margin: 4, errorCorrectionLevel: 'M' });
      if (disposed || id !== generation) return;

      link = url;
      qr = data;
      deadline = issuedAt + result.expires_in * 1000;
      remaining = Math.ceil((deadline - Date.now()) / 1000);
    } catch (e) {
      if (disposed || id !== generation) return;
      link = '';
      qr = '';
      remaining = 0;
      deadline = 0;
      error = '초대를 만들지 못했습니다. HTTPS 연결과 Agent 재시작 상태를 확인하세요.';
    } finally {
      if (id === generation) {
        busy = false;
      }
    }
  }

  async function copy() {
    if (!isLinkValid || Date.now() >= deadline || disposed) return;
    try {
      await navigator.clipboard.writeText(link);
      message = '초대 링크를 복사했습니다.';
    } catch (e) {
      message = '복사가 차단되었습니다. 아래 텍스트를 직접 선택해 복사해 주세요.';
    }
  }

  function onTextFocus(event: FocusEvent) {
    const target = event.target as HTMLTextAreaElement | null;
    if (target) target.select();
  }
</script>

<section aria-label="다른 기기 연결" class="host-invite">
  <h3>다른 기기 연결</h3>
  <p class="hint">PC에서는 초대 링크를 복사해 사용하고, 모바일에서는 QR을 스캔해 연결하세요. 두 기기 모두 Tailscale에 연결되어 있어야 합니다.</p>

  <label class="row">
    <span>호스트 이름</span>
    <input
      type="text"
      maxlength="80"
      bind:value={hostLabel}
      disabled={busy}
      data-testid="invite-label"
    />
  </label>

  <p class="hint">초대한 기기에 터미널 조작과 기기·설정 관리 권한을 부여합니다. 화면 공유는 별도로 승인합니다.</p>

  <button
    type="button"
    on:click={create}
    disabled={busy || !isAdminDeviceScope}
    data-testid="invite-create"
  >
    {busy ? '생성 중…' : '초대 만들기'}
  </button>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if message}
    <p class="message" role="status" data-testid="invite-message">{message}</p>
  {/if}

  {#if isLinkValid}
    <div class="qr">
      <img data-testid="invite-qr" src={qr} alt="다른 기기 연결 QR 코드" />
      <p class="remaining" data-testid="invite-remaining">남은 시간 {remaining}초</p>
      <button type="button" on:click={copy} data-testid="invite-copy">초대 링크 복사</button>
      <textarea aria-label="초대 링크" readonly on:focus={onTextFocus} value={link}></textarea>
      <p class="hint">모바일에서 QR을 스캔하거나 PC에서 붙여넣어 1회용 5분짜리 초대를 사용하세요.</p>
    </div>
  {/if}
</section>

<style>
  .host-invite {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  input,
  textarea,
  button {
    font: inherit;
  }
  input,
  textarea {
    padding: 0.4rem 0.6rem;
    border: 1px solid #c4c4c4;
    border-radius: 6px;
  }
  button {
    padding: 0.5rem 0.9rem;
    border: none;
    border-radius: 6px;
    background: #2563eb;
    color: #fff;
    cursor: pointer;
  }
  button:disabled {
    background: #9ca3af;
    cursor: not-allowed;
  }
  .hint {
    color: #a9b9cb;
    font-size: 0.85rem;
    margin: 0;
  }
  .error {
    color: #b91c1c;
    margin: 0;
  }
  .message {
    color: #94d8be;
    margin: 0;
  }
  .qr {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    align-items: flex-start;
  }
  .qr img {
    max-width: 100%;
    width: 320px;
    background: #fff;
    border: 1px solid #e5e7eb;
    border-radius: 6px;
    padding: 6px;
  }
  .remaining {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  textarea {
    width: 100%;
    min-height: 3rem;
  }
</style>