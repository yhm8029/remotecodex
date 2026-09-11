# Terminal engine compatibility gate

## 선택

`portable-pty = 0.9.0`, `alacritty_terminal = 0.25.1`을 초기 통합 후보로 사용했다. Alacritty의 VT state machine을 재사용하며 문자열 몇 줄을 터미널 상태로 오인하지 않는다. xterm.js는 클라이언트 렌더러다.

## 구현한 어댑터 범위

메인/대체 화면 기본 grid, 최근 2,000행, 색상/일반 SGR, 현재·저장 cursor 위치, 주된 mouse mode, bracketed paste, 일부 scroll region/tab 상태를 VT repaint로 내보낸다. UTF-8/escape 경계는 server fence에서 분리한다. DA/DSR 응답자는 서버 하나다.

## 미검증/불완전 범위

**full-fidelity라고 부르지 않는다.** v0.2에서 pending-wrap·dynamic palette export와 headless sync-update flush 코드를 추가했지만 Rust 테스트를 실행하지 못했다. saved charset, cursor shape, 일부 underline 속성, 모든 reset/alt-screen 변형, obscure modes, resize와 queued ConPTY output의 정확한 순서는 아직 golden 증거가 없다. 현재 어댑터로 화면의 모든 상태가 항상 보존된다고 보장하지 않는다.

`TerminalModel::snapshot()`은 `experimental_vt_snapshot`으로 광고된다. 재접속 시 이어붙일 원시 VT가 누락되면 임의의 오래된 로그 조각으로 복구하지 않고 새 snapshot을 요구한다. 원문 OSC 52/OSC 8/기타 제어 문자열 일부는 초기 보안 profile에서 차단된다. 문자 그대로 완전한 VT passthrough는 아니다.

## 다음 개발자의 결정 지점

1. 실제 Cargo dependency API와 이 어댑터를 컴파일한다.
2. Codex/PowerShell/xterm의 cell/mode golden을 확보한다.
3. Alacritty public API로 export할 수 없는 필드는 작은 명시적 adapter 확장 또는 engine 교체로 해결한다. `unsafe`로 private memory를 읽거나 임의 parser를 덧붙이지 않는다.
4. ConPTY resize는 output ordering과 막힌 pipe deadlock 테스트를 통과한 뒤에만 안정 기능으로 승격한다.
5. 가능한 경우 최근 같은 크기·generation의 재접속은 ring replay로 보완하되, missing range에서는 반드시 snapshot fallback을 유지한다.

현재 release gate: **NOT_VERIFIED**.

## v0.2 추가 검증 지점

- `Processor::stop_sync`를 사용해 클라이언트가 없을 때도 서버 모델의 sync buffer를 명시적으로 적용한다. 원문 출력은 그대로 흘리고 renderer의 batching과 server canonical state를 분리한다.
- public grid의 `input_needs_wrap`를 기존 마지막 glyph 재출력으로 복원한다. wide glyph/alt 화면/scroll region/ORIGIN 경합을 실제 engine→xterm golden으로 검증해야 한다.
- 공개 renderable colors의 ANSI 0..255 및 foreground/background/cursor를 export한다. palette query/reset과 snapshot의 일치성은 미검증이다.
- 실제 Alacritty/vte upstream 공개 API를 읽어 작성했으나 upstream master와 의존성 0.25.1의 API 일치를 컴파일 없이 보장하지 않는다.
- xterm query 응답 중복 억제 hook에 DECRQM과 palette queries를 보강했다. 실제 xterm 설치·TUI 동작은 아직 NOT_RUN이다.
