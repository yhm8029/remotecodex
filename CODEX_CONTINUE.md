# RemoteCodex — continuation guide

## 다음 작업: Tailscale 설치·연결 안내

[작업 계획](docs/superpowers/plans/2026-09-12-tailscale-onboarding.md)에 설치 감지,
공식 설치 프로그램 실행, 로그인 안내, 기존 Serve 설정 연결과 검증 범위를 정리했다.
현재는 **계획만 기록한 상태**다. 다른 PC에서 진행 중인 테스트는 기존 실행본과
설정으로 유지하고, 구현·빌드는 별도 작업 공간에서 진행한다. 이 문서 커밋만으로
Tailscale이나 실행 중인 EXE가 업데이트된 것은 아니다.

이 PC의 이전 24시간 soak는 사용자 요청으로 중단됐으며 24시간 PASS가 아니다.
중단 시점과 다른 PC 테스트의 구분은 위 계획의 인계 참고를 확인한다.

This checkout is at a **SPEC-completion-in-progress** checkpoint. Current local
fixtures cover real Windows PTYs, the readable projection UI, preview adapters,
native safety fixtures, device administration, PWA offline behavior, and several
Tauri flows. Source and evidence are recorded together in this checkpoint. Do not report the project as
complete while the gates in `IMPLEMENTATION_STATUS.md` remain open.

## 1. Establish the checkout and dependencies

Run from the repository root on Windows:

```powershell
git status --short
.\scripts\bootstrap.ps1
```

Keep the existing lockfiles. Use `npm ci --ignore-scripts` when a clean Node
install is needed, and do not silently change dependency versions.

## 2. Run the baseline checks

```powershell
npm test
npm run check:web
npm run build:web
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
```

For a real Agent/browser pass, use a fresh fixture config and an Agent build:

```powershell
.\scripts\test-windows.ps1 -AgentE2E -AgentPath target\release\rc-agent.exe
```

The current evidence for this flow is
`docs/test-results/spec-completion-2026-09-12/windows-agent.json`. It includes eight isolated
real CMD PTYs, authenticated projection access, current-screen projection, and
the actual Chrome readable-output/raw-terminal toggle. Do not replace it with a
home-page or DOM-only smoke check.

## 3. Terminal state and readable output

The terminal model now exposes a bounded current-screen projection. The API is:

```text
GET /api/v1/sessions/{session_id}/projection
Authorization: Bearer <token>
scope: TerminalRead
```

The response includes session identity, Agent epoch, model generation, sequence,
and either `terminal_projection` lines or a `terminal_raw` fallback. Physical
screen limits are 150 rows, 400 columns, and 256 KiB. Alternate-screen and
over-limit states fall back to raw xterm output. The web client validates the
response, allows one request in flight, validates sequence encoding and rejects stale identities and late responses, and
uses a 2.5-second freshness window.

Current terminal golden and Unicode evidence includes 12 terminal snapshot golden checks plus 3 Unicode checks,
full Unicode 17 width data, and normalized two-cell behavior. When debugging a
regression, use the model tests and the local Agent E2E result together; a
projection is a view of the current physical screen, not a reconstructed shell
history.

## 4. Preview adapters and PWA

The local preview evidence is split by concern:

- `docs/test-results/spec-completion-2026-09-12/preview-gateway.json` covers authenticated tickets,
  cookie and security-header handling, WS/SSE forwarding, revoke cancellation,
  and cleanup.
- `docs/test-results/spec-completion-2026-09-12/preview-vite.json` covers real Vite 6.4.3
  HMR through the gateway.
- `docs/test-results/spec-completion-2026-09-12/preview-next.json` covers real Next 16.3.3
  HTML changes and HMR through the gateway.
- `docs/test-results/spec-completion-2026-09-12/pwa-chrome.json` covers actual desktop Chrome
  service-worker caching and offline fallback.

These results do not prove HTTPS/browser-HMR on the final Tailscale path,
physical mobile behavior, or OAuth/service-worker behavior outside the fixtures.

## 5. Native and desktop checks

The native fixture commands remain:

```powershell
.\scripts\build-source.ps1 -NativeMedia -Desktop
.\scripts\test-media.ps1
.\scripts\gui-fixture.ps1
```

Current evidence shows owned foreground/geometry and key ordering checks, a
WGC/H.264 capture/encode bridge, and Tauri close/reopen lifecycle behavior. The
bridge does not cover full WebRTC/ICE/Tailscale, and the GUI fixture does not
cover physical lock, DPI, multi-monitor, UIPI, or two-PC acceptance.

## 6. Remaining mandatory work

Before a release claim, run and record the gates below with isolated fixtures and
sanitized result files:

- two-PC Tailscale HTTPS/WSS/ICE and remote media;
- physical iOS/Android IME, composition, resize, and viewport;
- 24-hour soak plus aggregate p95/p99 CPU, memory, input, output, and cleanup;
- clean-VM offline install, upgrade, uninstall, and owned-resource cleanup;
- hardware DPI, lock-screen, multi-monitor, and UIPI acceptance;
- remaining performance instrumentation/stress harness and license inventory closure.

Latest unsigned installers are 16,522,844 bytes (standard) and 232,037,136 bytes
(offline). Their archive checks, resource hashes, and 605-component SBOM schema
validation passed. See the checked-in package-results and package-inspection
JSON files. This does not replace clean-VM installation acceptance.

## 7. Reporting discipline

For every command, record the exact command, PASS/FAIL/NOT_RUN status, fixture
scope, and result path. Keep credentials, bearer tokens, cookie values, private
paths, and raw public URLs out of logs. Treat local fixture success as bounded
evidence and preserve the distinction between implementation, local validation,
and the remaining external acceptance gates.
