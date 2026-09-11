> v0.2 후속 작업에서는 `CODEX_CONTINUE.md`와 현재 `IMPLEMENTATION_STATUS.md`를 먼저 읽는다. 이 문서는 원래 제품 의도·구현 원칙이며 현재 저장소를 빈 프로젝트로 초기화하라는 뜻이 아니다.

# Codex 구현 시작 지시문 — v1.1

현재는 소스 초안이 존재한다. 먼저 `CODEX_CONTINUE.md`와 `IMPLEMENTATION_STATUS.md`를 읽고 아래 원래 구현 원칙을 지키며 이어서 작업하라. 같은 폴더의 `SPEC.md v1.1`와 `ACCEPTANCE_MATRIX.md`가 기준이다.
이 문서는 구현 지시이고, 제품 요구사항의 최종 기준은 `SPEC.md`다.

## 사용자가 만들려는 제품

회사 Windows PC를 실행 본체이자 서버로 유지한다. 집 PC/휴대폰은 같은 회사 PC의 실제 CMD·PowerShell·Codex PTY에 접속해서 입력하고 실제 출력을 본다. 세션을 다른 컴퓨터로 옮기거나 매 메시지마다 새 Codex를 실행하는 제품이 아니다.

회사 설치 패키지는 Tauri 기반 Setup.exe다. Rust Agent는 UI와 독립적으로 세션을 소유한다. 집에서는 브라우저와 Tailscale만으로 접속할 수 있어야 한다. GUI 영상 엔진은 필요할 때만 실행한다. 모바일은 세션 목록·CLI 출력·명령 작성창 중심이다.

## 확정 개발 순서

P0 기반 검증 → P1 터미널 원격제어 → P2 다중 PTY/재접속 → P3 localhost 프리뷰 → P4 Windows 창 미리보기 → P5 창 마우스/키보드 → P6 전체 데스크톱.

P0는 설계 위험 검증일 뿐 사용자 단계 변경이 아니다. P1부터 모바일 공통 프로토콜, P2부터 모바일 브라우저 동작을 고려한다. P6를 임의로 제외하지 않는다. Supervisor는 후속 선택 기능이며 원격 입출력 구현을 방해하지 않게 한다.

## 첫 작업

1. 현재 저장소와 개발환경을 확인한다. 기존 파일·사용자 변경을 보존한다. SPEC와 다른 코드가 있으면 차이를 기록한다.
2. `IMPLEMENTATION_STATUS.md`를 만들고 요구·테스트 ID, 구현 상태, 검증 상태, 증거 경로를 구분한다.
3. 실제 Windows 실행 환경과 설치된 Codex/PowerShell/Tailscale 버전을 확인한다. 명령 옵션과 라이브러리 API는 공식 문서/실제 --help로 확인한다.
4. P0에서 ConPTY 입출력, 한글 IME, Codex TUI, UI 종료 후 Agent 생존, terminal state engine 복원 PoC를 수행한다.
5. P1의 실제 세로 경로를 구현한다: 설치/실행 → 장치 승인 → PTY 생성 → 원격 웹 입력 → 회사 출력 → UI 종료/재접속.
6. P1의 필수 보안·성능·기능 게이트를 통과한 뒤 P2부터 순서대로 확장한다.

## 변경하면 안 되는 구현 규칙

- PTY 소유자는 Tauri 창이 아니라 독립 Rust Agent다.
- 기존 Windows Terminal 탭을 자동 탈취하지 않는다.
- 같은 세션은 한 작성자, 여러 시청자다. 모바일 열람만으로 PTY 너비를 바꾸지 않는다.
- 재접속은 서버 snapshot/sequence 기준이다. ANSI 최근 문자열 붙이기만으로 복원을 대체하지 않는다.
- 입력은 대량 출력·미리보기·영상·DB 경로와 분리한다. 입력 debounce로 버벅임을 만들지 않는다.
- 터미널 원문 전송은 binary/bounded buffering/flow control을 사용한다.
- 느린 모바일 하나 때문에 회사 PTY 작업이 멈추지 않게 한다.
- 관리 API는 Tailscale과 별개로 인증한다. 무인증 localhost/tailnet 신뢰를 만들지 않는다.
- 원격 기본 노출은 Tailscale Serve + HTTPS/WSS다. 임의 0.0.0.0 bind·Funnel·공개 포트·광범위 정책 변경을 하지 않는다.
- localhost 프리뷰는 관리 UI와 다른 origin에 둔다. 같은 호스트의 포트만 다른 경우 쿠키 한계를 숨기지 않는다.
- 창/화면 캡처는 구독자 0명일 때 중단한다. 매 프레임 PNG/base64/IPC를 정상 경로로 사용하지 않는다.
- 창 제어는 완전한 OS 샌드박스가 아니다. foreground/geometry/권한 실패 시 중단한다.
- UAC·잠금·회사 정책·백신·보안제품을 우회하지 않는다.
- 원격 연결·설치 사실을 숨기지 않는다. 가벼움은 자원 사용량으로 검증한다.
- Codex final 텍스트를 프로세스 종료/작업 완료로 단정하지 않는다.
- 입력 ACK는 명령 실행 완료가 아니다. 미확인 입력을 무조건 재전송하지 않는다.
- “Tauri니까 가볍다”로 끝내지 말고 SPEC의 CPU/RAM/지연/soak 기준을 측정한다.

## 라이브러리와 기존 코드 참고

공식 문서와 검증된 오픈소스를 우선한다. Happy는 모바일 UX·세션 접근 패턴을 참고하되 전체 서버 구조를 복제하지 않는다. PTY, VT 파서, WebRTC, 암호화 알고리즘을 이유 없이 처음부터 구현하지 않는다. 재사용은 원문 파일·commit·라이선스·NOTICE를 남긴다. 별도 Node/Python 상주 런타임이 필요한 방향으로 바꾸지 않는다.

terminal engine과 media stack의 선택은 SPEC의 ADR 게이트를 따른다. PoC에서 확인하지 않은 API를 존재한다고 가정해 대량 코드를 생성하지 않는다.

## 테스트와 보고

각 단계마다 lint/typecheck/unit/protocol/실기기 테스트와 성능 결과를 기록한다. `ACCEPTANCE_MATRIX.md`는 초기 상태가 모두 NOT_RUN이다. 실제 증거가 있는 항목만 VERIFIED로 바꾼다.

Windows가 없는 환경에서는 작성/정적 검사/플랫폼 독립 테스트를 수행하되 Windows ConPTY·캡처·설치가 검증되었다고 주장하지 않는다. 실제 두 PC, Tailscale, 모바일, 24시간 soak가 없으면 해당 항목은 NOT_RUN으로 남긴다. 그 제한과 무관하게 가능한 구현·검증은 계속한다.

컴파일 성공을 제품 완료라고 보고하지 않는다. 실행하지 않은 테스트를 통과로 채우거나, 성능 기준을 낮추거나, 실패 테스트를 삭제해서 진도를 만들지 않는다.

진행 상황은 무엇을 구현했고 무엇을 실행해 확인했는지 구분해 적는다. “계속 구현 중입니다”라는 말만 최종 결과로 남기지 않는다. 작업을 마칠 때는 실행 가능한 산출물, 수정 파일, 테스트 결과, 실패/미실행 항목, 다음 단계의 명확한 상태를 제시한다. 작업 환경 한계로 멈추는 경우 한계를 사실대로 기록하되 완료로 위장하지 않는다.

## 산출물

- 실행 가능한 소스와 dependency lockfiles.
- 단계별 실제 Windows 설치 패키지(빌드 가능한 환경에서만).
- 로컬에서도 실행 가능한 `scripts/check.ps1`, `test-windows.ps1`, `test-e2e.ps1`, `benchmark.ps1`, `package.ps1`.
- `IMPLEMENTATION_STATUS.md`, ADR, 환경/호환성 표, 원시 테스트/성능 결과.
- 초보 사용자도 회사/집 설치와 최초 페어링을 재현할 수 있는 README.
- 서드파티 LICENSE/NOTICE/SBOM.

이 지시문을 요약하거나 설계 문서만 다시 만드는 데서 끝내지 말고 P0의 실제 검증과 P1 구현부터 시작하라.
