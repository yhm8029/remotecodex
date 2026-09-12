# RemoteCodex — continuation guide

## 이 작업 공간의 최신 진행

**2026-09-12 21:27 KST 갱신:** 기능 커밋 3개는
`origin/work/tailscale-onboarding-20260912`에 push했고 원격 HEAD `3b88f53`을
확인했다. 아래 8시간 soak 기록 이후 PC의 Modern Standby로 검사 간격이
34분 벌어져 기존 `run-timeout15`는 FAIL로 끝났다. 로그는 보존했다.
충전기 연결 확인 및 절전 방지 보조 프로세스의 60초 smoke PASS 후
같은 동결 Agent로 새 `run-awake` 실행을 시작했다. 현재 결과 경로는
`C:/Users/user/remotecodex/runtime/soak-24h-20260912-014102/run-awake/result.json`이다.
Node PID 35004, 절전 방지 wrapper PID 38012. 가장 빠른 완료 시각은
2026-09-13 21:27 KST이며 이전 실행 시간을 합산하지 않는다.

`work/tailscale-onboarding-20260912`는 계획 커밋 `5116879`에서 분리한
구현 작업 공간이다. 설치 감지·검증된 공식 MSI 실행·트레이 로그인 안내와
호스트/클라이언트 UI 구현과 로컬 검증을 마쳤다. 실제 MSI 설치·로그인과
설치된 환경의 계정·Serve 보존 실검증은 NOT_RUN이다. 아래의 "계획만 기록" 문단은
원래 계획 커밋 시점의 기록이며 현재 구현 상태는
`docs/test-results/tailscale-onboarding-2026-09-12/README.md`를 따른다.

현재 PC의 기존 checkout `C:/Users/user/remotecodex`에서 별도 24시간 soak가
실행 중이다. `runtime/soak-current.json`과 해당 디렉터리의
`launch-timeout15.json`, `run-timeout15/result.json`을 확인한다. 아래 인계의
"11:20 중단"은 계획을 작성한 다른 PC의 기록이며 이 실행과 혼동하지 않는다.
이 worktree의 빌드·테스트로 기존 Agent 또는 서비스 중인 웹 자산을 바꾸지 않는다.

로컬 검사: Rust 58개, MSI 모의 15개, UI 9개 그룹 PASS. Svelte 진단 0개,
웹 및 Tauri CLI release 빌드 PASS. 실제 네이티브 호스트/클라이언트 감지
PASS이며 테스트용 UI는 종료했다. 사용자가 기존 GitHub 작성자 정보 재사용을
승인하여 이 저장소의 로컬 Git 설정에 적용했다. 백엔드 커밋 `6698c33`,
UI·동작 테스트 커밋 `f960c71`을 생성했다. 현재 PC의 연속 soak는 8시간
체크포인트까지 IN_PROGRESS이며 완료 결과는 아직 없다.

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

## 7. 2026-09-13 automatic Tailscale bootstrap checkpoint

The automatic bootstrap implementation is commits `50151a0` and `49515dd` on
`work/tailscale-onboarding-20260912`. It is followed by the documentation
checkpoint that records validation in
`docs/test-results/tailscale-onboarding-2026-09-13/`.

Validated locally: installer mock `PASS 19`; Rust `58 passed`; Svelte check
`0 errors, 0 warnings`; web production build; Tauri `--no-bundle --ci` build.
The resulting desktop executable hash is
`95D46602A4409F1A825DB1BDC4B22D398B7744C513B01435048D10FC98D23E31`.

Do not launch the missing-client native bootstrap on the PC currently running
the 24-hour soak: its intended behavior is to make a real official download and
may present UAC. Therefore real download/install/update/login/Serve and two-PC
acceptance remain NOT_RUN. The frozen soak checkout and its Agent must remain
unchanged.

## 8. Reporting discipline

For every command, record the exact command, PASS/FAIL/NOT_RUN status, fixture
scope, and result path. Keep credentials, bearer tokens, cookie values, private
paths, and raw public URLs out of logs. Treat local fixture success as bounded
evidence and preserve the distinction between implementation, local validation,
and the remaining external acceptance gates.
