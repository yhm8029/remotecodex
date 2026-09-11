# RemoteCodex 0.2.0 — implementation and verification status

**Checkpoint:** 2026-09-12
**State:** SPEC completion in progress
**Source state:** this document and the evidence directory belong to the same implementation checkpoint commit.

This document records evidence, scope, and remaining gates. Local fixture passes
are useful engineering evidence; they do not close external or release gates.

## Evidence that currently passes

| Area | Result | Evidence or scope |
|---|---|---|
| Agent and browser flow | PASS | `docs/test-results/spec-completion-2026-09-12/windows-agent.json`; 14 checks, including eight isolated real CMD PTYs, lease/reconnect/resize/process checks, projection auth, current-screen projection, and Chrome readable/raw toggle |
| Terminal model | PASS | 12 terminal snapshot golden checks plus 3 Unicode checks; full Unicode 17 data and normalized two-cell behavior |
| Projection API | PASS | `GET /api/v1/sessions/{id}/projection`; bounded physical-screen projection, alternate-screen/raw fallback, identity/sequence validation, one pending request, 2.5 s UI freshness |
| Codex TUI | PASS | `docs/test-results/spec-completion-2026-09-12/codex-tui-chrome.json`; installed native Codex profile through ConPTY/browser, live process identity, termination disables composer |
| Session lifecycle | PASS | `docs/test-results/spec-completion-2026-09-12/terminal-flow-chrome.json`; rename preserves UUID/PID, exit moves to history, explicit restart creates a new UUID/PID and preserves project identity |
| Device administration | PASS | `docs/test-results/spec-completion-2026-09-12/device-admin-chrome.json`; revoke, self-revoke, PTY PID preservation, no page errors |
| Native GUI safety | PASS | `docs/test-results/spec-completion-2026-09-12/gui-safety.json`; keydown-before-keyup, owned foreground and geometry, source identity, Win32 fixture |
| Native WGC/H.264 bridge | PASS, bounded | `docs/test-results/spec-completion-2026-09-12/capture-encode-bridge.json`; owned capture/encode fixture only. WebRTC, ICE, Tailscale, input, and two-PC behavior are outside its scope |
| Tauri lifecycle | PASS, bounded | `docs/test-results/spec-completion-2026-09-12/tauri-lifecycle.json`; close/reopen preserves Agent and PTY identity through SID IPC; clean-up is fixture-scoped |
| Preview gateway | PASS, bounded | `docs/test-results/spec-completion-2026-09-12/preview-gateway.json`; auth, one-use tickets, cookie/header policy, WS, SSE, revoke cancellation, and cleanup |
| Vite adapter | PASS | `docs/test-results/spec-completion-2026-09-12/preview-vite.json`; Vite 6.4.3 fixture and real HMR through the Node gateway |
| Next adapter | PASS | `docs/test-results/spec-completion-2026-09-12/preview-next.json`; Next 16.3.3 fixture, changed HTML, and real HMR through the Node gateway |
| PWA | PASS, bounded | `docs/test-results/spec-completion-2026-09-12/pwa-chrome.json`; actual desktop Chrome service-worker cache/offline fallback; physical mobile is not covered |
| SBOM | PASS, bounded | Latest packaged SBOM: 605 components, official CycloneDX 1.6 schema, no schema errors. License inventory retains unresolved/platform/build entries; schema validity is not a legal completeness certification |
| Unsigned installers | BUILT, archive verified | Latest readable-projection build: standard 16,522,844 bytes, offline 232,037,136 bytes. Both archives pass integrity checks; extracted critical resources match staging. See `docs/test-results/spec-completion-2026-09-12/package-results.json` and `package-inspection.json` |

## Readable projection contract

`rc-agent` exposes `GET /api/v1/sessions/{session_id}/projection` to clients
with `TerminalRead`. The response contains `session_id`, `agent_epoch`,
`generation`, `sequence`, and a `projection` object. A supported projection has
`source: "terminal_projection"`, `scope: "current_screen"`, bounded `lines`,
and `truncated: false`.

The model reads current physical rows, masks hidden cells, skips wide-character
spacers, preserves supported Unicode, and bounds output at 150 rows, 400
columns, and 256 KiB. Alternate-screen and over-limit states return
`source: "terminal_raw"` with a reason. The raw xterm stream remains the source
of truth when a projection cannot be represented safely.

`ReadableOutput.svelte` accepts only validated response shapes, rejects control
characters, malformed sequence encoding, stale identity and late responses, permits one pending request, and uses a
2.5-second freshness deadline. Svelte text interpolation keeps terminal output
out of an HTML interpretation path. The browser E2E covers toggling projection
and raw output in actual Chrome.

## Mandatory gates still open

These have not been represented as PASS by the local fixtures:

1. Two-PC Tailscale HTTPS/WSS/ICE behavior, including real remote media and
   reconnect behavior.
2. Physical iOS and Android IME, composition, resize, and viewport behavior.
3. 24-hour soak and aggregate performance thresholds, including p95/p99 input,
   output, memory, and helper cleanup. The idle CSV is a sample, not this gate.
4. Clean-VM offline install, upgrade, uninstall, and owned-resource cleanup.
5. Hardware DPI, lock-screen, multi-monitor, and UIPI acceptance.
6. Complete performance instrumentation/stress harness coverage and resolve the remaining license inventory entries before a verified release.

The native bridge result is intentionally not evidence for item 1. The PWA
result is intentionally not evidence for item 2. Local preview HMR results are
not HTTPS/browser-HMR evidence for the final Tailscale path.

## Historical baseline

`docs/test-results/windows-baseline-2026-09-11/` records the earlier source-only
baseline, including the old TypeScript/Rust/ConPTY test counts and unverified
native, preview, GUI, and mobile claims. Those counts and claims remain useful
for comparison but must not replace the current local result files above.

## Release rule

Report each result as `PASS`, `PASS (bounded)`, `BUILT (unsigned)`, or
`NOT_RUN`, with its evidence path and scope. Do not use “complete”, “release
ready”, or “all mandatory SPEC requirements pass” until every mandatory gate,
the latest build, and source review are closed.
