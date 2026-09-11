# 실제 검증 증거

## v0.2.0 — 최신

- `v0.2.0/all-tests.tap.txt`: 실제 `npm test` 원시 로그. core 96 + client 29 = **125 PASS**.
- `v0.2.0/environment.json`: 실제 Node/TypeScript/Linux, Cargo/Rust/PowerShell 미설치 및 native/E2E/performance 미실행 상태.
- `v0.2.0/source-validation.json`: JSON/TOML, MJS syntax, 상대 문서 링크 검사. Rust compile 또는 PowerShell 실행 검증이 아니다.
- `v0.2.0/results.json`: 러너 출력에서 계산한 test count와 실행 범위.

## v0.1.0 — 보존한 이력

루트의 `all-tests.tap.txt`, `core.tap.txt`, `client.tap.txt`, `environment.txt`, `source-structure.json`, `npm-resolution.txt`는 v0.1 증거다. `core.initial.tap.txt`의 초기 test harness 실패도 보존했다. 이를 최신 성공 결과로 혼동하지 않는다.

## 아직 실행하지 않은 검사

Windows/Cargo/ConPTY/Svelte 전체/실제 WebRTC/모바일/2PC/CPU·RAM·p95/24h soak는 NOT_RUN이다. 앞으로 실제 Windows 스크립트를 실행하면 `local-e2e`, `local-media` 같은 별도 폴더에 결과가 생성된다. 가상의 Windows 결과나 목표값을 측정 CSV에 채워 넣지 않았다.
