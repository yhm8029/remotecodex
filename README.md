# RemoteCodex — Windows source alpha

RemoteCodex is a Windows remote development console. The Agent owns Windows
PTY sessions, the web and Tauri clients render those sessions, and optional
native media and preview adapters extend the same authenticated control plane.

> **Status: SPEC completion is in progress.** The current source has passed a
> substantial local Windows checkpoint, including real browser, PTY, preview,
> native-fixture, and PWA checks. This is not a claim of complete SPEC or release readiness.

## Current checkpoint

The local evidence below is reviewable in `docs/test-results/spec-completion-2026-09-12/`:

- The Agent/browser E2E is PASS, including eight isolated real CMD PTYs,
  authenticated projection access, current-screen projection, and the actual
  Chrome readable-output/raw-terminal toggle (`docs/test-results/spec-completion-2026-09-12/windows-agent.json`).
- Terminal golden and Unicode checks pass, including full Unicode 17 width data
  and normalized two-cell behavior. The earlier 125 TypeScript / 35 Rust / one
  ConPTY count remains a historical baseline, not the current completion claim.
- Native GUI safety and the owned foreground/geometry checks pass; the
  keydown-before-keyup ordering is covered. The WGC/H.264 bridge fixture passes,
  but it does not cover full WebRTC, ICE, Tailscale, input, or two-PC behavior.
- Device revoke and self-revoke, Codex TUI, rename/exit/restart identity, and
  PWA offline behavior pass in actual desktop Chrome fixtures.
- Real Vite 6.4.3 and Next 16.3.3 fixtures pass through the Node gateway,
  including HMR. These checks do not claim HTTPS or browser HMR over the final
  Tailscale path.
- The latest source produced unsigned standard (16,522,844 bytes) and offline
  (232,037,136 bytes) NSIS installers. Both archives passed integrity inspection,
  and critical extracted resources match staging. The packaged SBOM passes the
  CycloneDX 1.6 schema with 605 components. Hashes and scope are recorded in
  `docs/test-results/spec-completion-2026-09-12/package-results.json`.

Mandatory external gates still outstanding are two-PC Tailscale HTTPS/WSS/ICE,
physical iOS/Android IME, 24-hour and aggregate performance, clean-VM offline
install/uninstall, and hardware DPI, lock-screen, multi-monitor, and UIPI
acceptance. See [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for the
full checkpoint and limits.

## Structure

```text
rc-agent                         authenticated HTTP/WS Agent and PTY owner
apps/web                         Svelte browser client
apps/desktop/src-tauri           Tauri host and Windows lifecycle integration
packages/terminal-client         terminal protocol/client logic
crates/rc-core                   shared protocol and wire types
crates/rc-media                  optional native capture/encode helper
crates/rc-platform-windows       Windows PTY, GUI, autostart, and safety code
docs/adr                          design decisions and compatibility notes
docs/test-results/spec-completion-2026-09-12/  checked-in local evidence
```

## Windows build and test commands

Use Windows 11 x64 with Rust stable MSVC/Cargo, C++ build tools, Node.js 22+,
and WebView2. Tauri prerequisites are documented at
<https://v2.tauri.app/start/prerequisites/>.

```powershell
.\scripts\bootstrap.ps1
.\scripts\build-source.ps1
.\scripts\build-source.ps1 -Desktop

npm test
npm run check:web
npm run build:web
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
.\scripts\test-windows.ps1 -AgentE2E -AgentPath target\release\rc-agent.exe
```

The native-media path additionally needs the pinned Windows GStreamer SDK and
runtime. Keep it explicit and isolated from the default Agent build:

```powershell
.\scripts\build-source.ps1 -NativeMedia -Desktop
.\scripts\test-media.ps1
.\scripts\gui-fixture.ps1
```

## Running a local Agent

```powershell
Copy-Item .\config\agent.example.toml .\config\agent.local.toml
# Set the intended public_origin and local paths in agent.local.toml.
.\target\release\rc-agent.exe --config .\config\agent.local.toml run
.\target\release\rc-agent.exe pair
.\target\release\rc-agent.exe status
```

The browser connects to the configured loopback endpoint. PTY and preview
access remain authenticated and scoped; project directories and device state
must be supplied by the operator. Tailscale Serve and public HTTPS require the
separate external gates listed above.

## Readable terminal projection API

`GET /api/v1/sessions/{session_id}/projection` returns a bounded, authenticated
snapshot of the current physical terminal screen. It requires the bearer token
and `TerminalRead` scope. The response carries the session identity, Agent
epoch, model generation, and a monotonic sequence so clients can reject stale
responses:

```json
{
  "session_id": "...",
  "agent_epoch": "...",
  "generation": 1,
  "sequence": "42",
  "projection": {
    "source": "terminal_projection",
    "scope": "current_screen",
    "lines": ["$ cargo test"],
    "truncated": false
  }
}
```

The server projection uses the current physical screen, removes hidden cells,
preserves supported Unicode text, and bounds the result to 150 rows, 400
columns, and 256 KiB. Alternate-screen and over-limit states return a
`terminal_raw` result with a reason, allowing the UI to retain the raw xterm
view. The projection is not a semantic transcript or shell-history API.

The web client validates source, scope, identity, sequence, control characters,
size, and a 2.5-second freshness window. It permits one request in flight and
falls back to the raw terminal when the state is unsupported or stale. Text is
rendered as text, never as HTML.

## Media and GUI safety

Native capture is optional. Source selection, approval, peer allowlists, and
control permission are explicit. The native bridge fixture proves owned capture
and encoding only; it is not evidence of complete remote video transport.
GUI actions require the owned window identity and foreground/geometry checks.
They must not bypass UIPI, UAC, lock-screen, or multi-monitor restrictions.

## Documentation and evidence

- [Implementation status](IMPLEMENTATION_STATUS.md) — current evidence and
  remaining mandatory gates.
- [Continuation guide](CODEX_CONTINUE.md) — safe command order and handoff
  instructions.
- [Windows baseline evidence](docs/test-results/windows-baseline-2026-09-11/README.md)
  — historical source-only baseline.
- [Media compatibility](docs/compatibility/MEDIA.md)
- [Preview contract](docs/compatibility/PREVIEW.md)

Do not describe the project as complete until the outstanding external gates,
the latest rebuild, and source review have all been closed.

## Building installer artifacts

On the configured Windows MSVC/GStreamer build host, run `./scripts/package-windows.ps1 -Offline`. This builds both unsigned NSIS variants and writes `runtime/artifacts/package-results.json`. Existing generated staging/assets must be moved aside explicitly before a repeat build. Do not install or update over a running Agent; installer hooks reject active or indeterminate host status.

The current snapshot also includes the M3 delegation audit in `docs/test-results/spec-completion-2026-09-12/m3-recent-calls.json`: 19 selected direct calls, 23,228 tokens. Applied snippets, integration corrections, and discarded responses are distinguished; GPT totals are unavailable.
