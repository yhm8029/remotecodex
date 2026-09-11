# 요구 → 소스 → 실제 검사 → 미실행 게이트 (0.2)

| 요구 | 실제 소스 | 이 환경의 증거 | 남은 검증 |
|---|---|---|---|
| 같은 회사 PTY | rc-agent/session.rs, ws.rs | Controller 테스트 + wire/policy | ConPTY/Codex 실행 |
| 다중 CMD·분할·입력 분리 | App/TerminalPane, lease.rs, controller.ts | per-session/connection tests | 실제 DOM/PTY 두 개 |
| 큰 paste/Ctrl+C | paste.ts, http.rs, session.rs, input.rs | paste 정책·Controller tests | 실제 PTY 혼잡·부분 실패 |
| 재접속 race | controller.ts attempt/reloadEpoch | stale async response/old WS 회귀 tests | two-PC/network switching |
| VT snapshot | terminal.rs | Rust 테스트 작성만 | sync/pending wrap/palette/charset golden |
| 웹 프리뷰 | preview.rs | 소스 유지, native test NOT_RUN | 실제 HMR/SSE/OAuth/cookie |
| 모바일 | App/TerminalPane/Drafts | IME/초안/resize 정책 tests | iOS/Android 실제 키보드 |
| 로컬 GUI 승인 | local.rs, sources.rs, LocalMediaAdmin, Tauri main | 승인/identity wire 정책 | SID/창·모니터 native enumeration |
| 창·모니터 영상 | rc-media/native.rs, rc-agent/media.rs | signal/SDP/ICE policy tests | WGC→H264→SRTP 실제 프레임 |
| video client | media-client.ts, MediaPane | browser transport 대역 15 tests | actual RTCPeerConnection/decoder |
| GUI 입력 | gui_input.rs, gui_session.rs, dpi.rs | 좌표/lease/freshness pure tests | 실제 SendInput/hook/잠금/foreground |
| 원격 차단/정리 | auth.rs/media.rs/helper Drop | client stop/congestion tests | server/helper/hook orphan 검사 |
| 저부하 | isolated I/O/helper, bounded queues | queue/pointer policy tests | CPU/RAM/GPU/latency/soak 실측 |

125개 테스트의 상세 이름과 실행 결과는 `docs/test-results/v0.2.0/all-tests.tap.txt`에 있다. 이 표는 native 동작의 성공 증거가 아니다.
