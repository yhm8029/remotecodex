# RemoteCodex 0.2.0 — 실제 Windows에서 이어가기

이 저장소는 v0.1의 빈 scaffold가 아니다. 기존 소스를 보존한 상태에서 `SPEC.md`(1.2), `IMPLEMENTATION_STATUS.md`, `ACCEPTANCE_MATRIX.md`, `CODEX_START.md`, `docs/adr/003-native-media-adapter.md`를 읽고 이어서 구현하라. 제품 범위는 P1→P6이며 모바일 CLI/경량 요구를 유지한다. 설치 패키지부터 만들지 않는다.

## 현재 증거

Windows 기본 검증은 [2026-09-11 기록](docs/test-results/windows-baseline-2026-09-11/README.md)을 기준으로 이어간다. Node/TypeScript 125개, Rust 35개, 실제 ConPTY 1개와 workspace/Tauri cargo check, Svelte 검사·웹 빌드를 통과했다. transport/video/IndexedDB는 여전히 테스트 대역이며 실제 GUI·native GStreamer·두 PC·모바일·성능은 미검증이다. 아래 1번은 재현 절차이고 다음 기능 검증 게이트는 2번이다. 원본 로그를 유지하고 예상 결과를 테스트 결과에 채우지 마라.

## 1. 컴파일부터 실제로 수행

사용자 변경을 확인하고 작업 branch에서 시작한다. force push/임의 publish/GitHub Actions를 사용하지 않는다. `scripts/bootstrap.ps1`은 실제 dependency lock을 생성한다. lockfile을 검토·커밋하고 실제 Rust/Node/SDK/Codex/Tailscale 버전을 기록한다. npm/Cargo API가 맞지 않으면 소스를 수정하며 버전/라이선스 변경을 문서화한다.

`cargo fmt --all` → `cargo check --workspace --all-targets --locked` → `cargo test --workspace --locked` → `npm test` → `npm run check:web` → `npm run build:web` → Tauri Cargo check 순서다. 125개 테스트를 삭제하거나 실패 검사를 비활성화해 통과시키지 않는다. NativeMedia feature on/off도 각각 빌드한다.

## 2. 실제 핵심 PTY 세로 경로

사용자가 실행한 Agent와 테스트 전용 임시 프로젝트/PTY를 이용한다. `scripts/test-windows.ps1 -AgentE2E -AgentPath target/release/rc-agent.exe`를 실행한다. 이 검사는 임시 기기/CMD 두 개만 생성·정리한다. echo된 입력이 아니라 실제 실행 출력으로 판단한다. 테스트 정리 실패를 숨기지 마라.

실제 PowerShell/Codex TUI, Tauri UI 종료와 Agent 생존, 같은 기기 두 브라우저 탭의 lease, 빠른 분할 focus, 256KiB paste 중 Ctrl+C, 연결 종료/reconnect/기기 해지를 검증하라. 기존 회사 업무 세션을 몰래 종료하지 않는다.

## 3. TerminalModel 완전성

`terminal.rs`에는 headless synchronized-update flush, pending-wrap 복원, dynamic palette export를 추가했고 기존 Rust 단위 테스트를 실행해 통과했다. saved charset/cursor shape/reset/alt-screen/Unicode 폭/resize 순서 등의 누락은 여전히 실측 golden으로 채워야 한다. raw 문자열 재생이나 private memory 접근으로 full-state 문제를 숨기지 않는다. query 응답은 서버 단일 소유자다.

## 4. 웹 프리뷰

기존 `preview.rs`와 PREVIEW.md의 제한을 실제 Vite/Next/WS/SSE/HTTP streaming/쿠키/Origin/revoke/process-replacement fixture에서 검증하라. 관리 bearer와 프리뷰 cookie 경계, loopback allowlist, registered-port 제약을 약화하지 않는다. Oauth/service-worker/document.cookie 호환을 모두 해결했다고 가정하지 않는다.

## 5. P4~P6 네이티브 경로

소스 경로는 `local source approval → scoped media WS → rc-media(native-media) → WGC/D3D11/MF H264/webrtcbin → MediaClient/video → Presented → global GUI lease → GuiSession/SendInput`이다. 단순 provider 계약만 남은 상태가 아니다. 같은 경로를 실제 컴파일하고 고쳐라.

GStreamer Rust 0.24와 Windows x64 MSVC 1.24+ SDK의 실제 호환 버전을 고정한다. SDK/플러그인은 source ZIP에 없다. `scripts/test-media.ps1` probe는 factory 존재만 확인한다. `scripts/gui-fixture.ps1`에서 실제 창/모니터/키보드/마우스/기기2대/타임아웃/physical-preemption/긴급 정지를 확인하기 전 available을 runtime VERIFIED로 승격하지 마라.

현재 한 source/한 video viewer와 geometry 변경 시 재승인 방식, 숫자형 승인 tailnet ICE만 허용하는 제약이 있다. 필요하면 명시적인 ADR로 호환성을 확장하되 외부 STUN/TURN·공개 주소·광범위 포트를 몰래 추가하지 않는다. 현재 정지 화면 적응형 fps와 인코더 fallback은 미완료다. 원본 성능 요구에 맞게 구현·측정한다.

Win32 소스 확인 우선순위: source identity/physical DPI 일치, foreground/UIPI 실패, 임계구간 local hook preemption, stale queued action 거절, own injected down의 해제, named pipe/data-dir ACL. BlockInput/UAC/잠금 우회는 금지한다. `SendInput` 성공과 의도한 창에서의 효과는 별도로 검증한다.

## 6. 출시 잔여와 최적화

Agent tray/명시적 로그인 자동 시작/업데이트 수명 UX를 원본 명세대로 보완한다. UI와 Agent/PTY의 종료를 연결하지 마라. 모바일은 CLI-first와 초안/IME/열람-only size 정책을 실제 iOS/Android로 확인한다.

SPEC의 Release 장비/측정 정의대로 idle CPU/RAM, terminal input p95/p99, 폭주 출력/Ctrl+C, 웹·영상 동시 회귀, MediaHelper 종료/orphan, 24h soak를 검증한다. 정지 화면 적응형 동작을 구현하라. SDK 전체 번들 용량과 활성 helper 메모리도 보고한다. 미달이면 기준을 낮추지 말고 프로파일로 원인을 수정하라.

## 결과 보고

실제 코드 수정, 실행 명령, raw 로그, PASS/FAIL/NOT_RUN, 남은 gate를 남긴다. 모든 P1~P6 제품 인수 조건을 통과해야 완료다. 인터페이스·컴파일·단위 테스트만으로 원격 프로그램 전체가 동작한다고 주장하지 않는다. 원격 Git 저장소는 사용자가 지정하기 전 임의 생성/업로드하지 않는다. EXE 설치형 패키지는 사용자 요청 시 별도로 만든다.
