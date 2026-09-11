# RemoteCodex — Source Alpha 0.2.0

회사 Windows PC를 실제 실행 본체로 유지하면서 CMD·PowerShell·Codex를 회사/집/모바일에서 공유하는 원격 개발 콘솔이다. 회사 UI나 집 브라우저를 닫아도 Agent가 살아 있는 동안 PTY를 유지한다.

> **소스 알파이며 설치형 완성품이 아니다.** Windows에서 TypeScript **125개**, Rust **35개**, 실제 ConPTY **1개** 테스트와 Rust workspace/Tauri 타입 검사, Svelte 검사·웹 빌드를 통과했다. [Windows 검증 기록](docs/test-results/windows-baseline-2026-09-11/README.md)을 참고한다. Tauri Release 빌드·실제 GUI·영상·두 PC·모바일·성능은 미검증이다. 먼저 [구현 상태](IMPLEMENTATION_STATUS.md)를 읽는다.

## 구조

```text
회사: rc-agent  ─ PTY 1 ─ CMD / Codex
                 PTY 2 ─ PowerShell / Codex
                 PTY 3 ─ 개발 서버
        ├ control WS / terminal output WS / paste HTTP
        ├ 승인한 localhost 프리뷰
        └ 선택적 rc-media ─ 승인 창/모니터 ─ H.264/WebRTC
             │
       Tailscale / Serve HTTPS          ← 공개 인터넷 서버를 만들지 않음
             │
      집 브라우저 / 회사 Tauri UI / 모바일 CLI
```

터미널 UI 클릭·TUI 마우스·Windows 실제 마우스는 서로 다른 경로다. 여러 CMD는 별개 PTY/입력 lease다. GUI 제어는 회사 desktop 전체에서 한 명만 사용한다. 모바일은 한 CLI와 명령 작성창을 기본으로 유지한다.

## Windows에서 소스 빌드

Windows 11 x64, Rust stable MSVC/Cargo, C++ build tools, Node.js 22+, Tauri용 WebView2가 필요하다. 공식 Tauri 선행 조건: https://v2.tauri.app/start/prerequisites/

저장소 루트에서 아래 스크립트를 **먼저 검토**한다. 실패가 나오면 다음 단계로 넘어가지 말고 `CODEX_CONTINUE.md`에 따라 실제 API/type 오류를 고친다. Windows에서 생성한 npm/Cargo lockfile이 포함되어 있다. 재현 설치에는 `npm ci --ignore-scripts`를 사용하며, `bootstrap.ps1`은 의존성을 다시 해결하므로 lockfile 변경을 검토한다.

```powershell
.\scripts\bootstrap.ps1
.\scripts\build-source.ps1           # Rust + 웹, 설치 프로그램 아님
# Tauri 실행 파일도 검사/빌드하려면:
.\scripts\build-source.ps1 -Desktop
```

`build-source.ps1`는 설치/서명/자동 시작/방화벽/VPN 설정을 하지 않는다. Release 실행 바이너리는 사용자의 PC에서 만들어지는 개발 산출물이다. 전달 ZIP에는 EXE/MSI나 SDK DLL이 없다.

## 먼저 터미널로 실행

```powershell
Copy-Item .\config\agent.example.toml .\config\agent.local.toml
# agent.local.toml에서 실제 public_origin, 경로 등을 검토/수정
.\target\release\rc-agent.exe --config .\config\agent.local.toml run
```

Agent를 켠 상태에서 회사의 다른 PowerShell:

```powershell
.\target\release\rc-agent.exe pair
.\target\release\rc-agent.exe status
```

회사 브라우저의 `http://127.0.0.1:3847`에서 1회용 티켓으로 기기를 등록하고 실제 프로젝트 폴더와 CMD/PowerShell을 선택한다. 그 PTY에서 평소처럼 `codex`를 실행한다. 티켓/토큰을 Git이나 로그에 저장하지 않는다.

원격은 회사 정책상 허용된 Tailscale과 **별도로 검토한 Serve 규칙**으로 HTTPS를 loopback Agent에 연결한다. 실제 외부 origin은 config와 정확히 맞춰야 한다. 설치만으로 기존 Serve 규칙을 지우거나 Funnel/0.0.0.0을 켜지 않는다. 집과 모바일은 각각 기기 등록을 한다.

Agent 콘솔에서 Ctrl+C 또는 Agent 자체 종료는 실제 PTY 종료다. **회사 Tauri UI 종료와 다르다.** 트레이/로그인 자동 시작은 아직 구현 완료하지 않았다.

## 창·전체 화면 확장

영상 경로에는 **선택적 GStreamer Windows x64 MSVC 1.24+ runtime + development SDK**가 필요하다. Node 런타임 서버를 추가한 것이 아니고, `rc-agent`는 이 SDK를 로드하지 않는다. SDK는 ZIP에 포함하지 않으며 패키징/라이선스/활성 자원 비용은 별도 검증 대상이다. 자세한 설정은 [MEDIA.md](docs/compatibility/MEDIA.md).

```powershell
# 승인된 SDK를 설치·검토하고 해당 개발 환경/PATH를 구성한 후
.\scripts\build-source.ps1 -NativeMedia -Desktop
.\scripts\test-media.ps1
.\scripts\gui-fixture.ps1
```

회사 config의 `media.enabled`, `allow_control`, 실제 company `tailnet_ip`, 집 PC `allowed_peer_ips`를 명시적으로 설정한다. Agent 재시작은 PTY를 종료할 수 있으므로 기존 작업을 보존한 후 사용자가 결정한다. 첫 실험은 테스트 계정/임시 PTY와 fixture로 한다.

회사 로컬 Tauri의 승인 화면 또는 CLI에서 대상을 승인한다. `media-sources`가 반환한 **실제 handle**만 사용한다.

```powershell
.\target\release\rc-agent.exe media-sources
# 예시: 출력에서 고른 실제 숫자 handle로 교체할 것
# .\target\release\rc-agent.exe media-approve --kind window --handle <실제숫자> --control
# .\target\release\rc-agent.exe media-approve --kind monitor --handle <실제숫자> --control
.\target\release\rc-agent.exe pair --gui
```

집 기기는 GUI scope가 있어야 Apps/Desktop 목록과 제어 권한을 사용할 수 있다. 기존 terminal-only 기기에 권한을 몰래 추가하지 않는다. 현재는 호스트당 영상 source/viewer 하나이며, 명시적으로 보기/제어를 선택한다. 회사의 공유 표시창 닫기 또는 Ctrl+Alt+F12는 영상·제어를 중단한다. 창 이동/resize 뒤에는 안전 정지 후 새 승인/보기 시작으로 복구한다.

## 검사

```powershell
npm test                              # 실제 portable 96 + client 29
.\scripts\check.ps1 -WindowsPtyTests  # Rust/Svelte/Tauri + 실제 ConPTY
# 사용자가 위에서 Agent를 이미 켠 상태에서:
.\scripts\test-windows.ps1 -AgentE2E -AgentPath .\target\release\rc-agent.exe
```

Agent E2E는 임시 기기와 CMD 두 개를 생성해 실제 왕복/분리/중복 거부/재접속을 검사하고 자신의 리소스만 정리한다. 기존 회사 PTY는 건드리지 않는다. 정리 실패 시 ID를 보고하고 FAIL로 기록한다. E2E는 이 작성 환경에서 실행되지 않았다.

`docs/test-results/v0.2.0/`에는 실제 이 환경에서 실행한 로그가 있다. `docs/test-results/local-*`는 앞으로 Windows 스크립트를 실제 실행하면 생기는 결과 폴더다. **125개 단위 테스트 통과가 Windows/GUI/CPU·RAM/24시간 안정성 합격은 아니다.**

## 주요 문서

[설계명세서](SPEC.md) · [현재 상태](IMPLEMENTATION_STATUS.md) · [Codex 이어가기](CODEX_CONTINUE.md) · [제품 인수표](ACCEPTANCE_MATRIX.md) · [회귀 추적표](docs/TRACEABILITY.md) · [변경 요약](CHANGELOG.md)

[터미널 호환](docs/compatibility/TERMINAL_ENGINE.md) · [웹 프리뷰 호환](docs/compatibility/PREVIEW.md) · [영상/입력 호환](docs/compatibility/MEDIA.md) · [미디어 ADR](docs/adr/003-native-media-adapter.md)

소스의 MIT 라이선스는 외부 의존성 라이선스를 대신하지 않는다. 실제 lockfiles·SDK 플러그인·NOTICE·SBOM·보안 검토 후 배포한다. UAC/잠금 해제/보안제품 우회/숨겨진 접속/로컬 BlockInput은 구현하지 않는다.
