# Windows 영상·GUI 입력 — 0.2 소스 호환 계약

**실기기 검증 전.** 기본 media는 꺼져 있다. 이 문서는 작동 보장이 아니라 실제 소스를 검증하는 방법과 제한이다.

## 빌드/런타임

Windows 11 x64를 최초 기준으로 한다. Rust stable `x86_64-pc-windows-msvc`, C++ 도구, 검토된 GStreamer x64 MSVC runtime와 development SDK(1.24 이상)를 함께 준비한다. architecture/toolchain이 다른 SDK를 섞지 않는다. `gst-inspect-1.0.exe`와 런타임 DLL/플러그인 경로가 사용자 개발 환경에서 해석되어야 한다.

필수 factories: `d3d11screencapturesrc`, `d3d11convert`, `mfh264enc`, `h264parse`, `rtph264pay`, `webrtcbin`, `nicesrc`, `nicesink`. missing이 있으면 실패한다. `mfh264enc`가 `d3d11-aware`가 아니면 소프트웨어/스크린샷 경로로 조용히 전환하지 않는다.

```powershell
.\scripts\bootstrap.ps1
.\scripts\build-source.ps1 -NativeMedia -Desktop
.\scripts\test-media.ps1
```

스크립트는 SDK 설치·DLL 수집·방화벽 변경·관리자 승격·설치 패키지 제작을 하지 않는다. factory 존재만 확인한 `--probe`는 실제 capture/encoder/ICE가 성공했다는 검사가 아니다.

## 연결 준비

1. Agent config의 `[media]`에 enabled/control 정책, 실제 회사 Tailscale IPv4, 집 PC의 명시적 IPv4 allowlist, 좁은 UDP 범위를 등록한다. 예시 100.64.0.10/11은 실제 주소가 아니다.
2. 관리 HTTPS/WSS는 기존 Serve 경로다. 영상 UDP는 별도 경로이며 회사 보안 정책상 허용이 필요하다. 광범위 전체 포트 허용·Funnel·외부 STUN/TURN 설정을 추가하지 않는다.
3. 회사 로컬 Tauri 관리 또는 SID가 검증된 CLI에서 창/모니터를 골라 승인한다. 네이티브 handle은 decimal string으로 처리하고 JS Number로 변환하지 않는다.
4. 새 GUI 기기는 로컬 `pair --gui`로 명시 승인한다. terminal-only 기기에 자동 GUI 권한을 추가하지 않는다. 원격 목록은 승인된 대상만 공개한다.
5. 집에서 Apps/Desktop → source 선택 → 보기 시작 → 실제 새 프레임 수신 후 제어권 요청. 열람만으로는 SendInput을 하지 않는다.

회사 SDK/후보/캡처 실패는 GUI 세션만 종료한다. terminal input/output을 함께 끊지 않는다.

## 영상 모드

- 창: WGC client area. 타이틀바/테두리는 기본 캡처에 포함되지 않는다. Windows 창 전체라는 표현으로 client area 제한을 숨기지 않는다. 모달/파일 선택 창은 별도 승인 대상이 될 수 있다.
- 데스크톱: 승인한 모니터 하나. 모니터 전환은 이전 영상 종료 후 새 source로 연결한다.
- 한 호스트 한 source/한 video viewer. PTY viewer 제한과는 별개다.
- 기본 최대 1280×720, 15fps; 허용 상한 1920×1080, 30fps. 이는 요청 profile이지 실측 fps가 아니다.
- 비디오 payload는 WebRTC, stdin/stdout IPC는 제한된 JSON 시그널/메타데이터만 사용한다.
- 활성 중 queue는 오래된 frame을 버리지만 현재 정지 화면 자동 fps/bitrate 감소는 미구현이다.

## 입력 보호

한 회사 desktop의 GUI lease는 하나다. 별도 PTY lease와 구분한다. source identity/creation time/geometry/DPI/foreground/창 가림/무결성 수준을 검사하고, frame이 500ms보다 오래되면 입력을 거절한다. 브라우저는 `requestVideoFrameCallback` 실제 표시를 근거로 Presented를 보낸다. 미지원 브라우저는 제어를 활성화하지 않는다.

회사 표시창 닫기 또는 Ctrl+Alt+F12가 emergency stop이다. 회사의 물리 키보드/마우스를 감지하면 원격 입력을 회수한다. 해당 hook는 interruption만 보고 키 내용/마우스 경로를 기록하지 않는다. BlockInput을 사용하지 않는다. 자기 자신이 주입한 down 상태만 추적해 release한다. UAC/secure desktop/높은 권한 대상은 지원하지 않는다.

일반 SendInput은 OS 전체 입력 스트림에 영향을 준다. 선택 창 캡처는 샌드박스가 아니다. foreground 경쟁·권한 전환·잠금 시 항상 원하는 창에 입력/해제를 보장할 수 있다고 주장하지 않는다. 첫 시험은 실제 업무 프로그램이 아니라 제공 fixture로 수행한다.

## 네트워크 호환 제한

ICE는 숫자형 승인 tailnet IPv4/UDP host 후보만 허용한다. 브라우저가 mDNS 이름만 제공하거나 허용 tailnet 인터페이스 후보를 노출하지 않으면 연결되지 않을 수 있다. STUN/TURN을 몰래 추가하거나 브라우저 보안을 끄는 것으로 해결하지 않는다. ICE 진단과 명시적 확장 설계를 먼저 검토한다. 터미널의 Tailscale relay 연결 가능성과 이 WebRTC adapter의 호환성은 별개다.

## 실제 검사 절차 (전부 현재 NOT_RUN)

| 검사 | 절차/증거 |
|---|---|
| Native compile | feature on/off의 Cargo check/test, SDK/driver/build 번호 기록 |
| 보이는 fixture | `scripts/gui-fixture.ps1`를 회사에서 직접 실행, 그 창을 승인 |
| 영상 확인 | 테스트 문구·클릭 카운터 변화가 집 video에 보이는지 실제 녹화/타임스탬프 |
| 입력 왕복 | 버튼/텍스트/휠/scan-key가 fixture에서만 반영되는지; IME 입력은 composer 사용 |
| 좌표 | 100/125/150/200% DPI, letterbox, 음수 모니터 원점, 확대/회전 |
| 안전 중단 | 회사 물리 입력, Ctrl+Alt+F12, 공유창 닫기, 창 가림, 최소화, 잠금 |
| release | 눌린 modifier/버튼 중 disconnect/revoke/blur/stop: 고착 여부 실제 확인 |
| 무승인 차단 | 다른 source UUID/expired ticket/무GUI scope/재사용 핸들/오래된 geometry 거절 |
| 수명·경합 | 100회 시작/종료 후 orphan helper/hook/timer/D3D surface 없음 |
| 성능 | Release에서 터미널만/웹/영상 동시 입력 p95 비교, CPU/RAM/GPU·24h soak |
| mobile | CLI 지속성 검증; 모바일 GUI 요구를 억지로 추가하지 않음 |

창 이동/크기 변경은 현재 세션을 안전 정지한 후 재승인/다시 보기로 처리한다. 자동 재협상·모든 모달 추적·소프트웨어 인코더 fallback·전체 Windows 로그인 화면·오디오·클립보드 상시 동기화는 이 버전의 완료 기능이 아니다.

공식 문서/SDK 비용 판단: [ADR 003](../adr/003-native-media-adapter.md). 원본 제품 인수 조건은 [ACCEPTANCE_MATRIX.md](../../ACCEPTANCE_MATRIX.md)이며 이 호환표가 원본 게이트를 삭제하지 않는다.
