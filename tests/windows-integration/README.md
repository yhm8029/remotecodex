# Windows 실기기 검증 — 아직 실행하지 않음

`cargo test -p rc-agent --test conpty_smoke -- --ignored --nocapture`는 실제 Windows CMD를 실행한다. 이 단일 smoke test 통과만으로 아래 통합 검증이 통과하는 것은 아니다.

1. 회사 Agent 실행 → 로컬 브라우저 등록 → CMD/PowerShell 각각 생성 → 각자 다른 marker 출력.
2. 집 브라우저를 별도 기기로 등록하고 같은 세션 ID/PID/agent epoch 확인.
3. 회사 Tauri UI 종료, 집 브라우저 종료, Wi-Fi 끊기, 재접속: 회사 PTY 유지 확인.
4. Sales/BeforeTalk 분할 후 빠른 1,000회 focus 전환. 각 입력 marker가 다른 PTY로 절대 들어가지 않는지 수집.
5. 같은 기기 브라우저 탭 두 개: 하나만 writer. 강제 제어권 이전 시 이전 연결의 queued input 거절.
6. `Ctrl+C`와 대량 출력 동시 발생. ACK의 receive_to_write와 외부 키→렌더 지연을 따로 측정.
7. 실제 Codex 버전, 한국어 IME, 한글+영문+emoji, 긴 줄, alt screen, resize, cursor mode, scroll region, reconnect golden 확인.
8. 창/데스크톱 capability가 false인지 확인. 미구현 캡처 기능을 완료로 오인하지 않는다.
9. 프리뷰는 Vite/Next/SSE/WS 각각 실제 dev server로 테스트하고 trace에서 management bearer/cookie 유출 여부 검사.
10. Tailscale Serve 실제 헤더, HTTPS 인증서, WebSocket upgrade, 방화벽 정책, 별도 Windows 사용자 pipe 거부 확인.

원시 로그에 pairing ticket, Authorization, 사용자 코드/명령 전체를 기록하지 않는다. 식별 가능한 테스트 marker만 사용한다.
