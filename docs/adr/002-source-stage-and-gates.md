> 이 ADR은 v0.1 당시 상태의 기록이다. v0.2 미디어 구현 선택은 ADR 003, 현재 판정은 ../../IMPLEMENTATION_STATUS.md를 따른다. 검증 정직성 원칙은 유지한다.

# ADR 002 — 검증 환경 없는 소스 작성의 경계

현재 환경은 Linux + Node 22.16.0 + TypeScript 5.8.3이다. Rust/Cargo/Windows/설치된 Svelte 의존성이 없고 npm registry DNS 접근이 실패했다.

따라서 이번 전달은 **소스 초안 + 실행된 portable tests + Windows 인수 계획**이다. 컴파일러를 돌리지 않은 Rust를 빌드 성공이라고 하지 않는다. Svelte 전체 번들을 build/check하지 않은 결과를 UI 검증이라고 하지 않는다. GPU 인코딩, 캡처, 원격 데스크톱을 인터페이스만으로 구현 완료라고 하지 않는다.

rc-media는 실제로 capability false를 반환한다. WGC/Media Foundation/WebRTC 통합 전 미디어 프로세스를 자동 실행하지 않는다. GUI input backend source는 있으나 RPC로 도달할 수 없다. 권한/로컬 stop/화면 identity 검증을 먼저 연결해야 한다.

source-stage 목표 수치를 줄이거나 원래 인수 테스트를 삭제하지 않는다. Windows에서 cargo check/fmt/test와 실제 terminal paths를 가장 먼저 수행한다. 컴파일 오류 발견 시 이 문서의 “검증 대기”를 “기능 원래 없음”으로 바꾸지 말고 실제 source를 고친다.
