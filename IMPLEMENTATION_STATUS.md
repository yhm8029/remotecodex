# RemoteCodex 0.2.0 — 구현·검증 상태

작성일: 2026-09-11. 기준: SPEC 1.2. **소스 알파이며, Windows에서 검증된 완성 배포판이 아니다.**

## 이번 판정

v0.1의 핵심 터미널 코드를 이어서 수정하고 P4/P5/P6의 실제 네이티브 파이프라인을 소스로 연결했다. 이전의 media 인터페이스/비활성 stub만 있는 상태와 다르다. 그러나 Rust·Windows 컴파일, Svelte/Tauri 전체 빌드와 실제 회사/집 연결은 이 환경에서 실행하지 못했다. API/타입/네이티브 호환 오류가 남아 있을 수 있다. 성능 목표를 달성했다는 측정 증거도 없다.

| 검사 | 실제 결과 | 범위/증거 |
|---|---|---|
| TypeScript core 컴파일·단위 테스트 | 96 PASS | wire/lease/좌표/IME/붙여넣기/SDP·ICE 정책 |
| API·Controller·MediaClient 타입 검사·테스트 | 29 PASS | 실제 TS 소스 + Node WebCrypto; 네트워크·영상·저장소는 테스트 대역 |
| 합계 | **125 PASS, 0 FAIL, 0 SKIP** | `docs/test-results/v0.2.0/all-tests.tap.txt` |
| MJS 문법/JSON·TOML/내부 문서 링크 | 별도 정적 검사 | `docs/test-results/v0.2.0/source-validation.json` |
| Rust format/check/test, Windows ConPTY | NOT_RUN | Cargo/rustc/Windows 없음 |
| Svelte/xterm 전체·Tauri 빌드 | NOT_RUN | 실제 의존성 설치 미완료 |
| 실제 Agent E2E, 두 PC Tailscale, iOS/Android | NOT_RUN | 실행 스크립트는 작성; 실행 성공으로 간주하지 않음 |
| 실제 WGC/MF H.264/WebRTC/SendInput | NOT_RUN | SDK/GPU/Windows 없음 |
| CPU·RAM·입력 p95/p99·24시간 soak | NOT_RUN | 목표 수치와 측정값을 혼동하지 않음 |
| 설치 프로그램, 서명, 배포 | 미작성 | 사용자 요청대로 이번 산출물은 소스 |

기존 51개 테스트를 삭제하지 않았다. 이번 125개는 core 96 + client 29라는 러너의 개별 테스트 수다. 반복문 안의 수천 좌표/패킷 검사를 추가 테스트 수로 부풀리지 않는다. 휴대폰 IME 정책 테스트는 실제 Safari 키보드 테스트가 아니다.

## 단계별 변경

| 단계 | 작성한 실제 경로 | 남은 출시 조건 |
|---|---|---|
| P0/P1 | 기존 PTY/인증/독립 Agent 유지. 입력용 WS와 최대 256KiB paste HTTP 경로 분리. 전체 내용 검사 후 8KiB 단위 PTY write, Ctrl+C 우선 경로 | Windows API/borrow/compile 오류 수정, 실제 입력 지연·Codex·IME |
| P2 | 다중 탭·분할 유지, 연결 attempt/reload epoch으로 늦게 도착한 이전 host/session/WS 응답 배제. sync-update flush·pending-wrap·동적 palette 복원 코드 추가 | saved charset/cursor shape 등 full-state, alt-screen/resize golden, 장기 부하 |
| P3 | 기존 승인된 loopback HTTP/WS/SSE 프록시 유지 | 실제 Vite/Next/HMR/OAuth/cookie·Origin/권한 철회 검사. PREVIEW.md 제한 유지 |
| P4 | `native.rs`: WGC/D3D11 → NV12 → Media Foundation H.264 → RTP/WebRTC. Agent의 승인된 source, 시그널링, helper 수명 관리; 브라우저 video 수신 | 네이티브 compile·SDK/factory·D3D11 협상·실제 프레임·인터넷 환경 검사 |
| P5 | 회사 local source 승인, 단일 GUI lease, DPI/letterbox/identity/foreground/신선도 검사, Win32 key/button/text, 회사 입력 우선·긴급 정지·주입 키 해제 | 실제 hook/UIPI/보안 데스크톱/경합/끊김/오클루전 시험 |
| P6 | 승인 모니터 source를 같은 캡처/제어 파이프라인에 연결, Desktop UI와 모니터 선택 | 회전·음수 좌표·DPI·잠금·다중 모니터 실제 검사 |
| 모바일 | 기존 CLI 우선 유지. 열람만으로 resize 금지, 초안/한글 composition/재접속. 모바일 기본 UI에 Desktop/Apps 자동 노출 안 함 | iOS/Android 실기기·PWA/native 앱·push는 미검증/후속 |

## 중요한 구현 선택: 미디어 SDK

영상은 `rc-media --features native-media`의 **선택적 GStreamer 0.24 Rust 바인딩 + Windows GStreamer 1.24 이상 런타임/개발 SDK**로 연결했다. 세부 SDK/플러그인 버전은 실제 lock/Windows build에서 고정해야 한다. SDK/DLL을 이 ZIP에 포함하지 않는다.

Rust Agent나 CLI-only 경로는 GStreamer에 링크하지 않는다. 미디어를 명시적으로 켠 경우에만 별도 helper를 실행한다. 기능 조회용 일시 probe는 별도이며 상시 캡처하지 않는다. 이것은 Node 서버나 Electron을 추가한 변경이 아니다. 다만 설치 용량·활성 영상 RAM·정지 화면 CPU를 측정한 것은 아니며, 최종 경량 목표에 실패하면 adapter를 교체해야 한다. ADR 003 참조.

## 현재 한계 — 완료로 숨기지 말 것

1. **네이티브 코드는 컴파일 검증 전**이다. `native-media`를 끈 기본 빌드에서 media가 false인 것은 의도된 동작이다. feature와 설정을 켜고 factory probe가 통과해도 실기기 검증 완료를 뜻하지 않는다.
2. 처음에는 호스트당 **영상 source 1개·영상 viewer 1개**다. 터미널은 다중 세션·다중 열람 구조를 유지한다. 창/모니터 이동·크기 변경은 안전 정지 후 재승인/다시 열기 방식이다. seamless renegotiation은 아직 아니다.
3. ICE는 승인된 숫자형 Tailscale IPv4/UDP만 허용한다. mDNS-only 후보, 허용되지 않은 주소, UDP 차단 환경은 영상 연결이 안 될 수 있다. 외부 STUN/TURN이나 포트 공개를 몰래 추가하지 않는다. 터미널은 독립적으로 유지한다.
4. 현재는 검증 가능한 D3D11-aware `mfh264enc`를 요구한다. 소프트웨어 인코더·저속 이미지 fallback, 정지 화면 적응형 fps/bitrate는 미구현이다. 모든 PC에서 영상이 켜진다는 보장은 없다.
5. 터미널 상태 복원은 여전히 experimental profile이다. 일부 상태 export를 보강했지만 모든 charset/cursor/Unicode/resize 의미를 검증한 것은 아니다.
6. Agent는 현재 **보이는 콘솔 실행**이다. Tauri UI 종료는 Agent와 독립이지만 Agent 콘솔 종료/로그아웃/재부팅 시 기존 PTY는 보장하지 않는다. Agent 트레이·로그인 자동 시작·설치/업데이트 수명 UX는 남아 있다. 미디어의 공유 표시창/긴급 정지 코드는 별도로 있다.
7. GUI 선택 창 제어는 OS 샌드박스가 아니다. 회사의 실제 포커스/커서에 영향을 준다. SendInput은 권한/잠금에 따라 거절될 수 있으며 원격에서 주입한 키 해제도 OS 제약을 받는다. 로컬 입력을 막거나 UAC를 우회하지 않는다.
8. 관리형 Codex 의미 상태 추적/Supervisor는 완료하지 않았다. `final` 출력만으로 작업 종료/성공을 단정하지 않는다.
9. 실제 dependency lock/SBOM/전이 의존성 라이선스·보안 감사가 아직 없다. 가짜 lockfile이나 측정 CSV를 만들지 않는다.

## 실행할 다음 게이트

`CODEX_CONTINUE.md` → 실제 의존성 resolve → Windows compile/type 오류 수정 → `scripts/test-windows.ps1 -AgentE2E` → 실제 Codex/TUI → P3 fixture → `scripts/test-media.ps1` → `scripts/gui-fixture.ps1`와 두 PC GUI → 원본 ACCEPTANCE_MATRIX/성능/모바일 순서다.

문서만 다시 쓰거나 테스트 기준을 낮추지 말고 해당 소스를 수정한다. 사용자 프로젝트 삭제/기존 세션 종료/공개 서버 배포를 테스트 준비로 자동 수행하지 않는다.
