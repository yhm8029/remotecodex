# 변경 기록

## 0.2.0 / SPEC 1.2 — 2026-09-11

- 기존 v0.1을 보존하고 256KiB UTF-8 paste 전용 HTTP 입력/검사/8KiB write 경로, Ctrl+C 우선 처리, 같은 세션 paste gate를 추가.
- 이전 connect/reload/WS 이벤트가 새 연결 상태를 덮어쓰는 경합을 방지. 모호한 입력 자동 재전송 금지 유지.
- 서버 headless sync-update 처리, pending-wrap 및 dynamic palette snapshot 코드를 보강. 완전 호환·Rust 검증은 아직 아님.
- P4/P5/P6 실제 source discovery/로컬 승인/scoped WS/helper/영상 수신/UI/GUI lease/native input pipeline을 소스로 연결.
- 선택적 GStreamer WGC→D3D11→MF H264→WebRTC 어댑터. CLI-only Agent와 분리. SDK와 바이너리는 미포함.
- DPI guard, 회사 물리 입력 우선, 눈에 보이는 공유 표시창·긴급 단축키, generation/geometry/fresh-frame·키 release 보호 코드 추가.
- 테스트 51 → **125개 PASS**. 실제 native/video/Windows/performance 합격으로 간주하지 않음.
- Windows source build, 실제 Agent E2E, media factory probe, WinForms 수동 GUI fixture를 추가. 스크립트는 여기서 미실행.
- SPEC/상태/인수표/ADR/참고자료/후속 Codex 지시 업데이트. 설치 EXE/MSI/원격 저장소 publication 없음.

## 0.1.0

기존 터미널/인증/프리뷰 source alpha, portable/client 51개 테스트. 원본 테스트 증거는 docs/test-results의 기존 파일에 유지했다.
