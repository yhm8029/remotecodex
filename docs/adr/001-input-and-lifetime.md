# ADR 001 — 실행 수명과 입력 영역 분리

상태: 설계 확정 / 구현 실기기 검증 대기

Agent는 UI의 자식 수명으로 실행하지 않는다. UI가 닫혀도 Agent/PTYS가 살아야 한다. 반면 Agent 자체 종료/재부팅 후 살아 있지 않은 OS 프로세스를 세션 파일로 “살아 있다”고 표시하지 않는다.

입력은 (a) UI 로컬 조작, (b) 선택 PTY의 byte input, (c) 전역 Windows desktop input의 세 영역이다. 마우스 버튼이 같아도 전송 계층을 섞지 않는다. TUI mouse reporting은 PTY에만 입력한다.

PTY lease는 client_id + connection_id + lease_epoch에 묶는다. 동일 기기 다른 브라우저 탭도 독립 연결이다. 입력은 server receive timestamp, generation, sequence, input_id를 가진다. accepted는 written이나 executed가 아니다. ACK가 끊긴 입력은 절대로 자동 재전송하지 않는다.

GUI lease는 PTY별 lease와 다르게 실제 Windows interactive desktop 전체의 단일 제어권이다. 창 두 개를 캡처해도 Windows의 foreground/cursor가 둘이 되는 것은 아니다. 실제 로컬 사용자는 원격보다 우선이며 물리 마우스를 막는 BlockInput이나 UAC 우회는 사용하지 않는다.
