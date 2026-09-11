# 참고 프로젝트와 적용 범위

자료 확인일: 2026-09-11. 아래 스타 수는 v0.1 조사 기록의 GitHub 페이지에 표시된 **반올림 수치**이며 고정 수치가 아니다. 스타 수만으로 안전성·품질을 보증하지 않는다.

| 프로젝트 | 관측 스타 | 실제 적용 범위 | 라이선스/취급 |
|---|---:|---|---|
| Tauri | 약 111k | Rust + OS WebView 기반 데스크톱 UI, local capability 분리 | MIT / Apache-2.0. 프레임워크 의존성 |
| WezTerm / portable-pty | 약 28.8k (WezTerm) | PTY 호스트·다중 세션 구조 참고, portable-pty 0.9.0 직접 의존성 | 상위 프로젝트/각 crate 라이선스를 배포 때 확인; portable-pty 라이선스 포함 필요 |
| xterm.js | 약 21.2k | 실제 브라우저 터미널, onData/onBinary, mouse protocol, 흐름 제어 | MIT. 직접 의존성 |
| Happy | 약 23.7k | 모바일 CLI/세션 선택/명령 입력 경험의 참고 | MIT. 코드를 fork하거나 앱 전체 구조를 복제하지 않음 |
| RustDesk | 약 123.1k | 터미널과 다른 GUI 영상/입력 영역의 구조적 참고 | AGPL-3.0. 소스 복제·링크·vendoring 없음 |
| Alacritty terminal engine | 이번 문서에 스타 수 기재 안 함 | VT parser/grid/cursor/mode API를 직접 확인하고 state adapter 통합 후보로 사용 | 직접 의존성. 해당 crate 라이선스를 배포 때 포함 |

## 원본 URL

- https://github.com/tauri-apps/tauri
- https://github.com/wezterm/wezterm
- https://docs.rs/portable-pty/0.9.0/portable_pty/
- https://github.com/xtermjs/xterm.js
- https://xtermjs.org/docs/guides/flowcontrol/
- https://github.com/slopus/happy
- https://github.com/rustdesk/rustdesk
- https://github.com/alacritty/alacritty
- https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event/enum.Event.html
- https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html
- https://v2.tauri.app/start/prerequisites/

## 직접 읽은 코드

Alacritty 참고 소스 ref: `d692748d3f61253ebe9f5094320120d22f6a046f`.

`alacritty_terminal/src/term/mod.rs`, `grid/mod.rs`, `lib.rs`의 공개 API/구조를 GitHub 연결 도구로 읽었다. 이 ref는 검토한 상위 저장소 snapshot 식별자이며, crates.io 0.25.1 tarball의 checksum이라고 주장하지 않는다. dependency lock과 실제 API 일치 여부는 사용자 Windows에서 compile gate로 확인해야 한다.

## 재사용의 의미

PTY·터미널 parser·브라우저 renderer를 처음부터 다시 쓰지 않고 공개 라이브러리 인터페이스를 사용한다. 코드가 실제로 복제되지 않은 참고 프로젝트에 대해 “그 프로젝트 코드를 가져와 검증 완료했다”고 표시하지 않는다. Happy의 UI를 참고했다는 이유로 Happy가 실행 프로세스를 다른 PC로 이동시킨다고 규정하지 않는다.

dependency resolution 실패로 전체 transitive SBOM/lockfile은 아직 없다. 실제 lockfile 생성 후 라이선스·보안 감사를 수행한다. RustDesk의 AGPL 소스를 이후에 가져오려면 별도 라이선스 검토와 프로젝트 라이선스 결정을 먼저 해야 한다.

## v0.2 추가 참고·의존성

- GStreamer d3d11screencapturesrc / mfh264enc / webrtcbin / WebRTCICE: 실제 요소 API를 호출하는 선택적 네이티브 어댑터. 문서와 Rust binding 버전 차이는 실제 compile gate로 확인.
- https://gstreamer.freedesktop.org/documentation/d3d11/d3d11screencapturesrc.html
- https://gstreamer.freedesktop.org/documentation/mediafoundation/mfh264enc.html
- https://gstreamer.freedesktop.org/documentation/webrtc/index.html
- https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer_webrtc/struct.WebRTCICE.html
- https://gstreamer.freedesktop.org/documentation/installing/on-windows.html
- https://gstreamer.freedesktop.org/documentation/application-development/appendix/licensing.html
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext
- Alacritty `term/color.rs` blob `66753deb3cfbc42a6417bfbcb5f46db1f963ca05`: 공개 palette index 확인.
- VTE `ansi.rs` ref `abeae765dd546dfff60b278f0757dcc71beb8ab1`: `Processor::stop_sync`/`sync_bytes_count` 공개 API 확인.

SDK/코덱은 외부 라이선스와 배포 검토 대상이며 RemoteCodex MIT만으로 모두 허용된다고 판단하지 않는다. 이번에 upstream 소스를 vendoring한 것은 없고 공개 라이브러리/요소를 호출한다. 새 스타 수를 추측해 추가하지 않았다.
