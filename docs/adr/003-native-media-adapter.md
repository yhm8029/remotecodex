# ADR 003 — on-demand GStreamer 네이티브 영상 어댑터

상태: **소스에 채택, Windows 컴파일·PoC 게이트 미통과**. 날짜: 2026-09-11. SPEC §4·§12 및 v1.2 §29 보완.

## 문제

v0.1은 WGC/Media Foundation/str0m이라는 기술 후보와 인터페이스만 있었고 실제 영상 파이프라인이 없었다. H.264 캡처/표면 변환/인코딩/RTP/ICE/DTLS를 각각 새로 연결하는 작업은 API·패킷화·수명 위험이 컸다. 사용자 요구는 실제 창/전체 화면을 보되 터미널 입력을 방해하지 않는 것이다.

## 결정

`rc-media`의 `native-media` feature에서 `gstreamer`, `gstreamer-webrtc`, `gstreamer-sdp` Rust 0.24 계열을 사용한다. SDK API 최소 목표는 GStreamer 1.24이며 실제 lockfile/SDK 호환 조합은 Windows 검증 후 고정한다. Agent, 웹 UI, 모바일, IPC 타입은 이 SDK를 링크하지 않는다.

공식 요소를 조합한다:

`d3d11screencapturesrc(capture-api=wgc, window client area 또는 monitor)` → `D3D11Memory BGRA` → `d3d11convert NV12` → `mfh264enc(low-latency,bframes=0)` → `h264parse` → `rtph264pay` → `webrtcbin`.

`str0m` 직접 연결은 이번 활성 어댑터가 아니다. 기존 캡처·encoder·transport 경계는 남겨 다른 어댑터로 교체할 수 있게 한다. 자체 암호화·NAT traversal·원격 로그인 엔진은 만들지 않는다.

## 경량성의 의미와 비용

CLI-only Agent는 SDK 의존성이 없다. 영상/기능 probe를 명시적으로 사용할 때 별도 rc-media 프로세스를 실행한다. 미시청 상태에 상시 capture/encode 루프를 두지 않는다. 영상 버퍼는 1~2프레임, 시그널링은 bounded queue다. 프레임마다 PNG/base64/Tauri IPC를 사용하지 않는다.

반면 SDK 설치·배포 파일 크기, 활성 helper의 RAM/CPU, 드라이버 협상/표면 복사 수는 아직 측정하지 않았다. SDK 전체 DLL을 무차별 번들해서 “가벼움 달성”이라고 하지 않는다. 현재 정지 화면 적응형 fps는 미구현이다. SPEC 목표가 실패하면 trace를 바탕으로 factory 구성/적응형 인코딩 또는 직접 MF 어댑터로 바꾼다. 이 ADR은 성능 기준 완화가 아니다.

## 네트워크·보안

Agent HTTP는 loopback 유지, WebRTC media만 승인 Tailscale IPv4와 제한된 UDP 범위를 사용한다. Host ICE add-local-ip-address, 공개/mDNS/relay candidate 거절, 외부 STUN/TURN 없음. 브라우저가 숫자형 허용 후보를 제공하지 못하면 GUI 불가로 표시한다. 터미널은 살아 있다.

source 로컬 승인 → scoped WS ticket → 한 media owner → helper 시작 → 정상 프레임 → 별도 GUI lease → frame/geometry/권한 검증 → 입력 순서다. 잠금/UAC/local input/timeout/프로세스 변경을 우회하지 않는다.

## 미완료·게이트

Rust API와 GObject signal 타입 검증, Windows factory/encoder 후보 검증, WGC client-area/DPI 매칭, NICE binding 실제 검사, 브라우저 SDP/H264 협상, 두 PC SRTP 수신, 100회 시작/종료, input 안전경합/자원누수, 원본 P4~P6 및 성능 인수표를 통과해야 한다. `probe.available=true`만으로 완료가 아니다.

## 공식 근거

- WGC source/client-area/D3D11: https://gstreamer.freedesktop.org/documentation/d3d11/d3d11screencapturesrc.html
- Media Foundation H264/D3D11 input/low latency: https://gstreamer.freedesktop.org/documentation/mediafoundation/mfh264enc.html
- WebRTC element: https://gstreamer.freedesktop.org/documentation/webrtc/index.html
- ICE local address/port bounds: https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer_webrtc/struct.WebRTCICE.html
- Windows SDK setup: https://gstreamer.freedesktop.org/documentation/installing/on-windows.html
- Licensing advisory: https://gstreamer.freedesktop.org/documentation/application-development/appendix/licensing.html

공식 `latest` Rust API 문서는 확인 시 0.25.3이었다. 소스 manifest의 0.24와 API가 같다고 빌드 없이 가정하지 않는다. 코드는 dependency-resolve 후 0.24 공개 API와 실제 signal signature를 확인하고 수정한다.
