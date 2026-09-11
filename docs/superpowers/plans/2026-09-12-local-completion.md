# 이 PC에서 가능한 잔여 작업 실행 계획

**목표:** 사용자 승인에 따라 현재 PC에서 가능한 계측·부하·복원·라이선스 검증을 완료하고, 실행 결과와 외부 조건을 구분한다.
**구조:** 기존 isolated worktree를 사용한다. 주 에이전트가 작은 M3 코드 생성을 직접 실행하고 Sol은 성능 설계, Luna는 라이선스 조사, Astra는 필요한 보안/수명 변경 리뷰를 맡는다.
**기술:** 기존 Rust Agent/Windows ConPTY, Svelte/xterm, PowerShell CIM, Node/Playwright, 로컬 MSVC/GStreamer.

## 수행 순서

- [x] 실제 dependency inventory 84개를 조사하고 직접 포함/공통 상위 license/현재 패키지 미포함/미해결 상태와 근거를 명시한다. 임의 텍스트 복제나 license 이름만으로 해결 처리하지 않는다.
- [x] PowerShell resource sampler에 PID+생성시간 검증, private working set, CPU machine-normalization, 제품/개발작업/Tailscale/합계와 raw sample 보존을 추가한다. 짧은 fixture로 결측·PID 재사용 처리 검증 후 실제 0/2/8 PTY 유휴 10분 측정을 실행한다.
- [x] 기존 input ACK의 agent_receive_to_write_us를 사용한다. 출력·클라이언트 parse/apply·enqueue는 명시적 진단 모드의 bounded 계측만 추가한다. 다른 프로세스 wall-clock 차감과 parse callback의 paint 오표시를 금지한다.
- [x] 소유 temp workload로 정상/다른 PTY 출력 스트레스의 10,000회 입력 분포, 1MiB/s 10분과 5MiB/s 5초, 100회 재연결, 500회 생성·종료를 실행한다. 테스트 시간·실제 달성 처리율·누락도 함께 기록한다.
- [x] 현재 PC에서 가능한 UI 수명 반복, 새 패키지 포함 파일/manifest/license/SBOM 검사, localhost 프리뷰/브라우저/입력 검증을 마무리한다.
- [x] 실패는 원인 분석과 좁은 M3 수정 후 같은 조건으로 재검증한다. 실제 사용자 프로세스·파일·OS 설정은 바꾸지 않는다.
- [x] 최신 검사, 문서/인수 표/실행 증거 갱신, 커밋·push와 원격 HEAD 확인.

## 완료 판단

현재 PC에서 실행 가능한 작업은 실제로 실행하거나, 구체적인 장비/권한/시간 경계와 관측 증거를 남긴다. 물리 iOS/Android, 두 실제 PC/tailnet, 깨끗한 VM, 24시간 경과가 필요한 인수는 존재하지 않는 성공으로 대체하지 않는다. 이 계획은 이미 승인된 SPEC17과 기존 구현 계획의 실행 세분화이며 새 사용자 승인 단계가 아니다.

## Execution evidence

See `docs/test-results/pc-completion-2026-09-12/README.md` for measured results and limits. Local implementation, bounded checks, packaging and review are complete. The24-hour run launched2026-09-11T21:32:05Z; this checklist records launch only, not a full-duration pass. Physical phones, secondPC/tailnet, cleanVM and reliableGPUallocation verification remain external gates. Two exact upstream license texts remain unresolved. Commit/push verification is recorded by final delivery.
