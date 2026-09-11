# RemoteCodex 필수 명세 구현 계획

> 주 에이전트가 작은 M3 코딩 호출을 직접 관리하고, GPT 하위 에이전트는 설계·리뷰·문서를 맡는다.

**목표:** SPEC 1.2의 필수 P0~P6 기능을 구현하고 현재 환경에서 가능한 실제 검증을 수행한다.
**구조:** 독립 Agent가 PTY를 소유한다. 웹/Tauri는 같은 인증·세션 프로토콜을 사용하고,
영상/Windows 입력은 별도 승인과 on-demand MediaHelper 수명을 유지한다.
**기술:** Rust, Windows ConPTY/Win32, Svelte/xterm, Tauri, GStreamer/WebRTC.

사용자는 2026-09-11에 필수 명세 전체 진행을 승인했다. 이전 ZIP의 소스 전용 범위는
이후 요청에 따라 설치 패키지 구현까지 확장한다. 선택적 Supervisor, 네이티브 모바일 앱,
push, MSI는 필수 완료 조건에 추가하지 않는다. 실제 계정 자동 시작과 Tailscale 노출
변경은 앱의 명시적 동의 흐름으로 구현하며 시험 준비를 이유로 임의 활성화하지 않는다.

## 진행 규칙

- M3 호출은 목표 하나·관련 파일 1~3개·명시적 완료 검사로 제한한다.
- 오류를 재현하고 원인을 특정한 후 수정시킨다. 형식/인코딩 문제와 코드 결함을 구분한다.
- 응답을 검토한 뒤 적용하고 단위 검증 후 다음 과제로 넘어간다.
- M3 반복 실패는 토큰 사용량과 문맥을 고려해 Luna/Spark로 전환한다.
- 전체 필수 인수와 코드 구현을 별도로 추적한다. 외부 장비·시간 부재를 PASS로 바꾸지 않는다.

## P0/P1: 실제 연결과 수명

- [x] 기본 Windows Rust/Svelte/Tauri 검사 및 ConPTY fixture — `53045cc`의 기록 참조.
- [x] `tests/e2e/windows-agent.mjs`: xterm headless 설정 수정 후 실제 Agent 인증/2PTY/lease/중복/복원/resize/종료 분리 9개 검사.
- [x] `apps/desktop/src-tauri/src/lifecycle.rs`, `main.rs`, `apps/web/src/App.svelte`: 명시적 host/client 선택, SID 확인 attach, detached 시작과 설치 경로 검색. 개별 검사 통과; 통합 리뷰 남음.
- [x] Agent 트레이: native message loop와 상태 표시, UI 열기, 원격 차단, GUI 중지, 세션 수를 표시한 종료 확인.
- [x] 로그인 자동 시작: 제품 소유 per-user 항목만 조회/등록/해제, 기본 꺼짐, 앱의 명시적 설정 UI.
- [x] NSIS per-user 패키지: Agent/media/web 자산 검색 규칙과 일치, WebView2 표준/오프라인 분리.
- [x] 업데이트/제거: 실행 중 Agent 교체 거부/승인 경로, 프로젝트 보존, 제품 소유 설정만 정리.
- [ ] 실제 Tauri 실행·닫기 후 동일 PID/PTY 확인, 실제 Codex/PowerShell TUI.

## P2: 복원과 세션 UX

- [x] `terminal.rs`: 커서 모양·깜박임 보존 회귀 재현 및 수정.
- [x] `terminal.rs`: 저장/현재 G0~G3 문자셋 정의 보존을 별도 회귀로 구현.
- [ ] 활성 문자셋, origin/wrap, alternate/reset/Unicode/resize 등은 공개 API와 cross-engine golden을 대조; 별도 terminal-engine ADR.
- [x] 관리형 Codex profile: 검증된 실행 파일/프로세스 식별, 종료 후 composer 차단. 선택적 Supervisor와 분리.
- [x] 종료/lost 이력과 명시적 재시작, 안정적 프로젝트 그룹, 제목 변경, 최근 활동 API/화면.
- [ ] 모바일 읽기 projection/raw fallback, 새 출력 표시·맨 아래 이동, 세션별 초안/IME/크기 보존 실제 브라우저 검사.

## P3: 프리뷰와 PWA

- [x] 실제 HTTP/WS/SSE/HMR/cookie/revoke fixture로 승인된 프리뷰 검증.
- [x] Tailscale 버전·기존 Serve 규칙 조회, 제품 소유 규칙 계획/명시적 적용/해제와 UI.
- [x] PWA manifest·아이콘·등록·독립 offline shell 구현과 캐시 경계 단위 검사. 통합 브라우저 검증 남음.

## P4/P5/P6: 영상과 입력

- [x] 공식 GStreamer 1.28.6 MSVC SDK per-user 설치, SHA256 확인, 과정별 환경 설정.
- [x] root lock의 kstring 2.0.2 고정, native-media check/test/Release build/probe와 필수 factory 8종 확인.
- [x] 소유 테스트 창만 대상으로 WGC → D3D11 → H.264 실제 프레임 검증.
- [ ] 승인 source → helper → WebRTC → 브라우저 video/Presented → GUI lease 실제 통합 검증.
- [x] 정지 장면 fps/bitrate 감소와 변화 시 회복, stalled/live의 정확한 표시.
- [x] 하드웨어 불가 시 제한된 저해상도/저fps software 또는 보기 전용 저속 fallback과 명시적 UI.
- [ ] 테스트 창의 입력/로컬 우선/긴급 중지/키 해제/재승인, monitor 음수 좌표·회전·DPI·잠금 실제 fixture.

## 출시 검증과 외부 조건

- [ ] 단계별 latency/resource/stress/누수 측정 harness와 제품 PID 합산.
- [ ] 전이 의존성 LICENSE/NOTICE/SBOM과 패키지 미디어 플러그인 목록.
- [ ] 두 실제 PC와 Tailscale HTTPS/WSS/ICE, 실제 iOS/Android 전환·IME·키보드.
- [ ] 실제 24시간 soak, GPU/모니터/잠금 조건, 깨끗한 VM의 설치·제거/WebView2 미설치·오프라인 검증.
- [ ] 인증서가 제공되는 경우에만 서명/서명 검증. 인증서가 없으면 unsigned 결과로 구분.

외부 장비·조건을 충족하지 못한 항목은 미검증으로 유지한다. `ACCEPTANCE_MATRIX.md`의
기존 139 + UX22 + V02 추가16 행은 실제 ID별로 매핑하며 테스트 수와 인수 항목 수를 혼동하지 않는다.

## 2026-09-12 통합 체크포인트

- 실제 Agent E2E 10개 PASS: CMD의 확장 DOS 경로 때문에 C:\\Windows로 폴백하던 오류를 수정하고 실제 %CD%를 검사했다.
- 트레이와 status-probe(실행 0 / 명확한 부재 3 / 오류 4), autostart 플랫폼 단위 검사는 통과했다. 실제 설치/계정 설정은 변경하지 않았다.
- 이름 변경/기록/새 UUID로 재시작 구현 후 실제 브라우저 검증 중. 종료 세션 분류와 분할 버튼 회귀를 검토에서 발견해 수정 중이다.
- WGC 소유 창에서 하드웨어 H.264 10 buffers/EOS 실측. WebRTC 및 GUI 제어 성공으로 확대 해석하지 않는다.
- 미디어 적응/소프트웨어 폴백/패키징은 진행 중이며 중간 컴파일 실패가 있어 아직 완료 또는 배포 가능으로 표시하지 않는다.
- M3 반복 실패: 프롬프트의 도구 없는 실행 환경과 실제 타입을 명시한다. 별도 code-runner에서 코드 전용 system 지시, temperature 0.2, reasoning_split, finish_reason 검사를 시험한다. 공식 Chat Completions 지원 범위 확인 완료. 코드 반환 형태가 개선됐지만 잘못된 API는 여전히 컴파일 검증한다.
- 커스텀 PE 파서는 M3 출력의 구조체 오프셋 오류 때문에 채택하지 않았다. 설치된 Microsoft dumpbin으로 일반/지연 DLL 의존성 폐쇄를 계산하는 작은 패키징 도구를 사용한다.

## 최신 체크포인트: 2026-09-12

위 체크박스는 구현 단위이며 전체 인수 결과는 ACCEPTANCE_MATRIX.md와 checked-in 결과 JSON을 따른다. 이전 통합 메모는 당시 상태로 보존한다.

- [x] Native tray/autostart와 명시적 호스트·미디어 설정, Tailscale 소유권 journal·미확정 트랜잭션 보호 구현/단위 리뷰. 실제 계정 설정은 시험을 위해 변경하지 않았다.
- [x] 새 표준/오프라인 NSIS 두 종류 전체 빌드 성공. 포함 파일·무결성·해시·staged media factory·605-component SBOM 스키마 검사 통과. 설치 실행 검증과 라이선스 inventory 완결성은 별도다.
- [x] 관리형 Codex 실제 TUI와 composer 차단, 실제 Chrome rename/history/new UUID 재시작, device revoke/selfrevoke, PWA 오프라인.
- [x] 전체 Unicode17 scalar profile(2-cell normalization) 생성기/드리프트 검사, 12개 실제 snapshot golden과 3개 Unicode 검사 통과.
- [x] 서버 current-screen 읽기 projection(물리 행 유지, 숨김 셀 마스킹, alternate/raw fallback), 실제 Chrome 원문 전환 및 8PTY 포함 14개 Agent 검사 통과.
- [x] 실제 Vite6.4.3/Next16.3.3 Node gateway HMR. HTTPS/실제 원격 브라우저 HMR과 구분한다.
- [x] 실제 소유 창 WGC/H264 bridge와 Win32 input safety fixture. keydown 관측 후 해제, 실제 다른 소유 창 foreground 확인 후 거부, geometry 정상 대조 후 변경 거부를 확인했다.
- [ ] 단계별 latency/resource/stress/누수 harness의 전체 coverage와 합산 private working set 측정. 기존 idle sampler와 짧은 관측으로 SPEC17을 닫지 않는다.
- [ ] 두 실제 PC의 Tailscale/ICE/WebRTC, 물리 모바일, 24h, clean VM, DPI/회전/lock/UIPI 인수.
- [ ] 라이선스 inventory의 플랫폼/빌드/공통 상위 LICENSE 대응 항목을 명확히 정리한다.

최근 M3 호출 19건 23,228토큰과 적용·수정·폐기 결과는 `docs/test-results/spec-completion-2026-09-12/m3-recent-calls.json`에 기록했다. 이 숫자는 전체 세션 비용이 아니다.
