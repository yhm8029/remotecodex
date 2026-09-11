# Windows 기본 검증 — 2026-09-11

`RemoteCodex_Source_v0.2.0.zip`의 소스를 실제 Windows에서 처음 검증했다.
`scripts/check.ps1 -WindowsPtyTests`의 최종 종료 코드는 **0**이다.
원시 출력은 [check.txt](check.txt), 최초 ConPTY 실패 증거는
[conpty-before.txt](conpty-before.txt)에 보존했다.

| 검사 | 결과 |
| --- | --- |
| bootstrap / 실제 npm·root Cargo·desktop Cargo lock 생성 | PASS |
| cargo fmt --all -- --check | PASS |
| cargo check --workspace --all-targets --locked | PASS |
| cargo test --workspace --locked | 35 PASS, ConPTY 1개는 이 명령에서 명시적으로 ignored |
| npm test | core 96 + client 29 = 125 PASS |
| npm run check:web | 오류 0, 경고 0 |
| npm run build:web | PASS, 기존 Tauri 모듈의 정적/동적 import 혼용 경고 있음 |
| 독립 Tauri workspace cargo check --locked | PASS |
| 실제 ConPTY --ignored --nocapture | 1 PASS, CMD 영문·한글 실제 출력 확인 |
| 생성한 ICO의 Windows 디코딩 / 재생성 해시 | 32×32 PASS / 동일 |

환경은 [environment.json](environment.json)을 참고한다. Rust의 기존 unused/dead-code
경고는 남아 있다. 검사 기준이나 테스트 수를 낮추지 않았다.

## 수정과 원인

- PowerShell 5.1이 `node -p`에 전달하는 JavaScript 따옴표를 제거했다.
  `node --version` 결과를 확인하고 버전을 파싱하도록 수정했다.
- `ProcessIdToSessionId`의 Windows API feature/import가 누락되어 있었다.
  `Win32_System_RemoteDesktop`과 해당 import를 추가했다.
- Axum 0.8의 `WebSocketUpgrade`는 선택적 `Option` extractor를 지원하지 않았다.
  handler에서 `Result`로 받고 기존 내부 함수에 `.ok()`를 전달한다.
  [사용한 Axum API](https://docs.rs/axum/0.8.9/axum/extract/ws/struct.WebSocketUpgrade.html).
- Tauri Windows 빌드에 필요한 `icons/icon.ico`가 없었다. 외부 자산이나 의존성 없이
  `node scripts/generate-desktop-icon.mjs`로 재생성할 수 있는 아이콘을 추가했다.
- ConPTY fixture는 초기 커서 질의 `ESC[6n`에 응답하지 않아 멈췄다.
  질의를 누적 바이트에서 확인한 뒤 `ESC[1;1R`로 응답하고 명령을 보낸다.
  무작위 marker의 실제 확장 결과와 `한글왕복` 두 성공 조건을 유지했다.
- 기존 압축된 Rust 코드는 통합 검사에 필요한 `cargo fmt`로 정리했다.
  이 기계적 포맷 변경은 M3에 대규모 구현을 맡긴 결과가 아니다.
- Tauri가 생성하는 `gen/`은 Git 제외 대상에 추가했다.

## M3 작업 단위와 실패 분석

M3에는 Node 버전 확인, Windows API import, Axum handler, 아이콘 생성,
ConPTY 진단, ConPTY handshake를 각각 분리해 맡겼다. 구현마다 필요한 파일만 전달했다.
설계·원인 분석은 주 에이전트와 Terra, 최종 명세·품질 검토는 Sol이 담당했다.
최종 검토에서 P0/P1 또는 요구사항 불일치는 발견되지 않았다.

M3의 JSON 응답에서 중첩 이스케이프가 발생했고, patch 응답에서는 추가·삭제 표시가
누락되었다. 작은 코드 원문 요청으로 형식을 단순화하고 부모가 허용 범위만 적용했다.
bootstrap은 두 번의 유효하지 않은 응답 후 Luna로 전환했다. 아이콘의 Uint8Array API와
ICO 오프셋 오류는 실제 실행·포맷 검토로 특정한 뒤 M3 재시도로 해결했다.
PowerShell 5.1 표준 입력의 한글 치환은 호출 측 문제로 분류하고 UTF-8 설정으로 수정했다.

직접 M3 API 호출 9회(최소 ping 포함)의 반환 usage 합계는 **8,413 tokens**다.
GPT 주 에이전트·하위 에이전트 토큰 계측은 제공되지 않아 전체 사용량으로 표기하지 않는다.
API 키나 원문 인증 정보는 기록하지 않았다.

## 다음 게이트와 제한

Agent E2E, 실제 Codex/TUI·브라우저 연결, Tauri Release 빌드/GUI 실행,
native-media SDK/feature 빌드, 실제 영상·입력, 두 PC Tailscale, 모바일,
성능·24시간 soak, 설치·서명은 **NOT_RUN**이다. GStreamer 도구는 현재 PATH에서
확인되지 않았다. 위 161개 테스트 통과를 전체 제품 인수 완료로 보지 않는다.

root lock의 선택적 native-media 의존성에는 현재 Rust 1.95보다 높은 버전을 요구하는
항목도 있다. native-media를 활성화할 때 SDK와 함께 별도 호환성 검증이 필요하다.
기본 feature 검사만으로 이를 통과 처리하지 않았다.

기존 `docs/test-results/v0.2.0/`와 `SOURCE_MANIFEST.json`은 원본 ZIP의 역사적 증거다.
원본 manifest를 현재 수정된 저장소 전체의 해시 목록으로 사용하지 않는다.
