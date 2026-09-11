<script lang="ts">
  import { onMount } from 'svelte';
  import type { AgentApi } from '@remotecodex/terminal-client';
  import type { SessionInfo } from '@remotecodex/core';

  export let api: AgentApi;
  export let session: SessionInfo;
  export let active: boolean = false;
  export let ready: boolean = false;
  export let onAvailable: (available: boolean) => void = () => {};

  interface ProjectionResponse {
    session_id: string;
    agent_epoch: string;
    generation: number;
    sequence: string;
    projection: {
      source: 'terminal_projection' | 'terminal_raw';
      scope?: 'current_screen';
      lines?: string[];
      reason?: string;
      truncated?: boolean;
    };
  }

  let visible: boolean = true;
  let text: string = '';
  let label: string = '';
  let dataSource: 'terminal_projection' | 'terminal_raw' = 'terminal_raw';
  let epoch: number = 0;

  let pending: boolean = false;
  let destroyed: boolean = false;
  let currentDeadline: ReturnType<typeof setTimeout> | null = null;

  const MAX_LINES = 150;
  const MAX_TOTAL_CHARS = 262144;
  const TIMEOUT_MS = 2500;

  function isC0OrC1(code: number): boolean {
    if (code < 0x20) return true;
    if (code === 0x7f) return true;
    if (code >= 0x80 && code <= 0x9f) return true;
    return false;
  }

  function containsControl(s: string): boolean {
    for (let i = 0; i < s.length; i++) {
      const code = s.charCodeAt(i);
      if (isC0OrC1(code)) return true;
      if (s[i] === '\n' || s[i] === '\r') return true;
    }
    return false;
  }

  function digitsOnly(s: string): boolean {
    if (s.length === 0) return false;
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      if (c < 0x30 || c > 0x39) return false;
    }
    return true;
  }

  function rawFallback(): string {
    return '표시할 수 있는 화면 스냅샷이 없습니다. 원본 터미널 출력을 확인하세요.';
  }

  function applyFallback(): void {
    text = rawFallback();
    label = '';
    dataSource = 'terminal_raw';
    onAvailable(false);
  }

  function clearDeadline(): void {
    if (currentDeadline !== null) {
      clearTimeout(currentDeadline);
      currentDeadline = null;
    }
  }

  function invalidate(): void {
    epoch += 1;
    text = '';
    label = '';
    dataSource = 'terminal_raw';
    onAvailable(false);
  }

async function tick(): Promise<void> {
    if (destroyed) return;

    if (!active || !ready || document.visibilityState !== 'visible') {
        applyFallback();
        return;
    }

    if (pending) return;
    pending = true;

    const requestEpoch = epoch;
    const start = Date.now();
    const id = session.session_id;
    const agentEpoch = session.agent_epoch;
    const generation = session.generation;

    let localTimer: ReturnType<typeof setTimeout> | null = setTimeout(() => {
        if (!destroyed && requestEpoch === epoch) {
            applyFallback();
        }
    }, 2500);

    currentDeadline = localTimer;

    try {
        const response = await api.request<ProjectionResponse>(
            `/sessions/${id}/projection`
        );

        if (destroyed || requestEpoch !== epoch) return;

        const elapsed = Date.now() - start;
        if (!active || !ready || document.visibilityState !== 'visible' || elapsed > 2500) {
            applyFallback();
            return;
        }

        if (!response) {
            applyFallback();
            return;
        }

        const projection = response.projection;
        const sequence = response.sequence;

        const identityExact =
            response.session_id === id &&
            response.agent_epoch === agentEpoch &&
            response.generation === generation;

        const sequenceValid =
            typeof sequence === 'string' && digitsOnly(sequence);

        const projectionValid =
            !!projection &&
            projection.source === 'terminal_projection' &&
            projection.scope === 'current_screen' &&
            projection.truncated === false &&
            Array.isArray(projection.lines!) &&
            projection.lines.length <= 150;

        let linesValid = projectionValid;
        let totalChars = 0;

        if (projectionValid) {
            for (const line of projection.lines!) {
                if (typeof line !== 'string' || containsControl(line)) {
                    linesValid = false;
                    break;
                }
                totalChars += line.length;
            }
        }

        const totalValid = totalChars <= 262144;

        if (!identityExact || !sequenceValid || !linesValid || !totalValid) {
            applyFallback();
            return;
        }

        text = projection.lines!.join('\n');
        label = '현재 화면 스냅샷 · terminal_projection';
        dataSource = 'terminal_projection';
        onAvailable(true);
    } catch {
        if (!destroyed && requestEpoch === epoch) {
            applyFallback();
        }
    } finally {
        pending = false;
        if (localTimer !== null) {
            clearTimeout(localTimer);
        }
        if (currentDeadline === localTimer) {
            currentDeadline = null;
        }
    }
}

  let timer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    const onVisibility = () => {
      visible = typeof document === 'undefined' || document.visibilityState === 'visible';
      tick();
    };
    visible = typeof document === 'undefined' || document.visibilityState === 'visible';
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', onVisibility);
    }
    timer = setInterval(() => {
      tick();
    }, 1000);

    return () => {
      destroyed = true;
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
      if (typeof document !== 'undefined') {
        document.removeEventListener('visibilitychange', onVisibility);
      }
      clearDeadline();
      pending = false;
      epoch += 1;
      onAvailable(false);
    };
  });

  $: key = `${api.base}|${session.session_id}|${session.agent_epoch}|${session.generation}|${active}|${ready}|${visible}`;
  $: {
    void api;
    void key;
    if (destroyed) {
      text = '';
      label = '';
      dataSource = 'terminal_raw';
    } else {
      invalidate();
    }
  }
</script>

{#if active && dataSource === 'terminal_projection'}
  <div>
    {#if label}
      <small>{label}</small>
    {/if}
    <pre data-source="terminal_projection" style="white-space: pre-wrap; overflow-wrap: anywhere; max-height: 60vh; overflow: auto;">{text}</pre>
  </div>
{:else if active}
  <p role="note" data-source="terminal_raw">{text}</p>
{/if}
