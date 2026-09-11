# RemoteCodex 설계명세서

> Windows 경량 설치형 원격 개발 콘솔 · Codex 구현용 기준 문서
>
> 버전: 1.2 · 개정일: 2026-09-11 · 상태: 구현 기준 / 소스 작성·검증 상태는 IMPLEMENTATION_STATUS.md 참조
>
> **이 문서의 성능 수치는 목표와 출시 판정 기준이지 측정 결과가 아니다.** 실제 Windows·Tailscale·브라우저 환경에서 검증하기 전에는 달성했다고 표시하지 않는다.

## 0. 제품을 한 문장으로 정의

**회사 Windows PC를 유일한 개발 실행 본체로 유지하면서, 같은 PC의 CMD·PowerShell·Codex 터미널을 회사·집·모바일에서 공유하고, 웹앱·Windows 앱·전체 데스크톱까지 필요한 범위만 단계적으로 원격 확인·조작하는 저부하 개발 콘솔.**

회사 PC에서 코드, 개발 서버, 빌드, DB, Codex가 실제로 실행된다. 집 PC와 휴대폰은 해당 실행 환경의 클라이언트다. 사용자의 위치가 바뀌어도 실행 위치와 터미널 세션을 옮기지 않는다.

### 0.1 변경하면 안 되는 핵심 요구

- 회사 PC = 명령 수신 서버 + 터미널 실행 서버. 별도 중앙 명령 서버나 VPS를 필수로 만들지 않는다.
- 집에서 보낸 입력은 회사 PC의 **선택된 실제 PTY**로 전달한다. 요약 채팅이나 별개 Codex 세션으로 바꾸지 않는다.
- 회사와 집에서 같은 터미널의 출력·커서·색상·진행 상태를 본다. 픽셀 단위 글꼴 일치가 아니라 동일한 터미널 상태를 의미한다.
- 탭별 실행 프로세스와 작업 디렉터리는 독립적이다. 한 프로젝트에 여러 터미널이 있을 수 있다.
- 클라이언트 종료·통신 단절·UI 재시작은 회사 PC의 터미널을 종료시키지 않는다.
- 개발 순서는 **P1 터미널 → P2 다중 PTY/재접속 → P3 localhost → P4 창 미리보기 → P5 창 입력 → P6 전체 데스크톱**으로 고정한다.
- 모바일의 기본 경험은 **CLI 중심**이다. 전체 데스크톱 축소 화면을 모바일 메인으로 사용하지 않는다.
- Windows 설치 패키지는 Tauri 기반 `Setup.exe`. 실행 엔진은 Rust로 작성하며 Node/Python 상주 서버를 제품 의존성으로 두지 않는다.
- 입력 지연·낮은 유휴 CPU/RAM·연결 복구를 부가 기능보다 우선한다.
- “깔려 있는지도 모를 만큼 가벼움”은 **낮은 자원 소모**를 뜻한다. 프로그램·접속 표시·권한 요청을 숨기는 기능을 만들지 않는다.

### 0.2 문서 규칙

`MUST`는 출시 필수, `SHOULD`는 특별한 근거가 없다면 준수, `MAY`는 선택이다. 제약 변경은 ADR(설계 결정 기록)을 남긴다. 다만 ADR만으로 사용자 핵심 요구나 보안 요건을 완화할 수 없다.

기준 문서는 이 `SPEC.md`다. `ACCEPTANCE_MATRIX.md`는 검사 목록, `CODEX_START.md`는 구현 시작 지시다. 구현 결과는 별도 `IMPLEMENTATION_STATUS.md`에 기록하고 설계안 자체를 성공 보고서처럼 덮어쓰지 않는다.

---

## 1. 실제 사용 시나리오

### 1.1 회사에서 작업 시작

1. 회사 PC에 RemoteCodex와 별도 네트워크 도구 Tailscale을 설치한다. Codex와 개발 도구는 기존 회사 PC 환경을 사용한다.
2. RemoteCodex에서 `호스트로 사용`을 선택한다.
3. `Sales / PowerShell`, `BeforeTalk / CMD` 터미널을 각각 만든다.
4. 해당 터미널에서 Codex를 실행하거나 관리형 Codex 프로필로 직접 실행한다.
5. 회사 UI를 닫아도 Agent가 터미널을 소유하므로 작업은 유지된다.

### 1.2 집에서 같은 작업 이어보기

1. 집 PC에서 Tailscale 연결 후 승인된 RemoteCodex 주소에 브라우저로 접속한다.
2. 최초 접속은 회사에서 사전 페어링하거나 회사의 로컬 승인으로 등록한다.
3. `Sales` 탭에 들어가면 기존 터미널 상태가 복원된다.
4. 입력한 문장과 Enter가 회사의 해당 터미널로 전달된다.
5. 회사 PC의 실제 출력이 다시 집으로 전달된다. 집에서 새 Codex를 실행하지 않는다.

### 1.3 웹앱 개발 확인

회사 PC에서 개발 서버를 실행하면 등록 후보 포트를 표시한다. 사용자가 승인한 포트만 프리뷰로 노출한다. 집 브라우저에서 프리뷰를 열고, Codex에게 UI 수정을 지시하고, 회사 개발 서버의 hot reload 결과를 확인한다.

### 1.4 EXE와 전체 화면 확인

P4에서는 승인된 프로그램 창을 보기만 한다. P5에서는 제어권을 받은 클라이언트가 그 창을 조작한다. P6에서는 별도 권한을 받은 뒤 회사 PC의 모니터 전체를 보고 조작한다. 세 모드는 권한과 표시를 구분한다.

### 1.5 모바일

휴대폰에서 세션 목록 → 세션 선택 → CLI 출력 확인 → 한글 입력 → 전송 순서로 사용한다. 이동통신 전환·앱 백그라운드 전환 뒤 다시 접속해도 회사 작업은 계속 살아 있어야 한다.

---

## 2. 범위 및 정확한 한계

### 2.1 포함

Windows 호스트, 회사용 Tauri UI, 집용 웹 UI, 선택적 집용 Tauri 클라이언트, 다중 PTY, 연결 복원, 프리뷰 프록시, 창 캡처, 창 입력, 전체 데스크톱, 모바일용 공통 프로토콜과 반응형 터미널 구조.

### 2.2 초기 범위 밖

중앙 SaaS, 자체 VPN, 자체 NAT traversal, 사용자 과금, 원격 코드 저장소 동기화, IDE 전체 기능, 오디오·카메라·프린터·USB 전달, 무제한 파일 전송, 잠금 해제·Windows 로그인·보안 데스크톱 제어, Linux/macOS 호스트, WSL/Docker 내부 포트 자동 탐색.

WSL/Docker는 사용자가 Windows loopback에 명시적으로 노출한 웹 포트부터 제한 지원할 수 있다. 컨테이너 안의 `localhost`를 Windows `localhost`와 같은 것으로 처리하지 않는다.

### 2.3 오해 방지

| 상황 | 보장 범위 |
|---|---|
| 이미 다른 Windows Terminal에서 실행 중인 탭 | 자동 부착/탈취하지 않음. 새 관리형 터미널을 사용 |
| 집 브라우저 종료/네트워크 단절 | 회사 PTY와 프로세스 유지 |
| 회사 Tauri UI 종료/충돌 | Agent가 정상인 동안 PTY 유지 |
| 회사 Agent 충돌/강제 종료 | 기존 PTY 유지 보장 안 함. 세션을 `lost`로 표시 |
| 회사 로그아웃/재부팅/전원 종료 | 프로세스 연속성 보장 안 함. 이력과 재시작 정보만 복원 |
| 화면 잠금 | 터미널은 허용된 정책에 따라 유지. GUI 캡처/입력은 일시 중지 |
| 절전/최대절전 | 원격 작업 지속을 보장하지 않음. 전원 정책을 사용자 동의 없이 변경하지 않음 |
| Codex가 final 메시지 출력 | 작업 완료나 프로세스 종료로 단정하지 않음 |
| 포트 프리뷰 | 실제 서버는 회사에서, HTML/JS는 집 브라우저에서 실행 |
| 창/데스크톱 미리보기 | 프로그램과 UI 렌더링 모두 회사에서 실행하고 영상만 전달 |

ConPTY는 호스트가 먼저 생성하고 자식 프로세스에 연결하는 구조다. pseudoconsole 종료는 연결된 콘솔 프로세스 종료에 영향을 준다. 따라서 UI와 PTY 소유자의 수명을 분리한다. [S03]

---

## 3. 단계별 개발 및 출시 게이트

P0는 사용자가 요청한 6단계를 바꾸는 기능 단계가 아니라, P1 구현 전 구조적 위험을 확인하는 짧은 검증 작업이다.

| 단계 | 실제 동작 산출물 | 다음 단계 진입 조건 |
|---|---|---|
| P0 기반 검증 | Rust PTY 왕복, 독립 Agent, 인증 설계, 터미널 상태 엔진 후보 비교 | Windows에서 한글·Codex TUI·프로세스 분리 증거 확보 |
| P1 Terminal 원격제어 | 한 개 실제 PTY, 로컬 Tauri·원격 웹 입력/출력, 인증, 기본 설치 EXE | 승인된 집 PC에서 같은 세션 조작, 입력 성능·보안 검사 통과 |
| P2 다중 PTY/재접속 | 독립 탭, 출력 복구, 단일 작성자, 화면 크기 정책, 장기 실행 | 8세션 스트레스·100회 재접속·24시간 유지 검사 통과 |
| P3 localhost Preview | 승인된 loopback 서비스, 별도 origin, WebSocket/HMR 프록시 | Vite·Next.js·정적/스트리밍 프리뷰와 보안 검사 통과 |
| P4 Windows Window Preview | 승인된 창의 저부하 영상, on-demand MediaHelper | GUI 환경 호환·영상 해제·터미널 성능 회귀 검사 통과 |
| P5 원격 마우스/키보드 | 창 제어권, 좌표/배율 처리, 안전한 입력 중단 | 잘못된 창·UAC·잠금·연결 끊김 입력 검사 통과 |
| P6 전체 Remote Desktop | 전체 모니터 보기/조작, 모니터 전환, 세션 전체 제어권 | 일반 사용자 데스크톱 범위의 기능·보안·성능 종합 검사 통과 |
| 모바일 확장 | P1부터 설계, P2부터 브라우저 검증, P3 이후 PWA/앱 단계화 | 기존 서버 API 변경 없이 CLI 중심 사용 가능 |

P4~P6를 만들기 위해 P1~P3의 사용성을 늦추지 않는다. 반대로 P6를 임의로 삭제하지 않는다. 각 단계는 숨겨진 목업이 아니라 실행 가능한 릴리스여야 한다.

---

## 4. 전체 아키텍처

```text
회사 Windows PC / 현재 로그인한 사용자 세션

  RemoteCodex.exe                 rc-agent.exe                 rc-media.exe
  Tauri 로컬 UI                  독립 Rust 실행 엔진          P4부터 필요할 때만
  [Terminal/Web/Apps/Desktop] ---> 인증 / 세션 / PTY 관리 -----> 창·화면 캡처
       |                         |                             영상 인코딩
       | 닫아도 작업 유지         +-- PTY A -> CMD/Codex        GUI 입력
       |                         +-- PTY B -> PowerShell        WebRTC
       |                         +-- 프리뷰 Gateway
       |                         +-- SQLite 메타데이터
       |                         +-- Win32 트레이/긴급 차단
       |
       +---- loopback HTTP/WS + 제한된 named-pipe 관리 IPC

Tailscale Serve: 승인된 tailnet 경로의 HTTPS/WSS를 loopback Gateway로 전달
                               |
                 +-------------+----------------+
                 |                              |
            집 웹/Tauri                     모바일 웹/앱
            전체 기능 UI                    CLI 중심 UI
```

### 4.1 프로세스 구성의 이유

사용자가 받는 것은 설치 파일 하나이지만, 설치 후에는 역할별 실행 파일을 둔다. **설치 파일 하나와 실행 프로세스 하나는 다른 요구다.**

- `rc-agent.exe`: 항상 필요한 최소 구성. PTY 소유, 웹/WS API, 인증, 트레이, 메타데이터 저장.
- `RemoteCodex.exe`: Tauri 화면. 열 때만 WebView를 만들고, 닫으면 기본적으로 UI 프로세스를 종료한다.
- `rc-media.exe`: 캡처·인코딩·GUI 입력. 활성 구독자가 있을 때만 시작한다.

Agent를 단순히 “Tauri가 종료되면 같이 정리되는 자식”으로 만들면 안 된다. 독립 수명을 갖도록 실행하고, UI는 기존 Agent에 연결한다. Agent와 UI의 명시적 종료 명령을 구분한다.

Agent 충돌 시에도 모든 PTY를 보존하는 별도 session-worker 다중 프로세스 구조는 초기 범위가 아니다. 필요하면 이후 ADR로 추가하되 메모리·관리 비용을 측정한다.

### 4.2 기술 스택

| 영역 | 기본 선택 | 제한/검증 |
|---|---|---|
| 설치/네이티브 UI | Tauri 2 계열, Rust, Windows NSIS Setup.exe | 구현 시 호환되는 안정 버전 고정 |
| 프론트엔드 | TypeScript + Svelte + Vite | 정적 산출물. 제품 실행에 Vite/Node 서버 불필요 |
| 터미널 표시 | xterm.js + 필요한 addon만 | 전체 화면·출력마다 반응형 프레임워크 재렌더 금지 |
| Agent 네트워크 | Rust Tokio + Axum 계열 | PTY blocking I/O를 async worker에 직접 올리지 않음 |
| PTY | Windows ConPTY, `portable-pty` 우선 검증 | 기능/정리 제약이 있으면 `windows` crate의 직접 래퍼로 전환 |
| 상태 복원 엔진 | 기존 Rust terminal engine | P0에서 `wezterm-term` / `alacritty_terminal` 후보 비교 후 하나 고정 |
| 메타데이터 | SQLite + rusqlite 계열 | DB는 설정/이력용. 터미널 byte마다 INSERT 금지 |
| 네트워크 접근 | 별도 Tailscale + Serve | 자체 VPN·Funnel·공개 포트 기본 금지 |
| 창 캡처 | Windows.Graphics.Capture | 지원 여부·잠금·장치 유실 검사 |
| 데스크톱 캡처 | WGC 우선, DXGI 대안 검증 | P6에서 모니터/드라이버 매트릭스로 선택 |
| 영상 인코딩 | Windows Media Foundation H.264 경로 | 하드웨어 지원은 런타임 검증. 항상 된다고 가정 금지 |
| 영상 전송 | WebRTC, v1.2 선택적 GStreamer native-media 어댑터 | ADR 003/§29. str0m은 직접 구현 대안으로 유지. 네이티브 SDK 비용·호환 검사 필요 |
| GUI 입력 | Windows 입력 API, 일반 사용자 권한 | UIPI·foreground·좌표 변경에 대한 중단 처리 |

Tauri는 OS WebView를 사용하는 구조이지만 이것만으로 앱의 RAM/CPU 성능이 보장되지는 않는다. Windows용 EXE 설치 패키징과 WebView2 배포 선택은 공식 문서 기준으로 구현한다. [S01][S02]

PTY와 terminal engine은 다른 구성요소다. `portable-pty`를 사용했다고 화면 상태 복구까지 해결된 것으로 취급하지 않는다. [S13][S14]

### 4.3 모듈 경계

```rust
// 역할 경계를 나타내는 개념 인터페이스. 실제 서명은 구현 ADR에 고정.
trait PtyBackend { /* spawn, read, write, resize, wait, shutdown */ }
trait TerminalModel { /* apply_bytes, snapshot, restore_metadata, query_reply */ }
trait NetworkExposure { /* inspect, plan, apply_approved, revoke_owned */ }
trait CaptureSource { /* list_approved, start, resize, stop */ }
trait VideoEncoder { /* capabilities, configure, encode, drain */ }
trait GuiInputBackend { /* validate_target, inject, release_pressed */ }
trait AgentAdapter { /* observed_status, safe_send_mode, resume_capabilities */ }
```

터미널 코어는 Tauri, 영상 코덱, 모바일 플랫폼 타입에 의존하면 안 된다. Windows 전용 기능은 플랫폼 모듈 아래로 격리한다.

---

## 5. 설치·실행·수명 관리

### 5.1 호스트와 클라이언트 모드

하나의 배포 패키지에서 `이 PC를 호스트로 사용` / `다른 PC에 접속`을 선택한다. 집에서 클라이언트 모드만 선택하면 PTY 호스트와 캡처 프로세스를 자동 시작하지 않는다. 집에서 브라우저만 사용하는 경우 RemoteCodex 설치 자체가 필요 없다.

Tailscale은 별도 구성요소임을 안내한다. Codex·Git·Node/Python 등 사용자의 개발 도구는 호스트 환경의 의존성이지 집 클라이언트나 RemoteCodex 자체의 필수 런타임이 아니다.

### 5.2 설치 패키지

- 표준: 서명 가능한 NSIS `RemoteCodex-x.y.z-setup.exe`, 기본 per-user 설치.
- 오프라인 옵션: WebView2 설치 구성요소를 포함한 별도 패키지. 표준 패키지와 파일 크기를 구분한다.
- MSI는 이후 조직 배포용 선택 사항이다.
- 서명 인증서가 없으면 “서명 완료”라고 보고하지 않는다. Windows/보안제품 검사 상태도 별도로 기록한다.
- 백신 차단을 해결하려고 예외 등록·검사 해제·은닉·실행 정책 우회를 자동화하지 않는다.
- 서드파티 구성요소의 라이선스/NOTICE/SBOM을 포함한다.

WebView2가 없는 환경에서 설치를 무조건 생략하면 Tauri UI가 실행되지 않을 수 있다. 설치 크기를 줄이는 것과 의존성 누락을 구분한다. [S02]

### 5.3 시작과 종료

자동 시작은 명시적 동의 후 **사용자 로그인 시 Agent 시작**으로 구현한다. 기본 구현은 로그인 이전 SYSTEM 서비스가 아니다. Windows 서비스와 사용자 GUI 세션을 동일하게 취급하지 않는다. [S23]

| 사용자 동작 | 처리 |
|---|---|
| X로 UI 닫기 | UI 종료, Agent/PTY 유지. 최초 1회 안내 |
| 트레이에서 UI 열기 | UI를 새로 열고 기존 Agent에 연결 |
| 원격 접속 차단 | 원격 토큰·연결·GUI 제어 중단. 로컬 작업은 유지 |
| 터미널 닫기 | 해당 PTY 종료 경고 후 명시적으로 종료 |
| Agent 완전 종료 | 살아 있는 세션 수 안내, 승인 후 전체 종료 |
| UI 업데이트 | 프로토콜 호환 시 Agent 유지 가능 |
| Agent 업데이트 | PTY를 강제로 끊지 않음. 사용자가 종료를 승인할 때 적용 |
| 제거 | 원격 노출/자동 시작/제품 소유 파일만 정리. 프로젝트 삭제 금지 |

트레이는 `대기`, `원격 연결 중`, `GUI 제어 중`, `연결 차단`을 구분한다. 작업 관리자에서도 정상 프로세스로 표시한다.

### 5.4 사용자 세션과 권한

동일 Windows 사용자별 Agent 하나를 기본으로 한다. mutex·named pipe는 사용자 SID 기준으로 구분한다. pipe ACL과 연결 사용자/세션을 확인한다. 임의 다른 로컬 사용자가 Agent 관리 명령을 보내지 못하게 한다.

Agent는 일반 사용자 권한이다. 관리자 권한으로 실행해야만 동작하는 구조를 만들지 않는다. 로그인한 사용자의 전체 권한으로 실행되는 원격 셸이라는 점은 온보딩에서 알린다.

---

## 6. 네트워크와 주소

### 6.1 기본 노출 방식

```text
회사 Agent 내부: http://127.0.0.1:3847
외부 관리 화면: https://office-pc.<tailnet>.ts.net/
접근 경로: 승인된 집/모바일 → Tailscale → Serve → Agent
```

`3847`은 기본 내부 서비스 포트다. 외부 주소에 반드시 `:3847`을 붙일 필요는 없다. HTTPS 443 진입점을 기본으로 하며, 실제 생성된 주소를 앱에서 복사할 수 있게 한다.

Serve는 tailnet 내부 공개용으로 사용한다. Funnel이나 라우터의 인터넷 포트포워딩을 켜지 않는다. Serve의 HTTPS/로컬 프록시 기능을 사용하되 설치된 CLI 버전과 기존 설정을 먼저 확인한다. [S04][S05]

### 6.2 MUST

- Agent 기본 bind는 IPv4 loopback. IPv6를 추가할 때도 `::1`만 사용한다.
- `0.0.0.0` / `::` 기본 bind 금지.
- 원격 클라이언트는 HTTPS/WSS. 로컬 관리 UI의 loopback 통신 예외는 명시적으로 제한한다.
- Tailscale 장치 연결과 RemoteCodex 애플리케이션 인증을 모두 통과해야 한다.
- 앱 설치만으로 tailnet 전체에 광범위 ACL/grant를 부여하지 않는다.
- 설치 전 기존 Serve 설정을 읽고, 사용자가 승인한 제품 전용 규칙만 추가한다. 다른 서비스의 설정을 reset하지 않는다.
- 포트 충돌 시 대안을 제시하되 몰래 타 프로세스를 종료하지 않는다.
- 네트워크 차단은 우회하지 않고 차단 원인·검사 결과를 표시한다.

### 6.3 기능 확장에 따른 추가 경로

P3의 프리뷰는 별도 HTTPS 포트를 사용할 수 있고, P4의 영상은 제한된 tailnet UDP 경로가 추가될 수 있다. 따라서 제품 전체를 “어떤 단계든 한 포트만 필요”라고 설명하지 않는다.

기본 예약안: 관리 HTTPS 443, 프리뷰 HTTPS 8444~8451 중 승인된 활성 항목만, 미디어 UDP는 P4에서 런타임/방화벽 검증 후 좁은 범위를 명시한다. 범위는 설정 가능하며 무제한 포트를 열지 않는다.

GUI/WebRTC 연결이 실패해도 터미널 경로는 유지한다. 필요한 UDP를 승인할 수 없으면 보기 전용 저속 대체 모드를 표시하거나 GUI 기능을 사용할 수 없다고 안내한다. [S10]

---

## 7. 인증·권한·위협 모델

### 7.1 보호할 대상

터미널 제어권, 회사 파일·코드·DB, Codex 인증, 개발 서버, GUI 화면, 장치 등록 정보. 주요 위협은 승인되지 않은 tailnet 장치, 악성 웹페이지/개발 프리뷰, 탈취된 클라이언트, 세션 ID 추측, 재전송, 출력에 포함된 악성 터미널 제어 시퀀스다.

이 제품은 회사 PC에 이미 침투해 동일 Windows 사용자 권한을 가진 악성 프로세스로부터 완전한 격리를 제공하지 않는다. 승인된 원격 셸 작성자 역시 그 Windows 사용자의 파일·명령 실행 권한을 가진다.

### 7.2 장치 페어링

1. 클라이언트는 장치 키를 생성한다. 웹은 WebCrypto P-256 비추출 키를 기본 후보로 하며, 지원·저장 지속성을 실제 브라우저에서 검증한다.
2. 회사 로컬 UI에서 1회용 페어링 티켓을 발행하거나 새 클라이언트의 키 지문을 확인해 승인한다.
3. 티켓은 CSPRNG 기반 128비트 이상, 기본 5분 만료, 1회만 소비한다. 사람이 비교할 짧은 코드는 별도 지문 표시이지 유일한 인증 비밀이 아니다.
4. 승인 결과는 client_id, 공개키, 권한, 승인 시각으로 저장한다. 표시 이름만 보고 장치를 신뢰하지 않는다.
5. 이후에는 nonce·audience·client_id·만료가 포함된 서버 challenge에 서명해 짧은 접근 토큰을 발급받는다.
6. nonce는 단 한 번만 사용하고 다른 호스트·origin의 서명을 재사용하지 못하게 한다.

웹의 비추출 키는 export 제한이지 XSS나 손상된 장치에 대한 완전한 방어가 아니다. 따라서 origin 분리, CSP, 의존성 검증을 함께 적용한다. [S20]

### 7.3 관리 API 인증

- 관리 access token: 기본 10분, 클라이언트 메모리 보관. 관리용 장기 bearer를 localStorage나 URL에 저장하지 않는다.
- 관리 인증을 origin 간 자동 전송되는 공용 쿠키에 의존하지 않는다. P3 프리뷰와 같은 호스트의 다른 포트에서 쿠키가 섞이는 문제를 피한다.
- REST는 Authorization bearer. WebSocket은 인증된 REST에서 발급한 30초·1회용·채널 제한 ticket으로 handshake한다.
- ticket 전달 방식은 WebSocket subprotocol 등 검증 가능한 방법 하나로 고정한다. URL query·접속 로그에 토큰을 남기지 않는다.
- ticket만으로 임의 세션 접근을 허용하지 않고 subject/resource/scope를 재검사한다.
- Origin·Host를 엄격히 검증한다. 원격 native 클라이언트도 관리 API를 무인증 호출하지 않는다.
- 장치 해지 시 활성 WS·구독·입력 lease·프리뷰 세션·미디어 연결을 즉시 철회한다.
- 무인증 rate limit, challenge 생성 상한, 잘못된 서명 backoff, frame/body 크기 상한을 둔다.

로컬 Tauri는 제한된 named-pipe IPC로 로컬 bootstrap 권한을 받는다. localhost에서 왔다는 사실만으로 관리자라고 판단하지 않는다.

### 7.4 권한 모델

`terminal.read`, `terminal.write`, `terminal.create`, `terminal.close`, `preview.read`, `window.view`, `window.control`, `desktop.view`, `desktop.control`, `admin.devices`, `admin.settings`로 구분한다. 기본 새 장치는 읽기 전용이며 필요한 권한을 명시적으로 승인한다.

중요: 이 구분은 UI/API의 오작동·오남용을 줄이는 경계다. `terminal.write`를 허용한 사람은 셸에서 다른 프로그램을 실행할 수 있으므로, 이를 강한 OS 수준 샌드박스 권한으로 광고하지 않는다.

### 7.5 웹·Tauri 보안

- 외부 프리뷰에 Tauri IPC 권한을 부여하지 않는다. 원격 페이지를 privileged Tauri webview의 같은 origin으로 로드하지 않는다. [S06]
- 관리 UI는 엄격한 CSP, 제한된 connect-src, 외부 임의 script 금지, 토큰 포함 URL 금지.
- 터미널 출력은 HTML로 삽입하지 않는다. OSC 링크는 허용된 scheme만 열고 사용자 동작을 요구한다.
- OSC 52 클립보드 쓰기, 자동 파일 열기, shell 실행 링크는 기본 비활성화한다.
- 파서의 OSC/DCS 길이 상한, 압축 해제 상한, 과대 frame 처리를 테스트한다. [S08]
- 비밀·키 입력·화면 녹화는 진단 로그에 기본 저장하지 않는다.

---

## 8. 터미널 실행 모델

### 8.1 세션 모델

프로젝트와 PTY를 분리한다. 프로젝트 하나에서 개발 서버용 터미널과 Codex용 터미널을 동시에 열 수 있다.

```text
Project(Sales)
  ├─ Terminal(Codex)
  ├─ Terminal(PowerShell / npm dev)
  └─ Preview(web-1)
Project(BeforeTalk)
  └─ Terminal(CMD)
```

필수 세션 필드: `session_id`, `project_id`, `label`, `launch_profile`, `initial_cwd`, `shell/program_path`, `process_identity(pid + creation_time)`, `agent_epoch`, `generation`, `state`, `cols`, `rows`, `output_seq`, `lease_epoch`, `last_activity_at`.

PID만으로 동일 프로세스를 판별하지 않는다. PID 재사용과 세션 generation 변경을 구분한다.

### 8.2 생성

CMD, Windows PowerShell, 설치된 PowerShell은 존재 여부를 확인해 프로필로 표시한다. 실행 경로/인자를 구조화해 처리하고, UI 입력을 불필요하게 문자열 shell concatenation으로 조립하지 않는다.

사용자 프로필 환경과 PATH를 존중하되 Agent의 인증용 비밀을 자식 환경에 상속시키지 않는다. 초기 cwd·환경 추가값은 검증하고 UI에서 확인 가능하게 한다.

기존 Windows Terminal 프로세스의 콘솔을 AttachConsole/키보드 훅으로 탈취하는 코드를 만들지 않는다.

### 8.3 I/O와 수명

ConPTY의 blocking read/write는 분리된 I/O 스레드나 검증된 전용 worker로 처리한다. 동시에 stdout 읽기, 입력, resize, 종료를 같은 blocking 작업 경로에 몰아넣지 않는다. handle 정리 순서·출력 drain·정상 종료·강제 종료를 각각 테스트한다. [S03]

ConPTY 출력은 터미널용 스트림이다. 일반 subprocess처럼 stdout/stderr가 완전히 분리되어 있다고 모델링하지 않는다.

Agent는 각 PTY 출력을 계속 소비한다. 원격 시청자가 없어도 자식 프로세스가 네트워크 ACK를 기다리게 만들지 않는다.

### 8.4 상태 머신

```text
created -> starting -> running -> exited -> archived
                      |   |
                      |   +-> closing -> exited
                      +-> lost
```

접속 상태는 별도로 `connected / reconnecting / disconnected / revoked`다. `disconnected`를 `exited`로 처리하지 않는다.

PTY의 `running`은 Codex가 추론 중이라는 뜻이 아니다. AI 작업 상태는 `unknown / output_active / awaiting_input_verified / finished_verified / failed_verified`로 별도 표현하며 근거 없는 상태는 `unknown`이다.

### 8.5 입력 방식

- 터미널 직접 모드: 실제 키·붙여넣기·방향키·Ctrl+C·Tab 등을 PTY에 전달한다.
- 명령 작성 모드: 여러 줄을 작성 후 전송한다. 전송 전에 대상 세션·현재 실행 모드를 표시한다.
- 한글 IME는 composition 중 Enter를 전송으로 처리하지 않는다. `compositionend` 후 최종 문자열을 1회만 전송한다.
- 모바일 작성창에서 아직 보내지 않은 글은 화면을 바꿔도 보존할 수 있으나 비밀 입력은 저장하지 않는다.
- bracketed paste는 지원/활성 모드를 확인한다. 여러 줄 붙여넣기와 Enter를 무조건 같은 방식으로 처리하지 않는다.

일반 셸에 자연어를 잘못 보내는 사고를 막는다. 관리형 Codex 프로필은 실행 중인 프로그램 identity를 추적하고, 종료 후 명령 작성창을 비활성화한다. 일반 CMD/PowerShell에서 수동으로 실행한 프로그램의 준비 상태가 불명확하면 “AI에게 보내기”를 임의로 활성화하지 않는다. 직접 터미널 입력은 명확한 사용자 조작으로 계속 제공한다.

### 8.6 Ctrl+C와 종료

`Ctrl+C 전송`, `현재 프로그램 중단`, `터미널 종료`, `Agent 종료`를 같은 버튼으로 합치지 않는다. 전자는 터미널 입력이며 항상 OS 프로세스 강제 종료가 아니다. 대상이 반응하지 않는 경우에만 별도 경고 후 해당 관리 프로세스 트리의 종료를 제공한다.

---

## 9. 단일 작성자와 여러 기기의 동시 접속

### 9.1 세션별 입력 lease

모든 승인된 읽기 클라이언트는 동시에 같은 세션을 볼 수 있다. **한 PTY의 작성자는 한 번에 하나**다. 회사 UI도 이 규칙을 따른다.

lease 필드: `session_id`, `client_id`, `lease_id`, `lease_epoch`, `expires_at`. 기본 TTL 15초, 활성 화면에서 5초마다 갱신한다. 매 키 입력마다 인증 DB를 조회하지 않고 유효한 메모리 상태와 권한 generation을 확인한다.

다른 클라이언트가 입력하려면 `제어권 가져오기`를 누른다. 기존 작성자에게 표시하고 서버가 원자적으로 변경한다. 이전 lease의 입력은 거부한다. 회사 로컬 사용자는 즉시 회수할 수 있다.

입력창 작성과 실제 전송을 구분한다. lease가 없는 동안 초안을 작성할 수 있지만 몰래 전송하지 않는다. 오래된 입력을 lease 획득 후 자동 실행하지 않는다.

### 9.2 크기와 탭 전환

한 PTY는 하나의 논리적 `cols × rows`를 갖는다. 회사 160열과 모바일 50열을 동시에 서로 다른 실제 PTY 크기로 유지할 수 있다고 약속하지 않는다.

- 작성자 또는 명시적으로 지정된 layout owner만 PTY resize 가능.
- 읽기 클라이언트는 확대/축소·가로 스크롤·viewport clipping으로 표시한다.
- 모바일로 보기만 했다는 이유로 회사 터미널을 50열로 바꾸지 않는다.
- `모바일 너비 적용`은 명시적 조작이며 모든 화면이 영향을 받는다고 알린다.
- resize는 50~100ms 범위에서 병합 가능하지만 키 입력에는 같은 debounce를 적용하지 않는다.
- 크기가 같은 resize 요청은 무시한다. 출력과 resize의 적용 순서를 서버에서 기록한다.
- 탭 전환은 세션 전환이지 프로세스 생성이 아니다. 활성 탭만 고빈도로 렌더링한다.

---

## 10. 출력 스트리밍·복원·흐름 제어

### 10.1 데이터 경로

```text
입력: Client input -> 인증된 control WS -> 입력 큐 -> PTY writer
출력: PTY reader -> 순서 부여 -> TerminalModel -> bounded ring -> 구독자
                                                     |
                                         활성 클라이언트만 실시간 렌더
```

입력 control WS와 대량 출력/프리뷰/영상 스트림을 분리한다. 다른 탭의 대량 로그가 현재 입력 요청 앞에 쌓이지 않게 한다. 영상 큐·DB·로그 출력 때문에 PTY 입력 큐를 기다리게 하지 않는다.

### 10.2 순서와 generation

- `agent_epoch`: Agent 시작마다 새 무작위 식별자.
- `session_id`: 저장 가능한 세션 UUID.
- `generation`: 같은 세션 항목에서 프로세스를 새로 띄울 때 증가.
- `output_seq`: 해당 generation의 출력/크기 변경 이벤트에 대한 단조 증가 순번. **byte offset이 아니라 이벤트 순번**이다.
- `byte_credit`: 흐름 제어용 처리 바이트 수. 이벤트 순번과 분리한다.
- 네트워크 도착 시각이 아니라 서버 이벤트 순서가 기준이다.

### 10.3 출력 binary envelope

P1부터 단순한 버전 있는 binary envelope를 사용한다. 원문 출력 대량 데이터를 JSON/base64로 재포장하지 않는다.

| 필드 | 크기 | 설명 |
|---|---:|---|
| magic | 4 bytes | `RCTM` |
| version | 1 byte | wire format version |
| kind | 1 byte | output / resize / snapshot metadata 등 |
| flags | 2 bytes | 예약 bit는 검증 |
| session_id | 16 bytes | UUID binary |
| generation | 4 bytes | unsigned, network byte order |
| output_seq | 8 bytes | unsigned, network byte order |
| payload_len | 4 bytes | frame 최대 길이 검증 |
| payload | 가변 | 원문 UTF-8/VT bytes 또는 kind별 payload |

고정 헤더는 40 bytes다. `agent_epoch`는 인증된 stream handshake에서 바인딩한다. 형식 변경은 version negotiation을 거치며 모르는 major는 명확히 거절한다. 하나의 WebSocket 메시지에 여러 레코드를 넣을 수 있지만 부분 레코드 처리 규칙을 고정한다.

기본 출력 chunk 상한 32KiB, 한 WS 메시지 상한 256KiB. snapshot은 별도 제한된 경로로 전송한다. 작은 입력은 control JSON에 문자열/제어 문자를 표현할 수 있으며, 붙여넣기에는 전용 chunk 경로를 사용한다.

### 10.4 입력 순서와 ACK

입력은 `input_id`, `input_seq`, `lease_epoch`, `generation`을 포함한다. 서버는 해당 살아 있는 Agent epoch에서 중복 `input_id`와 이전 순번을 재실행하지 않는다.

ACK를 구분한다.

- `accepted`: 검사 후 메모리 큐에 수락.
- `written`: PTY write가 완료됨. **Codex가 의미를 이해했거나 명령 실행을 완료했다는 뜻은 아님.**
- `rejected`: lease/권한/크기/세션 상태 등으로 거절.
- `delivery_unknown`: 연결/프로세스 종료로 전달 여부를 확인할 수 없음.

OS PTY write와 DB를 하나의 원자적 트랜잭션으로 묶을 수 있다고 가정하지 않는다. Agent 충돌을 가로지르는 exactly-once 명령 실행을 약속하지 않는다. ACK를 못 받았다는 이유로 Enter·삭제 명령·붙여넣기를 자동 재전송하지 않는다.

### 10.5 화면 상태 복구

**최근 문자열만 다시 붙이는 방식으로는 TUI 상태 복구가 되지 않는다.** 서버의 TerminalModel이 화면 상태를 유지하고, 재접속 클라이언트는 snapshot + 이후 이벤트로 복원한다.

snapshot 최소 범위:

- 기본/alternate 화면의 셀, 색상, SGR 속성, 지원 범위의 링크.
- 커서 위치/표시/스타일, 저장 커서, scroll region, 탭 정지점.
- 줄바꿈/자동 줄바꿈/원점 모드, application cursor/keypad, bracketed paste, focus/mouse 모드.
- 화면 크기, terminal profile, 적용 완료 output_seq.
- 스트림의 미완성 UTF-8/VT 파싱 상태를 안전하게 다루기 위한 경계 정보.
- 제한된 scrollback와 잘린 이력 여부.

xterm의 비공개 내부 객체를 임의로 덮어쓰는 복원을 기본으로 하지 않는다. 선택한 Rust engine이 제공하는 상태에서 검증된 VT snapshot 또는 명시적 호환 adapter로 복원한다. 지원하지 않는 기능은 capability로 제한한다.

P0에서 후보 엔진 두 개를 Codex TUI golden stream으로 비교해 **하나만 채택**한다. 평가 항목은 snapshot 충실도, xterm 재현성, 한글/emoji 폭, 메모리, 출력 처리량, 유지보수·라이선스다. 라이브러리를 채택했다고 전체 xterm 프로토콜 호환을 선언하지 않는다.

복원 순서:

1. 구독 시작 시 서버가 순서 S까지 일관된 snapshot을 만든다.
2. snapshot 준비 이후의 이벤트는 bounded tail queue에 보관한다.
3. 클라이언트는 실제 터미널 크기를 맞추고 snapshot을 적용한다.
4. S 이후 이벤트를 순서대로 적용한다.
5. `live` 상태를 표시한다. 복원 중 사용자 입력은 기본 비활성화한다.
6. 복구 큐가 넘치면 더 최신 snapshot으로 다시 시작한다. 중간 이벤트를 누락하고 정상 복원이라고 표시하지 않는다.

재접속이 빠르고 클라이언트가 동일 상태를 보유하며 ring 범위가 충분하면 마지막 적용 순번 이후만 재전송할 수 있다. 새 페이지·다른 기기·상태가 의심되는 경우에는 snapshot을 사용한다.

### 10.6 터미널 질의 응답 소유자

DA/DSR 등의 터미널 질의에 회사·집·모바일 xterm이 모두 응답하면 입력 스트림이 오염될 수 있다. 질의 응답은 서버 TerminalModel 한 곳에서 생성한다.

지원하는 질의 종류와 응답을 명시하고 클라이언트 자동 응답이 PTY로 중복 전달되지 않도록 adapter/parser hook을 구현한다. snapshot 재생 중 발생한 응답·focus 보고를 실제 사용자 입력처럼 보내지 않는다. 클라이언트가 없어도 지원 질의에 응답할 수 있어야 한다. 광고한 terminal profile 밖의 기능은 조용히 오동작시키지 않고 호환성 테스트에 드러낸다.

### 10.7 버퍼와 느린 클라이언트

| 항목 | 초기 기본값 | 의미 |
|---|---:|---|
| 서버 원문 ring | 세션당 최대 8MiB, lazy allocation | 최신 이벤트 재전송용 |
| 서버 TerminalModel scrollback | 최대 2,000줄, 추가 메모리 상한 적용 | 현재 화면과 최근 이력 |
| 클라이언트 적용 대기 high watermark | 256KiB | 이 이상 새 출력 전송 속도 제한 |
| low watermark | 64KiB | credit 재개 |
| 구독자별 전송 대기 | 최대 1MiB | 초과 시 snapshot 재동기화 |
| 기본 paste 크기 | 최대 256KiB | 큰 paste는 별도 확인, 절대 상한 1MiB |
| 사용자 입력 chunk | 최대 8KiB | 긴 paste를 작은 청크로 분리 |
| 출력 병합 지연 | 최대 4ms 목표 | 제어 입력은 이 타이머를 기다리지 않음 |

xterm의 `write`는 비동기 버퍼링되므로 호출 완료를 화면 표시 완료로 보지 않는다. 적용 완료 callback을 이용한 credit/ACK 흐름 제어를 구현한다. [S07]

느린 모바일 하나 때문에 모든 PTY와 데스크톱 클라이언트가 멈추면 안 된다. 서버 모델은 계속 최신 상태를 유지하고, 느린 구독자는 개별적으로 재동기화한다.

다만 무한 출력 생산자보다 항상 빨리 처리할 수 있다고 약속하지 않는다. 서버 terminal parser 자체가 한계를 넘는 경우 bounded queue와 OS pipe의 자연스러운 backpressure를 사용한다. 메모리 무한 증가·VT 중간 잘라내기 대신 `과도한 출력` 상태와 중단 경로를 제공한다.

### 10.8 한글·문자 폭

UTF-8 bytes가 네트워크 chunk 경계에서 나뉘어도 손실 없이 복원한다. 한글 IME·한글 경로·CJK·emoji·조합문자·서로게이트 쌍을 테스트한다. 문자 수와 byte 길이를 혼동하지 않는다. 서버/클라이언트 Unicode 폭 버전을 맞추고 불일치 프로필은 지원 범위를 명시한다.

---

## 11. localhost 프리뷰

### 11.1 서비스 탐색과 승인

터미널의 `localhost:3000` 문자열은 후보 발견 수단이지 공개 허가가 아니다. 해당 프로젝트의 관리 프로세스 또는 사용자가 지정한 포트를 확인하고 사용자가 승인해야 활성화한다.

네트워크 포트 목록을 전체 PC에서 초당 여러 번 스캔하지 않는다. 기본은 명시 등록 + 출력 후보 + 필요할 때만 OS listener 확인이다. PID와 생성 시각, executable, 포트 바인딩을 기록한다. 포트를 다른 프로세스가 재사용하면 기존 허가를 재검토한다.

업스트림은 숫자로 고정한 `127.0.0.1` 또는 검증된 `::1`만 허용한다. 사용자가 요청 URL에 임의 외부 주소·회사 내부망·메타데이터 IP를 넣어 호출하는 범용 프록시는 만들지 않는다.

### 11.2 origin 분리: 기본 경로

`/ports/3000`를 관리 UI와 같은 origin에 붙이는 것을 기본으로 하지 않는다. 프리뷰 페이지의 JavaScript가 관리 API에 접근하거나 `/` 기반 자원 경로가 깨지는 문제를 피한다.

```text
관리: https://office-pc.<tailnet>.ts.net/
프리뷰 A: https://office-pc.<tailnet>.ts.net:8444/
프리뷰 B: https://office-pc.<tailnet>.ts.net:8445/

Serve 8444 -> loopback PreviewGateway A -> 127.0.0.1:3000
Serve 8445 -> loopback PreviewGateway B -> 127.0.0.1:5173
```

외부 포트가 다르면 별도 origin이다. 그러나 **쿠키는 포트별로 격리되는 저장소가 아니다.** 따라서 관리 인증은 공용 쿠키에 의존하지 않고, 프리뷰 Gateway도 쿠키/권한 경계를 별도로 구현한다. [S18][S19]

### 11.3 프리뷰 인증과 쿠키

프리뷰는 tailnet에 있다는 이유만으로 무인증 공개하지 않는다.

- 관리 API에서 `preview_id`, client, grant, expiry에 바인딩된 30초·1회용 launch ticket을 발급한다.
- 신뢰된 프리뷰 bootstrap 페이지로 ticket을 전달해 소비한 뒤, 해당 preview에 한정된 HttpOnly/Secure 세션을 발급한다. ticket은 URL query로 저장하지 않고 fragment/명시적 POST 등으로 한 번만 전달 후 제거한다.
- Gateway는 preview session과 등록된 upstream을 검사한다. 다른 프리뷰에서 받은 credential은 사용할 수 없다.
- 관리 토큰·bootstrap 비밀·Gateway 예약 쿠키는 upstream에 전달하지 않는다.
- 같은 호스트의 여러 포트를 사용하는 기본안에서는 upstream Set-Cookie 이름을 preview별 namespace로 매핑하고, downstream 요청을 해당 upstream 이름으로 역변환하는 cookie jar를 둔다.
- upstream의 Domain/Path/Secure/SameSite 처리와 reserved name 충돌을 테스트한다. 이전 preview cookie·service worker가 다음 프로젝트에 재사용되지 않도록 slot 재사용 정책을 둔다.
- `document.cookie`에 직접 의존하는 인증/라이브러리는 prefix 매핑과 호환되지 않을 수 있다. 이 경우 “완전 호환”을 주장하지 않고, 별도 hostname 모드 또는 지원 제한을 안내한다.

**기본 포트 분리 모드의 신뢰 범위:** 같은 호스트의 여러 프로젝트는 서로 신뢰하는 개인 개발 앱이라는 전제다. namespace는 잘못된 upstream cookie 전달을 줄일 뿐, 서로 적대적인 프리뷰의 script-readable cookie를 강하게 격리하는 해결책이 아니다. 신뢰하지 않는 앱을 함께 실행하거나 서로 다른 사용자의 프로젝트를 서비스할 때는 별도 hostname 모드를 필수로 하고, 구성하지 못하면 동시 제공을 거절한다. 이 제한은 관리 콘솔의 bearer/origin 경계를 완화하지 않는다.

P3 기본 모드에서는 별도 제한 CSP의 `worker-src 'none'` 등으로 프리뷰 service worker 설치를 차단하고, 이에 의존하는 앱은 제한을 표시한다. 브라우저 정책으로 차단 가능한 범위는 실제 검증한다. 이미 등록된 worker가 있을 수 있는 origin을 다른 프로젝트에 자동 재배정하지 않는다. slot은 프로젝트에 바인딩하고, 모든 관련 클라이언트의 저장소 정리가 확인되지 않으면 새로 승인한 origin을 사용한다. 단순 서버 재시작으로 브라우저 저장소가 지워졌다고 가정하지 않는다.

더 강한 웹앱 호환/격리는 프리뷰마다 별도 hostname과 TLS를 배정하는 확장 모드다. 이는 추가 DNS/인증서 구성이 필요할 수 있으므로 기본 설치에 몰래 요구하지 않는다. 코드만으로 브라우저의 cookie origin 규칙을 없앨 수 있다고 가정하지 않는다.

### 11.4 HTTP·HMR·스트리밍

- 일반 요청/응답을 streaming proxy로 처리한다. 대용량 응답 전체를 RAM에 적재하지 않는다.
- WebSocket upgrade, SSE, chunked response, multipart, range 요청을 테스트한다.
- Host/Origin/X-Forwarded-*는 등록된 adapter 정책에 따라 처리한다. 모든 Origin을 허용하도록 upstream 보안을 무조건 해제하지 않는다.
- `Connection` 등 hop-by-hop 헤더를 정리하고 WebSocket handshake는 별도로 처리한다.
- Vite/Next.js용 작은 호환 adapter를 둔다. 정확한 설정 키는 설치된 개발 서버 버전에서 확인한다.
- Vite는 프록시의 WebSocket 지원과 명시적인 allowed host가 중요하다. `allowedHosts: true`, `cors: true`를 만능 해결책으로 사용하지 않는다. [S17]
- asset `/...`와 refresh routing이 유지되도록 루트 경로 프리뷰를 기본으로 한다.
- absolute `http://localhost:...` API/WS URL은 집 PC를 가리킬 수 있다. 이를 무조건 임의 문자열 치환하지 않는다. 프로젝트의 개발 설정을 제안하고 사용자 동의 후 적용한다.

### 11.5 로그인·보안 헤더·UI

OAuth callback, Secure cookie, CSP, X-Frame-Options, service worker는 앱별 호환 테스트 대상이다. iframe이 차단되면 새 브라우저 탭을 기본 대안으로 제공하며 upstream 보안 헤더를 임의로 제거하지 않는다.

Tauri 내부 프리뷰를 제공할 때도 privileged IPC를 노출하지 않는 별도 webview/origin을 사용한다. 통과하지 못하면 외부 브라우저로 연다. 기본 프리뷰는 “새 탭으로 열기”, 인앱 임베딩은 호환 확인 후 제공한다.

등록되지 않은 포트, DB·SSH 같은 비HTTP 프로토콜, Agent 자기 자신의 관리 포트로의 순환 프록시는 거부한다. 승인 취소 시 요청과 WS를 모두 닫는다.

---

## 12. Windows 창 미리보기: P4

### 12.1 목표

회사 PC에서 실행 중인 승인된 EXE의 실제 창을 집에서 **보기 전용**으로 확인한다. 웹 프리뷰로 EXE가 표시되는 척하거나 이미지 한 장을 실시간 영상이라고 표시하지 않는다.

Windows.Graphics.Capture는 창/디스플레이 프레임 획득을 제공한다. 지원 여부와 캡처 대상의 수명, 장치 변경을 처리해야 한다. [S09]

### 12.2 캡처 대상 등록

- 사용자가 로컬에서 고른 창, 또는 사전에 승인한 프로젝트 실행 경로의 관리 앱을 후보로 제공한다.
- `process_id + creation_time + window_handle + source_generation`으로 추적한다.
- “Codex가 실행한 창”만으로 항상 완전하게 추적할 수 있다고 가정하지 않는다. launcher·자식 프로세스·별도 브라우저·공용 프로세스 앱을 고려한다.
- 기본적으로 승인되지 않은 Outlook/메신저/다른 프로젝트 창을 캡처 후보에 자동 공개하지 않는다.
- 등록된 앱이 새 modal/파일 선택 창을 열면 같은 승인 범위인지 확인하고 별도 source로 처리한다.
- 재빌드로 binary가 바뀌는 프로젝트는 허용된 build 경로 정책과 재실행 확인을 사용한다. 임의 경로 바꿔치기는 허용하지 않는다.

### 12.3 파이프라인

```text
Windows.Graphics.Capture
 -> D3D11 texture
 -> crop/scale/color conversion
 -> 지원되는 하드웨어 H.264 encoder
 -> WebRTC media transport
 -> 브라우저/Tauri video decoder
```

`str0m`은 WebRTC 전달 계층 후보이며 H.264 인코더가 아니다. Media Foundation 인코딩과 프레임/패킷 연동은 별도로 검증한다. [S15][S16]

CPU로 매 프레임 스크린샷 → PNG → base64 → Tauri IPC를 거치는 경로를 정상 영상 모드로 만들지 않는다. GPU surface 유지와 복사 횟수 감소를 우선하되, “zero-copy”는 실제 측정·경로 검증 없이는 주장하지 않는다.

### 12.4 on-demand 자원 관리

- 시청자가 0명이면 capture/encode 루프는 0개다.
- 마지막 구독자 종료 시 2초 내 캡처·인코더·GPU surface 해제, 10초 내 MediaHelper 종료를 목표로 한다.
- 기본 동시 영상 source는 호스트당 1개다. 여러 창은 목록에 유지하되 선택한 창만 전송한다.
- 기본 프로필은 최대 1280×720, 15fps. 사용자가 품질을 올릴 때 1920×1080, 30fps를 지원 목표로 한다.
- 정적 화면은 낮은 전송률로 내려가고, 변화 시 프레임률을 복구한다. 바쁜 polling으로 정지 화면을 계속 인코딩하지 않는다.
- queue는 최신 프레임 1~2개를 유지한다. 오래된 화면을 계속 재생해 지연이 누적되지 않게 한다.
- 하드웨어 인코더가 없거나 제한되면 낮은 해상도/fps의 소프트웨어 경로를 명시적으로 선택하거나 보기 전용 저속 모드로 전환한다.

### 12.5 실패 상태

`unsupported`, `awaiting_permission`, `live`, `minimized_or_stalled`, `locked`, `source_closed`, `device_lost`, `network_degraded`를 구분한다.

최소화·화면 잠금·GPU 드라이버·DRM/보호 콘텐츠에서 항상 정상 캡처된다고 가정하지 않는다. 새 프레임이 오지 않으면 마지막 프레임에 갱신 시각과 중지 상태를 표시한다. 사용자의 모르게 창을 계속 복원하거나 잠금 상태를 해제하지 않는다.

보호 화면을 검은 화면으로 받았을 때 실제 앱이 검다는 식으로 오표시하지 않는다. 캡처 지원 정보가 없으면 `확인 불가`로 표시한다.

### 12.6 미디어 연결

시그널링은 기존 인증된 control API로 전달한다. WebRTC media와 터미널 WS를 분리한다. 네트워크는 승인된 Tailscale 경로를 사용하도록 후보/바인딩 정책을 검증한다.

브라우저 ICE·mDNS·tailnet 가상 인터페이스와 Windows 방화벽 조합을 실제 두 PC에서 테스트한다. 외부 STUN/TURN 서비스를 조용히 추가하지 않는다. WebRTC 실패 시 터미널을 중단하지 않는다. [S10]

대체 저속 이미지 모드는 1~5fps 범위의 보기 전용이며, 정상 저지연 원격조작 품질로 광고하지 않는다. 항상 현재 모드와 지연 상태를 표시한다.

---

## 13. 창 마우스·키보드 제어: P5

### 13.1 보기와 제어 분리

`window.view`가 있어도 바로 입력을 보낼 수 없다. `window.control`과 유효한 GUI lease가 모두 필요하다. GUI lease는 특정 창뿐 아니라 **회사 Windows 데스크톱 전체의 입력 자원 기준으로 한 개**다. 여러 창을 다른 사람이 동시에 제어하게 하지 않는다.

회사 로컬 사용자는 트레이의 `원격 입력 중지`와 등록 가능한 긴급 단축키로 즉시 회수할 수 있다. 사용자 동의 없이 로컬 입력을 차단하지 않는다.

### 13.2 좌표와 프레임 정합성

클라이언트는 영상 letterbox를 제외한 normalized 좌표와 `source_generation`, `geometry_version`을 보낸다. Agent는 캡처 source의 실제 좌표·창 rect·클라이언트 영역·DPI 배율을 적용한다.

테스트 범위: 100/125/150/200% DPI, 창 이동/크기 변경, 모니터 사이 이동, 음수 desktop 좌표, 최소화/복원, 브라우저 확대, 해상도 변경.

오래된 geometry의 클릭은 거절한다. 새 geometry와 첫 정상 프레임을 확인하기 전에는 클릭을 잠시 비활성화한다. 정지된 영상에 클릭이 계속 전달되지 않게 한다.

### 13.3 실제 Windows 입력의 제한

`SendInput` 계열 입력은 Windows 입력 스트림과 무결성 수준 제한을 받는다. 높은 권한의 앱에 일반 권한 앱이 자유롭게 입력할 수 있다고 가정하지 않는다. [S11]

입력 직전 대상 HWND·process identity·foreground를 확인한다. foreground 전환이 허용되지 않거나 다른 창으로 바뀌었으면 입력을 중단하고 알린다.

**중요한 한계:** 선택한 창만 영상으로 보여도, 일반 Windows 입력 전달은 완전한 창 샌드박스가 아니다. focus 경쟁과 다른 앱의 창 전환 때문에 이론적으로 다른 창에 입력될 가능성을 절대 0으로 만들었다고 주장하지 않는다. 로컬 사용 중에는 원격 GUI 입력을 정지하는 정책을 기본으로 제공하고, 창 단위 제어가 실제 회사 PC의 마우스/포커스에 영향을 준다고 표시한다.

### 13.4 키 입력 처리

- key down/up, scan code, 텍스트 입력을 구분한다. 한국어 텍스트 입력과 게임식 raw key 입력을 섞지 않는다.
- 클라이언트 modifier 상태와 서버가 주입한 pressed-key 집합을 관리한다.
- 연결 종료·lease 철회·focus 상실·source 종료 시 **이 제품이 주입한** 키/버튼을 release한다.
- 마우스 이동은 최신 값으로 병합할 수 있지만 click, down/up, Enter, modifier 변경을 임의로 drop하지 않는다.
- Ctrl/Alt/Shift가 눌린 채 고착되는 것을 필수 결함으로 취급한다.
- 브라우저/OS가 가로채는 단축키는 화면 버튼 등으로 가능한 범위를 제공한다. 모든 시스템 단축키 전달을 약속하지 않는다.
- 클립보드 동기화는 기본 OFF. 추가 시 명시적 단건 복사/붙여넣기와 크기 제한부터 제공한다.

### 13.5 제한 대상

UAC secure desktop, Windows 로그인·잠금 화면, Ctrl+Alt+Del, 보호 콘텐츠, 더 높은 무결성 앱은 기본 지원 대상이 아니다. 관리자 권한 확인을 우회하거나 UAC/백신/회사 정책을 끄지 않는다.

설치 마법사도 일반 권한 창까지 검증할 수 있지만, 관리자 승인 단계까지 집에서 반드시 완료할 수 있다고 안내하지 않는다.

---

## 14. 전체 Remote Desktop: P6

### 14.1 범위

P6는 전체 데스크톱 기능을 실제로 포함한다. 단, **현재 로그인한 일반 사용자 데스크톱**의 화면 공유·마우스/키보드 제어 범위다. RDP 서버를 새로 구현하거나 Windows 로그인 세션을 생성하는 것은 아니다.

기존 WGC 파이프라인을 모니터 source로 확장한다. 필요하면 Desktop Duplication API를 비교한다. 후자는 데스크톱 프레임·변경 영역 정보를 제공하는 별도 캡처 경로이므로 호환성/성능 검증 후 선택한다. [S12]

### 14.2 기능

- 전체 모니터 보기, 모니터 목록, 한 모니터씩 빠른 전환.
- 기본 1개 모니터 스트림. 여러 모니터 동시 전송은 별도 품질·자원 예산 승인 후 추가.
- 보기 전용/조작 가능 상태를 명확히 표시.
- 모니터 좌표·배율·회전·해상도 변경 처리.
- Terminal/Web/Apps/Desktop UI 간 전환 가능. Desktop에서 나가면 영상 구독을 정리한다.
- Apps/Desktop은 동일한 GUI 입력 lease를 사용한다. Terminal lease는 PTY별 별개다.
- 전체 화면에는 다른 업무 정보가 노출될 수 있음을 허용 전에 알린다.

### 14.3 비포함 및 실패 처리

로그인 이전 원격 접속, 자동 잠금 해제, 서비스의 Session 0 GUI 조작, secure desktop, 원격 장치 리다이렉션은 포함하지 않는다. Windows 사용자 잠금이 감지되면 영상·GUI 입력을 일시 중지한다. 터미널은 사전 승인한 잠금 중 접근 정책에 따라 유지한다.

P6가 켜져도 P1~P3의 가벼운 모드가 기본값이다. 앱 시작할 때 데스크톱 캡처나 인코더를 미리 실행하지 않는다.

---

## 15. 모바일 확장 설계

### 15.1 모바일은 별도의 제품 경험

모바일 기본 화면은 `호스트 상태 → 세션 목록 → CLI 출력 → 작성창`이다. Happy에서 참고할 부분은 세션 접근, 입력 편의, 상태 표현이며, 기존 실행 세션을 새 AI 세션으로 바꾸는 구조를 따라갈 필요는 없다. Happy의 실제 소스 재사용은 해당 파일·커밋·라이선스를 확인한 경우에만 한다. [S24]

모바일 기본 capability는 `terminal.read`, 승인된 `terminal.write`, 선택적 `preview.read`다. GUI/데스크톱 권한은 모바일이라는 이유만으로 자동 부여하지 않는다.

### 15.2 최소 UI 계약

```text
OFFICE-PC  연결됨                   [세션]
Sales / Codex             [제어권 가져오기]
------------------------------------------------
최근 CLI 출력
• 실제 출력 또는 검증된 상태
• 원문 보기

[아래 새 출력 12개]
------------------------------------------------
메시지를 입력하세요...
[Ctrl] [Esc] [Tab] [↑] [↓]  [전송]
```

- 기본 세로 1열, 터치 영역 최소 44 CSS px 목표.
- 동시 고빈도 구독은 현재 세션 하나. 나머지는 가벼운 상태 이벤트만 받는다.
- 한글 IME·가상키보드·safe-area·화면 회전·VisualViewport 변화를 처리한다.
- Enter 기본 동작과 전송 버튼은 구분한다. 조합 중 Enter는 전송하지 않는다.
- 위로 스크롤해 읽는 중에는 최신 출력으로 강제 점프하지 않는다.
- 작은 화면에서는 터미널 논리적 열 수를 유지한 가로 이동/확대 보기를 제공한다.
- 전체 터미널과 `읽기 쉬운 출력`을 전환할 수 있다. 후자가 TUI를 완벽히 변환할 수 없으면 원문으로 안내한다.
- 연결이 끊긴 상태에서 쓴 문장은 초안이다. 재연결 즉시 몰래 전송하지 않는다.

### 15.3 출력 표시와 AI 의미 해석

초기 모바일은 실제 PTY 출력의 충실한 표시가 기준이다. ANSI 화면을 단순 줄 로그로 바꿔 진행바·반복 렌더·alternate screen을 대화 메시지처럼 쌓지 않는다.

읽기 쉬운 모드는 TerminalModel의 제한된 line projection 또는 공식 구조화 이벤트가 검증된 adapter에서만 만든다. 모든 항목에 `terminal_raw`, `terminal_projection`, `adapter_verified` 출처를 구분한다.

Codex의 App Server 같은 구조화 인터페이스는 후속 adapter 후보이지 기존 TUI에 자동으로 붙는 관찰 API라고 가정하지 않는다. 실제 설치 버전의 지원 범위와 실행 세션 소유권을 확인한다. PTY 모드와 구조화 모드가 같은 Codex thread를 동시에 쓰지 않게 한다. [S21][S22]

### 15.4 모바일 배포 순서

1. P1: API에 플랫폼 의존성을 넣지 않고 반응형 레이아웃 기반을 만든다.
2. P2: Android Chrome / iOS Safari에서 실제 키보드·재접속을 검증한다.
3. P3 이후: HTTPS 웹 UI를 PWA로 제공한다. service worker에는 UI 정적 자원만 캐시한다.
4. 이후: 동일 프로토콜을 쓰는 Tauri 모바일 또는 플랫폼 앱을 검토한다. 데스크톱 Win32 코드를 모바일 빌드에 묶지 않는다.

PWA·모바일 앱은 회사 Agent가 계속 살아 있는 것과 별개다. 모바일 OS가 백그라운드 연결을 중단할 수 있음을 전제로 복원한다. 상시 WebSocket이나 정확한 백그라운드 실행 간격을 약속하지 않는다.

푸시 알림은 별도 선택 기능이다. APNs/FCM/Web Push 등 외부 전달 인프라와 추가 설정이 필요할 수 있으므로 “중앙 서버 없는 기본판의 필수 기능”에 넣지 않는다. 기본판은 재접속 시 확인하며, 푸시 추가 시 원문 코드/로그를 notification payload에 넣지 않는다.

### 15.5 capability negotiation

클라이언트는 다음을 선언한다.

```json
{
  "protocol_major": 1,
  "client_kind": "mobile_web",
  "supports": ["terminal.raw", "terminal.compose", "preview.external"],
  "viewport": {"width_css": 390, "height_css": 844},
  "max_active_terminal_streams": 1,
  "prefers_reduced_motion": true
}
```

서버는 실제 권한과 지원 기능의 교집합만 제공한다. 화면이 작다는 이유로 새로운 세션을 생성하거나 실행 위치를 바꾸지 않는다.

---

## 16. 데스크톱 UI/UX 계약

### 16.1 기본 배치

왼쪽: 호스트/프로젝트/터미널 목록. 중앙: 선택된 터미널. 상단: `Terminal | Web | Apps | Desktop`. 하단: 연결 상태, 제어권, 실제 출력/성능 상태. 회사 Tauri와 집 웹의 주요 레이아웃은 동일하게 유지한다.

프로젝트 중심으로 묶되 탭의 단위는 실제 세션이다. tab title은 사용자가 바꿀 수 있고 서로 같은 이름이어도 내부 session_id는 구분한다.

### 16.2 명확히 구분할 상태

- 호스트 연결 vs 터미널 프로세스 실행.
- 터미널 출력 발생 vs Codex의 실제 추론/작업 완료.
- 원격 입력 수락 vs PTY 전달 완료 vs 작업 실행 완료.
- 읽기 전용 vs 입력 제어 중.
- 실시간 영상 vs 마지막 프레임 vs 저속 대체 모드.
- 터미널 화면 복원 vs 새 프로세스 실행.

### 16.3 UI 성능 규칙

xterm 인스턴스는 프레임워크 state의 매 문자 업데이트에 매달리지 않는다. DOM 리스트에 로그 전체를 쌓지 않는다. 터미널 데이터는 전용 adapter 경로로 직접 전달하고, UI state는 label/unread/status처럼 작은 메타데이터에만 사용한다.

활성 xterm만 고빈도 처리한다. 비활성 세션은 상태/최신 sequence를 기억하고 다시 열 때 snapshot을 적용한다. 이미 켜져 있는 다른 프로젝트의 TUI 렌더를 백그라운드에서 모두 60fps로 돌리지 않는다.

UI 숨김, preview 닫힘, 모바일 background에서 animation/requestAnimationFrame/render loop를 정리한다. WebGL context lost는 터미널 재시작이 아니라 renderer 복구/대체로 처리한다.

### 16.4 사용자 보호

위험한 조작을 숨기지 않는다. `이 터미널 종료`, `전체 원격 차단`, `Agent 종료`, `장치 승인 취소`는 서로 다른 명령과 확인 문구를 사용한다. 탭을 바꾼 직후 오래된 작성창 문장을 다른 세션으로 보내지 않도록 draft를 session_id에 바인딩한다.

---

## 17. 성능 예산: 필수 출시 판정 기준

### 17.1 해석

“앱 자체의 버벅임 없음”을 **측정 가능한 지연·버퍼·자원 기준**으로 정의한다. 모든 하드웨어·부하·네트워크에서 0ms/0%/0MiB를 보장한다는 문구를 쓰지 않는다.

다음은 구현 전 목표값이다. P0에서 기준 장비를 확정하고, P1부터 같은 장비·조건으로 반복 측정한다. 기준을 넘으면 원인·개선 결과를 남기고 해당 단계 출시를 차단한다. 테스트 결과를 보고 임의로 기준을 완화하지 않는다.

### 17.2 기준 환경

Windows 11 x64의 지원 중인 빌드, 4코어/8논리 CPU 이상, RAM 16GiB, SSD, 내장 GPU 기준. 정확한 CPU 모델·Windows build·GPU driver·전원 모드·WebView2/브라우저 버전을 결과에 기록한다. 별도 고성능 외장 GPU를 기본 요구로 하지 않는다.

Release 빌드, debug logging OFF, 백신 기본 상태, 정상 OS 전원 정책에서 측정한다. 네트워크와 무관한 기준은 loopback에서 먼저 측정한다. 그 뒤 실제 회사↔집 tailnet에서 측정한다.

### 17.3 자원 예산

CPU는 전체 머신 100%로 정규화한다. 예: 8논리 CPU에서 한 코어를 꽉 쓰면 약 12.5%로 계산한다. 메모리는 대상 프로세스들의 합산 private working set을 기본으로 하고 private bytes도 함께 기록한다. GPU memory는 별도 기록한다.

| 상황 | CPU 목표 | 메모리 목표 | 비고 |
|---|---:|---:|---|
| Agent 대기, PTY 0, UI/영상 닫힘 | 10분 평균 ≤0.3% | ≤70MiB | 트레이·API 포함 |
| PTY 2개 유휴, UI/영상 닫힘 | 10분 평균 ≤0.5% | Agent ≤110MiB | 셸/Codex는 별도 |
| PTY 8개 유휴, UI/영상 닫힘 | 10분 평균 ≤0.8% | Agent ≤220MiB | 버퍼 lazy allocation |
| 로컬 UI + 활성 터미널 1개 유휴 | 제품 합산 평균 ≤1.0% | Agent+UI+관련 WebView2 ≤350MiB | 전체 제품 프로세스 보고 |
| 텍스트 출력 총 100KiB/s, 활성 1개 | 제품 합산 평균 ≤3% | 안정 상태 ≤400MiB | 기본 터미널 프로필 |
| 720p/15fps 창 보기, HW encoder | 호스트 제품 평균 ≤5% | 호스트 제품 ≤450MiB | GPU/decoder 부하는 별도 표기 |
| 1080p/30fps 창/화면 보기 | 호스트 제품 평균 ≤8% | 호스트 제품 ≤550MiB | HW 지원 장비에서만 gate |

영상 표의 기본 호스트 조건은 회사 Tauri UI가 닫혀 있고 Agent+MediaHelper가 실행되는 경우다. 집 viewer의 CPU/RAM/GPU와, 회사 UI까지 동시에 연 경우의 합산 비용은 별도 시나리오로 반드시 측정한다. 비교 보고서에서 이 조건을 생략하지 않는다.

**제외하되 별도로 반드시 보고할 것:** 실제 Codex, CMD/PowerShell 자식, 컴파일러, 개발 서버, 실행한 EXE, Tailscale의 추가 부하. 제외는 사용자 체감 비용을 숨기기 위한 것이 아니다. 보고서는 제품/개발 작업/Tailscale/합계를 모두 보여준다.

미디어를 안 볼 때는 미디어 CPU가 0에 가깝고 MediaHelper 프로세스가 종료되는지를 별도 검사한다. working set 강제 trimming으로 수치만 낮추는 최적화는 금지한다.

### 17.4 지연 목표

| 측정 구간 | 정상 부하 목표 | 스트레스 목표 |
|---|---:|---:|
| 클라이언트 input event 처리 → 전송 enqueue | p95 ≤4ms | p99 ≤10ms |
| Agent input 수신 → PTY write 완료 | p95 ≤5ms, p99 ≤15ms | p95 ≤15ms, p99 ≤40ms |
| PTY output 수신 → 전송 enqueue | p95 ≤5ms | p95 ≤15ms |
| 클라이언트 output 수신 → parse/apply 완료 | p95 ≤16ms | p95 ≤33ms |
| loopback echo fixture의 왕복 화면 반영 | p95 ≤40ms, p99 ≤80ms | p95 ≤100ms |
| 열려 있던 탭 전환 후 사용 가능 | p95 ≤100ms | p95 ≤200ms |
| 1MiB 이하 snapshot 재접속 | loopback p95 ≤1초 | 저속 네트워크는 별도 보고 |
| Ctrl+C input을 PTY에 전달 | p95 ≤10ms | 출력 폭주 시 p95 ≤50ms |

`Ctrl+C 전달`과 대상 앱이 실제 중단되는 시점은 구분한다. 후자는 대상 프로세스 동작의 영향을 받는다.

네트워크 RTT, 대상 셸/Codex 처리시간, 앱 내부 처리시간을 분리한다. 동기화되지 않은 두 PC의 wall clock을 빼서 one-way 지연을 계산하지 않는다. 같은 프로세스의 monotonic clock 구간과 식별자가 있는 round-trip fixture를 사용한다.

xterm write callback은 파싱/적용 완료 지표다. 실제 픽셀 표시 지연은 렌더 이벤트·프레임 추적을 별도로 측정하며 callback만으로 화면 paint 완료를 주장하지 않는다. [S07]

### 17.5 스트레스 및 지속성

- 출력 1MiB/s를 10분 처리하고 입력·전환·메모리 상한을 확인한다.
- 5MiB/s 출력 burst를 5초 발생시킨다. 복구 후 정상 상태로 돌아와야 한다.
- 다른 PTY에서 출력 폭주 중 현재 탭 입력을 10,000회 측정한다.
- 영상 + 프리뷰 HMR + 2개 터미널을 동시에 사용해 입력 지연이 정상 조건 대비 p95 10ms 넘게 악화되지 않는지 검사한다.
- 전송 backlog는 표시한다. 평소 숨겨진 backlog가 수초로 누적되지 않게 한다.
- 네트워크 단절/재연결 100회, 세션 생성/종료 500회, UI 재시작 50회를 검사한다.
- 24시간 soak에서 PTY와 세션 ID 유지. 안정화 이후 private bytes 증가가 10MiB/시간 이상 지속되면 누수 조사 대상으로 출시 차단한다.
- 핸들·스레드·GPU surface·WS·timer 개수도 기록한다. 메모리 숫자 하나만 보고 통과 처리하지 않는다.

### 17.6 구현 최적화 원칙

이벤트 기반 I/O, 필요한 때만 캡처, bounded queue, 메시지 복사 최소화, binary 출력, 우선순위 입력, inactive tab render 중단, lazy allocation, 묶음 메타데이터 저장을 기본으로 한다.

정상 동작에서 전체 프로세스/포트/파일을 100ms마다 스캔하지 않는다. 지속적인 UI polling·초당 DB commit·실시간 LLM 요약·영상 base64·무제한 scrollback·백그라운드 모든 탭 렌더를 금지한다.

프로파일링으로 확인하지 않은 `unsafe`, 전역 realtime priority, 안티바이러스 해제, 시스템 타이머 해상도 강제 변경을 성능 해결책으로 쓰지 않는다.

---

## 18. API·프로토콜 계약

### 18.1 원칙

API major version은 URL과 handshake에 표시한다. 권한은 모든 mutation에서 서버가 검증한다. Rust protocol types를 단일 기준으로 JSON Schema/TypeScript 타입을 생성하거나, 동등한 단일 소스 방식을 ADR로 확정한다. 서로 다른 손작성 DTO가 점점 어긋나는 구조를 피한다.

TLS·Tailscale 설정과 독립적인 애플리케이션 프로토콜로 만들고, capability에 따라 PC/모바일 기능을 제한한다.

### 18.2 REST 자원

| API | 기능/권한 |
|---|---|
| `GET /healthz` | 최소 생존 정보. 프로젝트/세션/버전 상세 비공개 |
| `GET /v1/info` | 인증 후 기능·프로토콜·호스트 정보 |
| `POST /v1/auth/challenges` | rate limited 장치 challenge |
| `POST /v1/auth/verify` | 서명 검증, access token |
| `POST /v1/pairing/requests` | 새 장치 등록 요청 |
| `POST /v1/pairing/approvals` | 로컬 승인 또는 적절한 owner 권한 |
| `GET /v1/projects` | 승인된 프로젝트 목록 |
| `GET/POST /v1/sessions` | 세션 조회/생성 |
| `POST /v1/sessions/{id}/lease` | 제어권 획득/회수 |
| `POST /v1/sessions/{id}/close` | 명시적 세션 종료 |
| `GET /v1/sessions/{id}/snapshot` | 일관된 화면 복원 상태 |
| `POST /v1/stream-tickets` | 특정 WS용 일회용 ticket |
| `GET/POST /v1/previews` | 승인된 프리뷰 조회/등록 |
| `POST /v1/previews/{id}/launch` | 프리뷰 한정 launch ticket |
| `DELETE /v1/previews/{id}` | 승인/노출 철회 |
| `GET /v1/windows` | 승인된 창 후보만 반환 |
| `POST /v1/media/sessions` | 창/모니터 stream 생성 |
| `DELETE /v1/media/sessions/{id}` | stream 종료 |
| `POST /v1/gui/lease` | 단일 GUI 입력 제어권 |
| `POST /v1/devices/{id}/revoke` | 장치 철회 |
| `POST /v1/admin/block-remote` | 원격 연결 일괄 차단 |

파일 탐색·파일 전송·임의 HTTP proxy API는 단계 범위 밖이므로 편의상 노출하지 않는다.

### 18.3 WebSocket 채널

`/v1/control`: lease, 입력, 세션 이벤트, 미디어 signaling, 상태.

`/v1/terminal-stream`: 선택한 terminal output/snapshot 관련 스트림. 구독자별 credit와 상태를 가진다.

미디어는 WebRTC를 사용하며 대량 프레임을 control에 보내지 않는다. 프리뷰 HTTP/WS는 관리 stream과 별개다.

제어 메시지 예시:

```json
{
  "v": 1,
  "type": "terminal.input",
  "request_id": "req-uuid",
  "session_id": "session-uuid",
  "agent_epoch": "agent-epoch-uuid",
  "generation": 1,
  "lease_epoch": 3,
  "input_id": "input-uuid",
  "input_seq": 52,
  "payload": {"kind": "utf8", "text": "계속 진행해\r"}
}
```

여기서 paste의 원자성은 다른 일반 입력과 섞이지 않는 순서 보장을 뜻하며, 이미 실행한 문자를 되돌리는 all-or-nothing 실행 보장이 아니다. 가능한 한 제한된 크기의 paste를 commit 전에 검증·버퍼링하고, 실제 PTY 전달 후 취소하면 남은 부분만 중단한다. 긴 paste 중 긴급 Ctrl+C는 남은 paste를 취소한 뒤 우선 전달할 수 있어야 한다.

실제 구현은 JSON Schema에 enum/최대 길이/추가 필드 허용 여부를 명시한다. 큰 붙여넣기는 begin/chunk/commit 방식으로 원자적 순서를 보장하고, 중간에 lease가 바뀌면 남은 전송을 취소한다. cancel되어도 이미 PTY에 전달된 부분을 되돌릴 수 있다고 표시하지 않는다.

### 18.4 오류 모델

`UNAUTHENTICATED`, `FORBIDDEN`, `DEVICE_REVOKED`, `LEASE_REQUIRED`, `LEASE_STALE`, `SESSION_GENERATION_MISMATCH`, `HOST_RESTARTED`, `SESSION_LOST`, `INPUT_DELIVERY_UNKNOWN`, `FRAME_TOO_LARGE`, `RATE_LIMITED`, `RESYNC_REQUIRED`, `PREVIEW_NOT_APPROVED`, `UPSTREAM_CHANGED`, `CAPTURE_UNSUPPORTED`, `SOURCE_STALE`, `DESKTOP_LOCKED`, `ELEVATION_REQUIRED`, `MEDIA_UNAVAILABLE`를 최소 정의한다.

오류는 code, 사용자가 이해할 설명, retry 가능 여부, request_id를 갖는다. 인증 실패에 프로젝트 이름·경로·현재 터미널 내용을 포함하지 않는다.

---

## 19. 저장 구조·로그·복구

### 19.1 파일 배치

```text
%LOCALAPPDATA%/RemoteCodex/
  config.toml
  state.db
  logs/                 # 크기 제한 진단/감사 이벤트
  recordings/           # 사용자가 켠 세션만, 선택 사항
  updates/              # 검증된 업데이트 임시 파일
  runtime/              # 민감한 bootstrap 파일은 권한 제한·짧은 수명
```

프로젝트 소스는 원래 디렉터리에 둔다. RemoteCodex 내부로 복사·이동하지 않는다. 호스트 개인키/민감 credential은 Windows 사용자 보호 저장소를 사용하고 평문 config나 git에 넣지 않는다.

### 19.2 SQLite 최소 테이블

| 테이블 | 주요 필드 |
|---|---|
| `schema_migrations` | version, applied_at |
| `devices` | client_id, public_key, label, scopes_json, approved_at, revoked_at |
| `projects` | project_id, label, root_path, created_at |
| `sessions` | session_id, project_id, label, launch_profile_json, initial_cwd, generation, lifecycle_state |
| `session_runs` | run_id, session_id, agent_epoch, pid, process_started_at, started_at, ended_at, exit_code, end_reason |
| `previews` | preview_id, project_id, upstream_loopback, port, process_identity, exposed_origin, policy_json, revoked_at |
| `capture_grants` | grant_id, project_id, source_policy_json, view_allowed, control_allowed, expires_at |
| `audit_events` | event_id, at, client_id, action, resource_type, resource_id, result |
| `settings` | key, value_json, revision |

live PTY handle, WebSocket, lease timer, 모든 터미널 bytes는 DB에 저장하지 않는다. 관리 상태는 transaction으로 저장하고, 지연 가능한 변경은 묶어서 저장한다. DB access는 PTY hot path 밖의 전용 작업 큐로 보낸다.

### 19.3 기본 로그 정책

진단 로그는 크기/기간 제한을 둔다. 초기 기본값 총 50MiB 또는 7일 중 먼저 도달한 제한으로 회전한다. audit에는 연결·승인·제어권·종료 같은 메타데이터만 기본 저장한다.

원문 터미널 기록은 기본 OFF, 세션별 opt-in이다. 켜면 키·경로·코드·개인정보가 출력에 포함될 수 있음을 알린다. 입력 raw keystroke 저장은 별도 기능으로 추가하지 않는다. 토큰/비밀이 출력되는 모든 형태를 완벽히 마스킹한다고 약속하지 않는다.

기록 OFF에서도 살아 있는 Agent의 bounded ring과 화면 snapshot으로 재접속은 가능하다. 다만 잘린 오래된 출력과 Agent 재시작 이전 화면을 전부 복원할 수 있다고 안내하지 않는다.

### 19.4 재시작 복구

시작 시 이전 `running` 세션이 현재 Agent epoch 소유가 아니면 `lost` 처리한다. PID가 같다는 이유로 다른 프로세스에 붙지 않는다. 세션 항목·초기 cwd·프로필은 복원하고, 실제 프로세스 재실행은 명시적으로 수행한다.

Codex 재개 옵션은 설치된 버전의 공식 지원을 확인해 제공한다. 이미 살아 있는 동일 thread에 두 writer를 붙이지 않는다. 오래된 로그를 근거로 새 Codex를 자동 실행하지 않는다. [S21][S22]

---

## 20. 선택 확장: Codex Supervisor

이번 6단계의 완료 조건에 Supervisor를 끼워 넣지 않는다. 먼저 정확하고 빠른 원격 입출력을 완성한다.

후속 기능은 feature flag 기본 OFF로 한다. `final 문자열 감지 = 프로세스 종료 = 미완료 = 자동 재시작`이라는 로직은 금지한다.

정확히 구분할 것: 응답 턴 종료, 입력 대기, 승인 대기, 네트워크 장애, 프로세스 종료, 사용자가 요청한 정지, 실제 작업 완료. 자동 재개는 검증 가능한 상태·사용자의 사전 허가·횟수/비용 상한·중단 버튼·circuit breaker를 갖춘 경우에만 한다.

미확인 테스트 결과를 `126/131` 같은 그럴듯한 수치로 생성하지 않는다. 구조화된 결과나 실제 확인한 프로세스 종료 정보가 없으면 `확인 안 됨`이다.

---

## 21. 저장소 구조와 개발 산출물

```text
remotecodex/
  SPEC.md
  CODEX_START.md
  ACCEPTANCE_MATRIX.md
  IMPLEMENTATION_STATUS.md
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  package.json
  pnpm-lock.yaml
  apps/
    desktop/                 # Tauri UI shell, local IPC bootstrap
    web/                     # 정적 웹 진입점
  packages/
    ui/                      # 공유 Svelte UI
    protocol/                # 생성 TypeScript/JSON Schema
    terminal-client/          # xterm adapter, flow control, snapshot
  crates/
    rc-agent/
    rc-protocol/
    rc-session/
    rc-terminal-model/
    rc-pty-windows/
    rc-auth/
    rc-preview/
    rc-platform-windows/
    rc-media/                # P4부터 실행 구현
    rc-store/
    rc-test-fixtures/
  tests/
    protocol/
    windows-integration/
    terminal-golden/
    browser-e2e/
    security/
    performance/
  scripts/
    check.ps1
    test-windows.ps1
    test-e2e.ps1
    benchmark.ps1
    package.ps1
  docs/
    adr/
    compatibility/
    test-results/
    threat-model.md
  licenses/
```

처음부터 빈 crate 수십 개를 만드는 것이 목표는 아니다. 분리 경계는 지키되 P1에 필요한 실행 단위를 먼저 만들고, 미래 기능은 interface·capability·문서로 예약한다. 계약 변경은 schema compatibility test로 확인한다.

개발 도구 버전은 실행 시점에 공식 릴리스/문서를 확인해 lock한다. 문서에 적혀 있지 않은 버전·명령 옵션을 추측해서 사용하지 않는다.

---

## 22. 테스트 및 완료 정의

### 22.1 검사 계층

Unit → protocol/property → Windows integration → 브라우저 E2E → 실제 두 PC/Tailscale → 모바일 실기기 → 성능/soak → 설치/제거 순서로 증거를 쌓는다. 자세한 테스트 ID는 `ACCEPTANCE_MATRIX.md`에 있다.

Mock PTY 테스트만으로 ConPTY 호환을 통과 처리하지 않는다. Linux에서 컴파일했다고 Windows 캡처를 검증했다고 보고하지 않는다. 자동화하지 못한 실기기 항목은 `NOT_RUN`으로 남긴다.

### 22.2 단계별 필수 실증

- P1: 회사/집에서 같은 PTY의 echo·한글·제어키 확인, 무인증 입력 차단, UI 종료 후 생존.
- P2: 8세션 중복/교차 입력 없음, 두 작성자 경쟁, snapshot/TUI 복구, 모바일 열람 시 resize 없음, 24시간 유지.
- P3: Vite/Next.js HMR, root asset/refresh, HTTP/WS/SSE, 인증 철회, 관리 origin 공격 차단, 지원 불가 로그인 모드 표시.
- P4: 일반 창·크기 변경·최소화·닫힘·GPU 유실, 시청 종료 시 자원 해제, 터미널 성능 회귀 없음.
- P5: 실제 마우스/키보드, DPI·창 이동·구 geometry 거절, lease 회수, modifier 해제, UAC/잠금 처리.
- P6: 모니터 전환·좌표·표시·회수·다른 업무 정보 노출 안내, GUI 실패에도 터미널 유지.
- 모바일: Android/iOS IME·회전·전환·재접속·초안·가로 스크롤·세션 보존.

### 22.3 성능 결과 형식

`benchmark-results.json`에는 commit, build mode, 환경, 실행 시간, sample 수, scenario, p50/p95/p99/max, CPU 정의, 메모리 정의, 제외 프로세스와 그 부하, 실패 항목을 기록한다. 분포를 숨긴 평균 하나만 보고하지 않는다.

`IMPLEMENTATION_STATUS.md`에는 각 요구 ID의 `IMPLEMENTED / VERIFIED / FAILED / NOT_RUN / DEFERRED`를 구분한다. 목업·가정·수동 확인 요청을 완료로 표시하지 않는다.

### 22.4 출시 차단 결함

다른 세션으로 입력 전달, 중복 Enter 실행, 불명확한 재전송, 무인증 터미널 조작, preview→관리 권한 상승, UI 종료로 PTY 종료, 무한 메모리/버퍼 증가, GUI modifier 고착, 다른 창에 잘못된 입력이 발생하는 재현 결함, 백신/정책 우회, 미검증 성능을 달성했다고 주장하는 보고.

---

## 23. Codex 구현 순서

### P0: 구조적 위험 확인

실제 Windows에서 portable-pty/ConPTY로 한글·Codex TUI·resize·Ctrl+C를 확인한다. Tauri UI를 종료해도 독립 Agent와 PTY가 남는지 확인한다. terminal engine 두 후보에서 snapshot 복구와 메모리를 비교한다. 인증 bootstrap과 프리뷰 origin 설계를 ADR로 고정한다.

P0에서 불확실한 캡처/인코더 코드를 대량 생성하지 않는다. 다만 나중에 영상 프로세스를 분리할 interface 경계는 확정한다.

### P1: 실제 세로 절단 구현

설치/실행 → 장치 승인 → PTY 하나 생성 → 집 웹에서 입력 → 회사 출력 표시 → UI 종료 후 유지까지 이어지는 실제 경로를 완성한다. 입력/출력 trace fixture로 성능을 측정한다. 이 단계에서 운영 인증을 “나중에”로 미루지 않는다.

### P2: 다중화와 지속성

세션 생성/종료, 단일 작성자, 독립 탭, snapshot, bounded ring, 느린 클라이언트, 모바일 read-only 크기 보존, 24시간 지속성을 구현한다.

### P3: 웹 프리뷰

허가된 포트 등록, 별도 origin Gateway, scoped launch, cookie 제한, HTTP/WS/SSE, 프레임워크 adapter, 새 탭 열기를 완성한다.

### P4: 창 영상

MediaHelper를 on-demand로 붙인다. WGC/코덱/WebRTC를 실제 하드웨어에서 각각 확인하고 종단간 통합한다. GPU 없는 테스트 환경에서 화면이 나온다고 거짓 보고하지 않는다.

### P5: 창 입력

GUI lease, geometry, stale 방지, foreground 확인, modifier cleanup, 로컬 회수, 권한 제한을 구현한다. 영상보다 입력의 안전성을 우선한다.

### P6: 모니터 확장

같은 media/input 구조를 모니터 source로 확장하고, 데스크톱 권한 및 local-user 충돌 처리를 완성한다. 기능 OFF 시 부하가 남지 않게 한다.

각 단계 후 lint/typecheck/test/build/실기기 검사/성능 결과를 남긴다. 현재 환경에서 못 한 검사는 `NOT_RUN`으로 기록하고 가능한 구현·검증은 계속 수행한다. 테스트를 삭제하거나 기준을 낮춰서 다음 단계로 넘어가지 않는다.

---

## 24. 설계 검토에서 해결해야 하는 두 가지 선택

### ADR-001: Rust terminal state engine

P0에서 `wezterm-term`과 `alacritty_terminal`의 실제 현재 버전을 확인해 비교한다. 선택 결과에는 메모리, TUI fixture, 상태 직렬화/복원 방식, xterm 차이, 의존성 크기, 라이선스를 기록한다. 어떤 후보도 목표를 만족하지 못하면 작은 호환 adapter 개선안을 작성한다. 자체 VT emulator를 처음부터 만들기 시작하지 않는다.

### ADR-002: WebRTC/하드웨어 인코딩 통합

P4 시작 시 `str0m + Windows Media Foundation`을 우선 검증한다. 하드웨어 surface 전달, H.264 packetization/profile, 브라우저 decode, tailnet ICE, reconnect를 확인한다. 목표를 못 맞추면 검증된 다른 미디어 라이브러리의 크기·라이선스·지연을 비교해 변경한다. 미디어 선택 때문에 터미널 프로토콜이나 Agent의 PTY 수명을 다시 설계하지 않는다.

이 두 항목은 확인되지 않은 라이브러리 기능을 확정 사실로 쓰지 않기 위한 **검증 게이트**다. 제품 목표나 단계 순서는 미정이 아니다.

---

## 25. 금지하는 구현 지름길

1. 이미 열린 외부 터미널을 키보드 훅/화면 긁기로 억지 제어하기.
2. 실제 PTY 대신 `exec`를 매 메시지마다 새로 실행하기.
3. UI/WebSocket 종료 시 PTY를 자동 종료하기.
4. 모든 키 입력을 50~200ms debounce하거나 로그/영상 큐 뒤에 세우기.
5. xterm에 무제한 write하고 ACK/버퍼/파싱 상태를 무시하기.
6. 끊긴 뒤 마지막 명령을 무조건 다시 보내기.
7. 모바일 접속만으로 세션을 새로 만들거나 회사 화면 크기를 변경하기.
8. 프리뷰를 관리 UI와 같은 privileged origin으로 제공하기.
9. localhost/같은 tailnet이면 무조건 신뢰하기.
10. 모든 창·모든 모니터를 상시 캡처하고 안 쓰는 영상 encoder를 유지하기.
11. GUI 입력을 완전히 창별 격리했다고 보장하기.
12. UAC·잠금·회사 보안 정책·백신을 우회하기.
13. “Tauri라서 가벼움”, “Rust라서 빠름”만으로 성능 검사 생략하기.
14. Codex final 텍스트나 출력 중단만 보고 무한 자동 재실행하기.
15. 테스트 미실행을 통과로 적거나 목업을 실제 구현이라고 보고하기.

---

## 26. 참고 자료와 적용 범위

아래는 2026-09-11 확인한 1차 자료다. 이 문서의 아키텍처·성능 예산·권한 기본값·단계 구성은 RemoteCodex를 위한 설계 제안이며, 해당 프로젝트들이 그대로 보장하는 수치/기능이라는 뜻이 아니다. 실제 의존성 선택 시 현재 버전과 라이선스를 다시 확인하고 commit/version을 기록한다.

- **[S01] Tauri Architecture** — OS WebView/Rust 구조. `https://v2.tauri.app/concept/architecture/`
- **[S02] Tauri Windows Installer** — NSIS/MSI·WebView2 설치 모드. `https://v2.tauri.app/distribute/windows-installer/`
- **[S03] Microsoft: Creating a Pseudoconsole session** — ConPTY 생성·I/O·resize·종료. `https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session`
- **[S04] Tailscale Serve** — tailnet 내부 서비스 공개. `https://tailscale.com/docs/features/tailscale-serve`
- **[S05] tailscale serve command** — HTTPS/프록시/포트/설정 수명. `https://tailscale.com/docs/reference/tailscale-cli/serve`
- **[S06] Tauri Capabilities** — webview별 권한 경계. `https://v2.tauri.app/security/capabilities/`
- **[S07] xterm.js Flow Control** — 비동기 write와 흐름 제어. `https://xtermjs.org/docs/guides/flowcontrol/`
- **[S08] xterm.js Security** — 터미널 출력과 브라우저 보안. `https://xtermjs.org/docs/guides/security/`
- **[S09] Microsoft Screen capture** — WGC·frame·지원/장치 변경. `https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture`
- **[S10] WebRTC Peer connections** — signaling/ICE·연결 구조. `https://webrtc.org/getting-started/peer-connections`
- **[S11] Microsoft SendInput** — 입력 스트림·UIPI 제한. `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput`
- **[S12] Microsoft Desktop Duplication API** — 데스크톱 프레임 캡처. `https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api`
- **[S13] portable-pty API** — Rust PTY abstraction. `https://docs.rs/portable-pty/latest/portable_pty/`
- **[S14] WezTerm 공식 저장소** — Rust terminal/multiplexer 구현 참고. `https://github.com/wezterm/wezterm`
- **[S15] Microsoft H.264 Video Encoder** — Media Foundation encoder 속성. `https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder`
- **[S16] str0m 공식 저장소** — Rust WebRTC transport 후보. `https://github.com/algesten/str0m`
- **[S17] Vite Server Options** — host/origin·WS/HMR 프록시 제약. `https://vite.dev/config/server-options`
- **[S18] MDN Same-origin policy** — scheme/host/port 경계. `https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Same-origin_policy`
- **[S19] MDN Using HTTP cookies** — cookie scope·보안 속성. `https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Cookies`
- **[S20] MDN CryptoKey.extractable** — 키 export 제한의 의미. `https://developer.mozilla.org/en-US/docs/Web/API/CryptoKey/extractable`
- **[S21] OpenAI Codex CLI reference** — 설치 버전의 명령 지원 확인. `https://developers.openai.com/codex/cli/reference/` (확인 시 공식 `learn.chatgpt.com/docs/developer-commands?surface=cli`로 연결)
- **[S22] OpenAI Codex App Server** — 구조화 integration 후보·프로토콜 확인. `https://developers.openai.com/codex/app-server/` (확인 시 공식 `learn.chatgpt.com/docs/app-server`로 연결)
- **[S23] Microsoft Interactive Services** — Windows 서비스와 사용자 GUI 세션 제약. `https://learn.microsoft.com/en-us/windows/win32/services/interactive-services`
- **[S24] Happy 공식 저장소** — 모바일/웹 UX 참고, 재사용 시 파일별 라이선스 확인. `https://github.com/slopus/happy`

---

## 27. 최종 인수 문장

**회사에서 RemoteCodex 안에 두 개 이상의 실제 터미널을 띄워 Codex를 실행한 뒤 UI를 닫고, 집에서 같은 세션에 들어가 자연스럽게 입력·출력을 확인할 수 있어야 한다. 웹 프리뷰와 이후 GUI/데스크톱 확인을 켜도 터미널 입력이 제품 내부 병목 때문에 눈에 띄게 지연되지 않아야 한다. 휴대폰은 같은 서버·같은 세션을 CLI 중심 화면으로 사용해야 한다.**

이 문장이 실기기에서 검증되지 않으면 제품의 핵심은 완료되지 않은 것이다.


## 28. v1.1 확정 추가 — 마우스, 다중 CMD/PowerShell, 분할 화면

사용자의 추가 확인에 따라 아래 규칙을 확정한다. §9·§13·§16·§18을 보완하며, 충돌하면 이 절의 보다 구체적인 입력 구분/연결 식별 규칙을 적용한다. 설계 범위를 P1~P6보다 축소하지 않는다. 현재 소스에 없는 기능은 IMPLEMENTATION_STATUS.md에서 별도로 표시한다.

### 28.1 마우스 입력의 세 영역

| 사용자가 하는 동작 | 실제 대상 | 반드시 지킬 규칙 |
|---|---|---|
| 프로젝트/탭/분할 패널 선택 | 집/회사/모바일의 로컬 UI | 회사의 Windows 포인터에 주입하지 않음 |
| 일반 터미널 글자 선택·복사·스크롤 | 해당 클라이언트의 터미널 뷰 | 회사 OS 마우스를 움직이지 않음 |
| 마우스 지원 TUI 내부 클릭·휠 | 해당 PTY에 터미널 mouse protocol 전송 | 실제 프로그램이 reporting을 켠 경우에만 전달, writer lease 필요 |
| Windows App Preview 버튼 클릭 | 승인한 회사 Windows GUI 대상 | P5의 별도 전역 GUI lease·frame/geometry/권한 검증 필요 |
| Desktop 화면 클릭·이동 | 승인한 회사 Windows interactive desktop | P6 승인과 P5 보호 장치 필요 |

TUI가 mouse reporting을 켜면 휠이 프로그램으로 전달될 수 있다. 따라서 “휠은 언제나 로컬 스크롤”로 고정하지 않는다. Shift 등 명시적인 로컬 선택 우회 동작을 제공한다. **터미널 mouse input을 SendInput 좌표 클릭으로 바꾸지 않는다.** read-only 뷰는 로컬 선택은 가능하지만 TUI 입력은 차단한다.

### 28.2 여러 CMD/PowerShell

- 회사 PC의 CMD/PowerShell마다 서로 다른 `session_id`, PTY handle, shell PID, generation, 입력 큐, output sequence, lease를 가진다.
- Sales에서 Ctrl+C를 눌렀다고 BeforeTalk이나 Dev Server로 보내면 안 된다. 브로드캐스트 입력은 기본 기능이 아니다.
- “화면에서 탭 닫기”와 “회사 프로세스 종료”를 별개의 버튼/API로 구분한다. 후자만 명시적 확인 뒤 `terminal.close`를 사용한다.
- 프로젝트는 터미널과 1:1이 아니다. 같은 회사 프로젝트 폴더에서 Codex/Dev Server/Test 터미널을 함께 만들 수 있으며 서버가 안정적인 project_id를 부여한다.
- 클라이언트가 접속하지 않아도 활성 PTY를 유지한다. 일반 Windows Terminal에서 이미 실행한 임의 탭의 자동 attach/탈취는 추가하지 않는다.

### 28.3 입력 제어권은 기기 ID만으로 판정하지 않음

`(agent_epoch, session_id, generation, client_id, connection_id, lease_epoch)`를 검증한다. 같은 등록 기기에서 브라우저 탭 두 개를 열어도 connection_id는 다르다. 한 탭이 다른 탭의 writer 권한을 자동 공유하지 않는다. 강제 인계는 명시적인 takeover 요청이어야 한다.

writer lease는 15초, heartbeat는 5초를 기본값으로 한다. 인증 갱신은 일회용 scoped WS ticket으로 **같은 연결**에서 수행할 수 있다. 인증 갱신 자체 때문에 세션을 재생성하거나 writer 연결 ID를 바꾸지 않는다. 권한 철회/연결 종료/lease 만료 후 큐에 남은 이전 입력은 쓰기 직전에 다시 검증한다. 이미 OS 파이프에 전달된 입력을 취소했다고 거짓말하지 않는다.

### 28.4 데스크톱 분할 화면

초기 구현은 동시에 보이는 최대 두 PTY 패널을 지원한다. 숨은 탭을 포함한 회사 PTY 실행 수와 렌더링 패널 수는 다르다. 사용자가 세 번째 세션을 분할로 열면 실행을 종료하지 않고 보조 패널의 표시 대상을 교체한다.

- 화면의 강조 표시와 실제 입력 라우팅이 일치해야 한다.
- 포인터/키보드 focus 변경은 입력 전에 동기적으로 확정한다. UI 프레임 렌더 완료를 기다려 첫 클릭을 버리거나 이전 PTY로 보내지 않는다.
- 실제 키 입력/클릭 전송은 한 활성 패널에만 보낸다. 분할의 다른 패널은 자기 세션의 출력만 갱신한다.
- UI의 클릭은 로컬 동작이다. TUI focus-reporting이 켜진 경우 선택된 PTY에 정상적인 focus protocol을 전달할 수 있으나 Windows GUI 클릭과 혼동하지 않는다.
- 숨긴/백그라운드 패널은 렌더링을 중지하거나 구독을 해제한다. 회사 PTY의 실행·로그/모델 처리와 뷰 수명을 묶지 않는다.

### 28.5 모바일

모바일은 한 활성 CLI 뷰, 세션 전환, 입력 초안, Ctrl+C/Esc/Tab/방향키 도구가 우선이다. 데스크톱 분할 레이아웃을 축소해서 두 개 동시에 보여주지 않는다. 같은 회사 세션을 선택한다는 의미는 유지한다.

모바일로 읽기만 할 때 `resize`를 보내면 안 된다. writer라도 모바일 열 수 변경은 명시적 동의가 필요하다. 기본은 기존 PTY 폭을 유지하고 클라이언트 가로 스크롤/확대 등으로 표시한다. 초안은 세션별로 분리하고, 한글 composition 중 Enter를 전송 버튼으로 해석하지 않는다. 백그라운드에서 연결이 끊기면 복귀 후 상태를 새로 동기화하고 입력을 자동 재전송하지 않는다.

### 28.6 GUI 입력은 터미널의 확장이지만 독립된 보안 영역

Windows 창 두 개가 떠 있다고 실제 Windows 마우스/foreground도 둘로 분리되는 것이 아니다. GUI 제어권은 같은 interactive desktop 전체에서 하나다. PTY별 독립 writer와 혼동하지 않는다. 창만 캡처한다고 window-only 입력 sandbox가 보장되지 않는다.

회사 사람이 물리 키보드/마우스를 쓰면 원격 GUI 입력을 중단하거나 회수하는 로컬 우선 정책이 필요하다. UI에 “보기 전용”이라고 표시하는 것만으로 회사의 실제 물리 입력이 막히는 것은 아니다. BlockInput, Secure Desktop/UAC 우회, 사용자가 모르게 입력하는 기능을 구현하지 않는다.

좌표는 letterbox를 제거한 normalized source 좌표로 전달하고, 서버가 현재 캡처 source의 물리 좌표·DPI·모니터 원점으로 변환한다. 다른 source/generation/geometry, 너무 오래된 frame, 잘못된 HWND/PID/creation-time, foreground 변경은 거절한다. disconnect·blur·인계·잠금 때는 원격에서 주입한 눌린 키/버튼만 해제한다.

### 28.7 코드 작성에 적용한 구조

```text
apps/desktop/src-tauri      회사 로컬 Tauri UI 껍데기 / 제한된 named-pipe 관리
apps/web                   Svelte 반응형 UI / 탭·분할·모바일 CLI
packages/core              wire frame·복원 순서·입력 정책·좌표·IME 순수 로직
packages/terminal-client   장치 인증·control WS·xterm 스트림 클라이언트
crates/rc-agent             실제 PTY/인증/HTTP/WS/프리뷰/SQLite
crates/rc-core              Rust wire·lease·input ledger·ring·VT fence·media 정책
crates/rc-platform-windows  SID·IPC·process identity·Win32 GUI 입력 보호 소스
crates/rc-media             P4~P6 native-media 어댑터; 기능/설정/SDK 검사 실패 시 false
```

소스가 존재함, 해당 플랫폼에서 컴파일됨, 실제 기능이 동작함, 성능 목표를 통과함은 네 가지 다른 상태다. `REFERENCE_PROJECTS.md`, `IMPLEMENTATION_STATUS.md`, `CODEX_CONTINUE.md`를 함께 읽는다. **모든 필수 gate를 통과하기 전 제품 전체를 완료라고 하지 않는다.**


## 29. v1.2 구현 계약 — 실제 미디어 경로·재접속·큰 입력

2026-09-11, 소스 0.2.0. 이 절은 §4·§8·§10·§12~§14·§28의 구체화다. 선택 기술 변경은 ADR 003에 기록했고 기존 보안·성능·모바일 요구를 완화하지 않는다. 코드 존재와 제품 인수 통과는 분리한다.

### 29.1 영상 구현 선택

`rc-media`의 Windows-only `native-media` feature에서 GStreamer Rust 0.24 계열을 사용한다. WGC/D3D11 surface → NV12 → Media Foundation H.264 → RTP → webrtcbin 경로를 실제 소스로 연결한다. SDK API 최소 목표는 1.24이며 사용자 PC의 실제 버전/플러그인을 고정·검증한다. 원래 str0m 후보는 대안이며 현재 연결 어댑터는 아니다.

Agent/PTY/모바일 CLI는 GStreamer를 링크하지 않는다. 명시적인 media probe/view만 별도 helper를 실행한다. CLI-only 상태에서는 캡처·인코더·미디어 SDK 상주를 만들지 않는다. 공유 중 visible indicator/긴급 중단이 동작하기 전 입력을 허용하지 않는다. DLL 번들 용량·활성 메모리·CPU·정지 화면 부하는 실측하며 SDK를 사용했다고 최적화 완료로 판단하지 않는다.

### 29.2 승인과 source identity

창/모니터 열거는 회사 로컬 SID 승인 경로에만 있다. 원격 목록에는 로컬에서 승인한 source UUID만 노출한다. Window는 HWND/PID/creation-time/client-area rect, Monitor는 HMONITOR/device-name/rect를 묶는다. 네이티브 handle/64-bit sequence는 JSON decimal string, 정수 정밀도 손실 금지다.

GUI scope는 terminal scope와 별도 승인한다. 하나의 호스트에서 한 source/한 video viewer를 우선 지원한다. 여러 터미널의 병렬 실행/열람을 제한하는 변경은 아니다. source 교체 시 이전 미디어 세션을 종료하고 새 ticket/approval 경로로 연결한다. 창 이동/resize/identity 변경은 입력 중단 및 재승인 방식이며 seamless renegotiation은 후속 게이트다.

### 29.3 분리된 미디어 API와 wire

- `GET /api/v1/media/capabilities`: 설정/feature/SDK probe 가능 여부. factory 성공은 Windows runtime verified와 별개다.
- `GET /api/v1/media/sources`: principal의 window/desktop scope에 맞는 승인 목록만.
- 기존 `POST /api/v1/tickets/ws`의 media channel: source UUID에 제한한 30초 1회 ticket.
- `/api/v1/ws/media`: source/owner/auth/offer/answer/ICE/presented/global lease/input/stop. 영상 bytes는 여기에 실어 보내지 않고 WebRTC로 전송한다.
- Agent↔MediaHelper: 96KiB 상한 JSON lines, bounded commands/events, 최초 Init 1회, 종료/EOF 시 cleanup. 입력/출력 큐와 PTY 큐는 공유하지 않는다.

초기 peer 후보는 숫자형 승인 Tailscale IPv4 UDP만. company interface/UDP 범위는 명시 설정하며 외부 STUN/TURN/public/mDNS 후보를 조용히 허용하지 않는다. 이 정책 때문에 브라우저/회사망에 따라 GUI가 안 될 수 있다. 오류를 표시하고 터미널은 유지한다.

### 29.4 GUI 입력 안전 상태

helper encoded-frame 갱신과 browser 실제 presented-frame 모두 최신이어야 제어를 획득한다. 기본 허용 frame age는 500ms, lease TTL 15초/renew 5초다. generation/geometry/monotonic input seq/허가된 action을 검사한다. 소스 최소화/잠금/가림/foreground/권한 오류면 원격 입력을 중단한다.

회사 물리 입력은 local low-level hook에서 **내용을 기록하지 않고 interruption만** 처리한다. native revision을 즉시 바꾸어 큐에 남은 옛 입력을 거절한다. 새 Arm가 큐에 있는 동안 local input이 발생해도 stale Arm로 재활성화하지 않는다. 눈에 보이는 공유 표시창 닫기/Ctrl+Alt+F12를 긴급 종료로 제공한다. local input을 BlockInput으로 막지 않는다.

DPI는 Win32 thread-local per-monitor-aware scope로 처리하고 이전 context를 복원한다. RAII guard를 async task 이동으로 다른 thread에 넘기지 않는다. mouse move만 병합하고 discrete down/up/키는 순서를 보존한다. lease/revoke/disconnect/blur/timeout 때 자신이 주입한 pressed state만 해제한다. SendInput/UIPI/secure-desktop 제약을 우회하지 않는다.

### 29.5 붙여넣기와 입력 경합

일반 control WS 입력은 8KiB 상한을 유지한다. 사용자가 명시한 큰 paste는 source에 바인딩된 인증 HTTP route `/api/v1/sessions/{id}/paste`로 보낸다. text 전체의 UTF-8 길이/허용 제어문자를 먼저 검사하고 최대 256KiB를 수락한다. JSON escaping을 고려한 body 상한은 별개다.

PTY writer는 최대 8KiB씩 쓰면서 lease/connection/generation을 다시 검사한다. 해당 PTY의 다른 일반 입력이 paste 중 섞이지 않게 하되 Ctrl+C는 우선 처리한다. 다른 PTY의 입력은 영향을 받지 않는다. writer가 시작한 bracketed-paste 구간은 cancellation 시 가능한 범위에서 닫는다. 이미 OS에 쓴 입력을 rollback했다고 하지 않는다. 모호한 timeout/partial delivery는 자동 재전송하지 않는다.

### 29.6 재접속과 복원 검증

client connect attempt/reload epoch을 각각 증가시켜 오래된 인증/host/session-list/WS 응답이 현재 연결과 lease를 덮어쓰지 못하게 한다. stop은 observer 수명만 정리하고 회사 PTY를 종료하지 않는다.

headless server TerminalModel의 synchronized-update를 명시적으로 flush하는 코드, pending-wrap·dynamic palette snapshot 코드를 추가했다. 해당 Rust 테스트는 실기기에서 실행해야 한다. saved charset/cursor shape/모든 reset/alt/Unicode/resize를 검증하지 않은 상태에서 full-fidelity로 표기하지 않는다.

### 29.7 출시 판정

125개 portable/client 테스트의 PASS는 core/client 로직 증거다. Windows/Rust/전체 Svelte/Tauri/SDK/two-PC/모바일/성능/soak는 별도 게이트다. EXE 설치 패키징은 현재 사용자 요청 범위 밖이지만 Agent tray/자동 시작·영상 적응형 fps 등 원래 요구를 완성했다고 간주하지 않는다. 모든 미구현·미검증 항목은 IMPLEMENTATION_STATUS.md와 인수표에 남긴다.
