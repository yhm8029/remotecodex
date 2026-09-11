# RemoteCodex 인수·회귀 테스트 매트릭스

> 기준: SPEC.md v1.2 · 작성일: 2026-09-11
> **이 매트릭스는 제품 통합 인수 계획이다. 모든 제품 인수 항목은 NOT_RUN이다. 별도의 portable/client 단위 테스트 125개 결과는 IMPLEMENTATION_STATUS.md에 기록했으며 제품 전체 통과로 승격하지 않는다.**

## 사용 규칙

실제 구현 후 각 ID에 환경·commit·절차·원시 로그/trace·판정 근거를 연결한다. 자동화 fixture와 실기기 검증을 구분한다. 해당 단계의 필수 항목이 FAILED/NOT_RUN이면 VERIFIED 릴리스로 보고하지 않는다.

상태: `NOT_RUN` / `PASS` / `FAIL` / `BLOCKED` / `NOT_APPLICABLE_WITH_REASON`. 구현 완료 여부는 별도 IMPLEMENTATION_STATUS.md에서 추적한다. 이유 없는 N/A·목표값 완화·테스트 삭제는 허용하지 않는다.

성능 숫자는 SPEC의 조건·측정 정의와 함께 적용하는 설계 목표다. 테스트가 존재하는 것과 목표 달성은 다르다. 테스트 환경이 없다면 BLOCKED 또는 NOT_RUN으로 기록한다.

## BASE — 기반 구조

단계: **P0/P1** · 근거: SPEC §4–8

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| BASE-001 | 실제 ConPTY 왕복 | Windows에서 CMD와 PowerShell을 각각 생성하고 echo/dir 명령 입력 | 실제 자식 PID와 출력 증거가 있고 메시지마다 프로세스를 다시 만들지 않음 | NOT_RUN |
| BASE-002 | 독립 Agent 수명 | 터미널 작업 중 회사 Tauri UI를 종료·재실행 | Agent/PTY PID·generation이 유지되고 작업 계속 진행 | NOT_RUN |
| BASE-003 | 외부 터미널 비탈취 | 일반 Windows Terminal 탭 실행 후 RemoteCodex 시작 | 기존 탭을 몰래 attach/종료/재시작하지 않음 | NOT_RUN |
| BASE-004 | Agent 단일 인스턴스 | 동일 사용자에서 앱을 빠르게 두 번 시작 | Agent 1개, 소유 PTY 중복 생성 없음 | NOT_RUN |
| BASE-005 | 사용자 경계 | 다른 Windows 사용자로 named-pipe 관리 명령 시도 | 허용되지 않은 사용자는 거절, 정보 노출 없음 | NOT_RUN |
| BASE-006 | 종료 명령 구분 | UI 닫기/터미널 닫기/Agent 종료를 각각 실행 | SPEC 수명 표대로 해당 범위만 종료 | NOT_RUN |
| BASE-007 | Agent 충돌 표시 | 테스트 전용 세션에서 Agent 강제 종료 후 재시작 | 이전 세션 lost, 기존 실행이 유지된 것처럼 표시하지 않음 | NOT_RUN |
| BASE-008 | 프로세스 재사용 | 종료한 PID와 동일 숫자가 재사용되는 fixture 실행 | creation_time/generation 불일치로 이전 권한·세션 재사용 거절 | NOT_RUN |

## SEC — 인증과 보안

단계: **P1+** · 근거: SPEC §6–7, §18

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| SEC-001 | 무인증 차단 | 토큰 없이 REST/WS/터미널 생성·입력 호출 | 자원 정보·명령 실행 없이 401/동등 오류 | NOT_RUN |
| SEC-002 | tailnet만으로 인증 불가 | 같은 tailnet의 미등록 장치에서 접속 | 관리 API와 터미널 접근 거절 | NOT_RUN |
| SEC-003 | pairing 재사용 | 소비한 티켓과 만료한 티켓으로 장치 등록 재시도 | 모두 거절, 기존 장치 권한 유지 | NOT_RUN |
| SEC-004 | challenge 재전송 | 서명 응답을 재사용하거나 다른 host/audience에 제출 | nonce·audience 검사로 거절 | NOT_RUN |
| SEC-005 | 권한별 차단 | 읽기 장치로 write/create/close/gui-control 호출 | 서버 측 scope 검사로 거절 | NOT_RUN |
| SEC-006 | 장치 철회 | 활성 터미널/프리뷰/미디어 장치를 로컬에서 revoke | 활성 연결·lease·프리뷰 권한 종료, 재입력 불가 | NOT_RUN |
| SEC-007 | 악성 origin | 다른 origin 페이지에서 API/WS/CSRF 요청 시도 | 인증·Origin·Host 검사로 차단 | NOT_RUN |
| SEC-008 | 로컬 네트워크 미노출 | LAN 인터페이스와 인터넷에서 관리 포트 확인 | 승인한 tailnet 경로 외 노출 없음 | NOT_RUN |
| SEC-009 | 토큰 노출 검사 | 로그·URL·localStorage·오류·프로세스 인자를 검색 | 장기 bearer/개인키/일회용 ticket 노출 없음 | NOT_RUN |
| SEC-010 | Tauri IPC 격리 | 악성 preview 페이지에서 Tauri invoke와 파일 API 호출 | 원격 페이지에 privileged capability 없음 | NOT_RUN |
| SEC-011 | 출력 기반 공격 | HTML/OSC52/위험 URL/긴 OSC-DCS를 출력하는 fixture 실행 | HTML 실행·클립보드 무단 변경·임의 파일 열기 없음 | NOT_RUN |
| SEC-012 | 과대 입력 방어 | 제한 초과 HTTP body/WS frame/paste 전송 | bounded 거절, Agent 정상·메모리 상한 유지 | NOT_RUN |
| SEC-013 | rate limit | 인증/challenge/pairing 요청 폭주 | rate limit·상한 동작, 정상 승인 클라이언트 입력 경로 유지 | NOT_RUN |
| SEC-014 | 기존 Serve 설정 보존 | 다른 로컬 서비스를 등록한 상태에서 설치·해제 | RemoteCodex 소유 규칙만 변경, 다른 서비스 보존 | NOT_RUN |
| SEC-015 | 보안제품 차단 | 서명/보안제품 차단 환경을 검사하고 오류 경로 실행 | 우회·예외 등록 없이 정확한 상태/진단 제공 | NOT_RUN |

## TERM — 터미널 정확성

단계: **P1/P2** · 근거: SPEC §8–10

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| TERM-001 | 한글 조합 | 한글 입력·변환·Backspace·조합 중 Enter를 반복 | 중복 문자·조기 전송·분해 입력 없음 | NOT_RUN |
| TERM-002 | 한글 경로 | 공백/한글/괄호가 포함된 cwd에서 명령 실행 | 경로 손실·인자 잘못 해석 없음 | NOT_RUN |
| TERM-003 | UTF-8 분할 | 한글/emoji bytes를 모든 가능한 경계로 나눠 전송 | 연결된 원문과 동일한 문자 출력 | NOT_RUN |
| TERM-004 | 문자 폭 | CJK/emoji/조합문자/혼합 ASCII fixture 출력 | 선언한 Unicode profile에서 커서·열 정합 | NOT_RUN |
| TERM-005 | 기본 제어키 | Tab·방향키·Home/End·Delete·PgUp/PgDn 테스트 | 대상 터미널 동작과 일치 | NOT_RUN |
| TERM-006 | Ctrl+C | 출력이 있는 장기 실행 fixture에 Ctrl+C 전송 | 입력 전달 시간 측정, Agent/다른 세션은 종료되지 않음 | NOT_RUN |
| TERM-007 | 붙여넣기 | 한 줄/여러 줄/큰 텍스트를 붙여넣기 | bracketed paste 정책 준수, 중복 Enter·잘림 없음 | NOT_RUN |
| TERM-008 | 붙여넣기 중 철회 | 전송 도중 lease 철회 | 남은 입력 차단, 이미 전달된 부분을 성공 취소라고 오표시하지 않음 | NOT_RUN |
| TERM-009 | ANSI/TUI | 커서 이동·색·scroll region·progress/alternate fixture 실행 | golden state와 일치 | NOT_RUN |
| TERM-010 | Codex TUI 실증 | 실제 설치된 Codex로 대화·도구 출력·승인 UI 사용 | raw CLI 경험 유지, mock로 통과 처리하지 않음 | NOT_RUN |
| TERM-011 | 쿼리 응답 단일화 | DA/DSR fixture를 회사·집·모바일 동시 구독으로 실행 | 서버 응답 1회, 클라이언트별 중복 응답 없음 | NOT_RUN |
| TERM-012 | 무구독 질의 | 시청자 없이 질의와 출력을 수행 | 지원 질의 응답·출력 소비 유지 | NOT_RUN |
| TERM-013 | snapshot 중 query | 질의가 있는 TUI를 snapshot 복원 | 과거 질의 응답이 다시 실제 입력으로 전송되지 않음 | NOT_RUN |
| TERM-014 | 관리형 Codex 종료 | 관리형 Codex가 종료된 상태에서 작성창 확인 | 자연어가 바깥 셸로 자동 전달되지 않음 | NOT_RUN |
| TERM-015 | final 오탐 방지 | final 형태 문자열과 일시적 출력 중단 fixture 실행 | 프로세스 종료·작업 완료로 단정하거나 자동 재시작하지 않음 | NOT_RUN |
| TERM-016 | 종료 drain | 출력 중 PTY 정상/강제 종료를 반복 | deadlock·유실 handle·Agent 전체 정지 없음 | NOT_RUN |

## MULTI — 다중 세션과 복원

단계: **P2** · 근거: SPEC §9–10, §19

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| MULTI-001 | 8세션 분리 | 8개 PTY에서 고유 식별자 출력·입력 | cross-session 출력/입력 0건 | NOT_RUN |
| MULTI-002 | 동일 label | 서로 같은 이름의 세션을 여러 개 생성 | UUID 기준으로 정확히 구분 | NOT_RUN |
| MULTI-003 | 프로젝트 다중 PTY | 프로젝트 하나에 Codex·개발 서버 세션 생성 | 프로젝트 그룹은 같고 실행 수명은 독립 | NOT_RUN |
| MULTI-004 | writer 경쟁 | 두 클라이언트가 동시에 lease 획득/입력 | 서버 원자적 단일 writer, 이전 lease 입력 차단 | NOT_RUN |
| MULTI-005 | 로컬 회수 | 집에서 작성 중 회사 로컬 UI가 제어권 회수 | 원격 이전 lease가 즉시 무효화 | NOT_RUN |
| MULTI-006 | 모바일 read-only resize | 넓은 PTY를 모바일에서 읽기만 함 | PTY cols/rows 변동 없음 | NOT_RUN |
| MULTI-007 | 사용자 resize | writer가 빠르게 창 크기를 변경 | 순서·크기 정합, 동일 resize no-op | NOT_RUN |
| MULTI-008 | 빠른 delta reconnect | ring 내 범위에서 네트워크 단절 후 재접속 | 누락·중복 없이 순번 이어짐 | NOT_RUN |
| MULTI-009 | 새 페이지 snapshot | TUI 실행 중 새 브라우저에서 연결 | 커서·alternate·모드·화면 복구 | NOT_RUN |
| MULTI-010 | ring 초과 reconnect | 단절 중 ring보다 많은 출력 생성 | 최신 snapshot으로 복구, 잘린 이력 표시 | NOT_RUN |
| MULTI-011 | 복구 중 출력 | snapshot 전송 중 resize/출력 지속 | S 시점 snapshot과 이후 tail 이벤트가 일관됨 | NOT_RUN |
| MULTI-012 | 느린 구독자 | 모바일 수신/ACK를 느리게 하고 집 클라이언트 유지 | 느린 구독자만 resync, 회사 작업/다른 클라이언트 지속 | NOT_RUN |
| MULTI-013 | ACK 분실 | 입력 write 직후 ACK 연결을 끊음 | 명령 자동 중복 실행 없음, 불명확 전달 상태 표시 | NOT_RUN |
| MULTI-014 | generation 거절 | 세션 재시작 뒤 이전 input frame 전달 | generation mismatch 거절 | NOT_RUN |
| MULTI-015 | 100회 재접속 | 네트워크 reconnect를 100회 반복 | 세션 유지·중복 구독/버퍼/핸들 누수 없음 | NOT_RUN |
| MULTI-016 | 24시간 무클라이언트 | 실제 호스트에서 24시간 작업 후 재연결 | 작업 수명 유지, 현재 상태와 이력 한계를 정확히 표시 | NOT_RUN |

## WEB — 웹 프리뷰

단계: **P3** · 근거: SPEC §11

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| WEB-001 | 후보와 승인 분리 | 터미널에 localhost URL만 출력 | 승인 전 외부 프리뷰 열리지 않음 | NOT_RUN |
| WEB-002 | loopback 서비스 | 127.0.0.1에만 bind한 개발 서버 등록 | 집에서 승인된 프리뷰로 접근 가능 | NOT_RUN |
| WEB-003 | 등록되지 않은 포트 | 등록되지 않은 port/id로 요청 | 접근 거절 | NOT_RUN |
| WEB-004 | SSRF/순환 차단 | 외부 IP·회사 LAN·Agent 관리 포트·CONNECT 요청 | 범용 proxy/관리 재귀 접근 불가 | NOT_RUN |
| WEB-005 | 프로세스 바뀜 | 서버 종료 후 다른 프로그램으로 동일 port 사용 | 기존 공개 권한 자동 재사용하지 않음 | NOT_RUN |
| WEB-006 | origin 분리 | 프리뷰에서 관리 origin DOM/storage/API 접근 시도 | SOP·인증·CORS 경계 유지 | NOT_RUN |
| WEB-007 | Vite HMR | 실제 Vite 앱에서 코드 수정/자원 갱신 | 페이지와 HMR WS가 올바른 원격 주소로 작동 | NOT_RUN |
| WEB-008 | Next.js 개발 | 실제 Next.js dev에서 화면/route 수정 | 지원 버전에서 hot reload·asset·refresh 정상 | NOT_RUN |
| WEB-009 | root asset/refresh | 절대 /asset·deep route·브라우저 새로고침 | path-prefix 깨짐 없이 정상 | NOT_RUN |
| WEB-010 | HTTP 스트리밍 | SSE·chunked·range·큰 multipart 요청 | streaming 동작·메모리 전체 적재 없음 | NOT_RUN |
| WEB-011 | WS 인증 철회 | 연결 중 preview/device revoke | 활성 HTTP/WS 권한 철회 | NOT_RUN |
| WEB-012 | 관리 credential 격리 | 업스트림에서 headers/cookies를 검사 | 관리 bearer·bootstrap secret 전달 없음 | NOT_RUN |
| WEB-013 | cookie 네임스페이스 | 두 프로젝트의 같은 이름 쿠키를 교차 요청 | Gateway가 승인된 preview jar만 전달, 한계 명시 | NOT_RUN |
| WEB-014 | script-cookie 제한 | document.cookie/OAuth 의존 앱 검증 | 호환되지 않는 경우 제한/별도 hostname 안내, 거짓 정상 없음 | NOT_RUN |
| WEB-015 | iframe 차단 | X-Frame-Options/CSP가 있는 앱 미리보기 | 보안 헤더 우회 없이 새 탭 경로 제공 | NOT_RUN |
| WEB-016 | localhost 절대 API | 브라우저 코드에 localhost API/WS가 있는 앱 | 잘못된 집 PC 호출을 진단하고 명시적 설정 가이드 제공 | NOT_RUN |
| WEB-017 | slot 재사용/SW | 이전 프리뷰 저장소·service worker가 있는 origin 재사용 시도 | 이전 프로젝트 코드가 새 프로젝트 권한을 승계하지 않음 | NOT_RUN |

## VIDEO — 창 영상

단계: **P4** · 근거: SPEC §12

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| VIDEO-001 | 일반 EXE 창 | 승인된 테스트 EXE의 실제 UI 표시 | 실제 창의 동작 화면을 전달, snapshot을 live라고 오표시하지 않음 | NOT_RUN |
| VIDEO-002 | 비승인 창 | 메신저 등 다른 프로그램이 떠 있는 상태에서 목록 요청 | 승인되지 않은 창 내용/캡처 자동 공개 없음 | NOT_RUN |
| VIDEO-003 | 창 크기 변경 | 캡처 중 window resize와 DPI 이동 | frame pool/geometry 갱신, 깨진 surface 재사용 없음 | NOT_RUN |
| VIDEO-004 | 최소화/복원 | 캡처 대상 최소화·복원 | 상태/갱신 시각 표시, 정지 프레임을 live로 표시하지 않음 | NOT_RUN |
| VIDEO-005 | 원본 종료 | 창과 대상 프로세스 종료 | source_closed, 자원 정리, 다른 재사용 HWND로 전환 없음 | NOT_RUN |
| VIDEO-006 | GPU 장치 유실 | device-lost fixture 또는 안전한 재현 환경 사용 | 복구/오류 분리, PTY는 유지 | NOT_RUN |
| VIDEO-007 | 잠금 | Windows 잠금 상태 전환 | GUI 일시 정지/표시, 잠금 해제 우회 없음 | NOT_RUN |
| VIDEO-008 | 구독자 0 | 마지막 미디어 viewer 종료 | 2초 내 capture/encoder 해제 목표, 10초 내 helper 종료 목표 | NOT_RUN |
| VIDEO-009 | 하드웨어 인코더 | 지원 iGPU에서 codec 경로·GPU 지표 측정 | 실제 HW 사용 증거, 단순 encoder 생성 성공만으로 판정하지 않음 | NOT_RUN |
| VIDEO-010 | 하드웨어 미지원 | HW unavailable 조건으로 실행 | 해상도/fps 제한 또는 명시적 degraded mode | NOT_RUN |
| VIDEO-011 | tailnet ICE | 실제 두 PC/브라우저에서 영상 연결·재연결 | 승인한 경로에서 동작, 불필요한 외부 TURN 자동 추가 없음 | NOT_RUN |
| VIDEO-012 | 영상 연결 실패 | 미디어 UDP를 제한하고 터미널 연결 유지 | GUI 실패만 표시, 터미널 입력·출력 유지 | NOT_RUN |

## INPUT — 창 입력

단계: **P5** · 근거: SPEC §13

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| INPUT-001 | 보기 전용 | window.view만 있는 장치에서 입력 전송 | 모든 GUI mutation 거절 | NOT_RUN |
| INPUT-002 | GUI 단일 작성자 | 서로 다른 창을 두 장치가 동시에 제어 요청 | Windows desktop 전체 기준 GUI lease 1개 | NOT_RUN |
| INPUT-003 | 클릭 좌표 | 영상 letterbox/브라우저 zoom에서 격자 버튼 클릭 | 테스트 fixture의 의도 좌표로 정확히 입력 | NOT_RUN |
| INPUT-004 | DPI 매트릭스 | 100/125/150/200%에서 클릭/드래그 | 배율·클라이언트/화면 좌표 정합 | NOT_RUN |
| INPUT-005 | 구 geometry | resize/모니터 이동 이전 프레임 좌표 전송 | SOURCE_STALE/동등 오류, 잘못된 클릭 없음 | NOT_RUN |
| INPUT-006 | foreground 변경 | 로컬 앱이 focus를 가져간 직후 원격 입력 | 원격 입력 정지·알림, 재현되는 오입력 결함은 차단 | NOT_RUN |
| INPUT-007 | modifier 종료 | Ctrl/Shift/mouse down 상태에서 연결 종료 | 제품이 주입한 key/button만 release, 고착 없음 | NOT_RUN |
| INPUT-008 | 로컬 회수 | 트레이/긴급 단축키로 원격 제어 중지 | 입력 즉시 중단, viewer 상태 갱신 | NOT_RUN |
| INPUT-009 | 한글 텍스트 | 입력 테스트 EXE에서 한글·emoji·붙여넣기 | 문자 손실·double injection 없음 | NOT_RUN |
| INPUT-010 | 권한 높은 앱 | 일반 권한 Agent에서 관리자 앱/UAC 화면 제어 시도 | 제한 표시·실패 처리, 우회하지 않음 | NOT_RUN |
| INPUT-011 | 정지된 영상 입력 | 영상이 오래 갱신되지 않은 상태로 클릭 | 정책에 따라 입력 비활성화, 오래된 장면 클릭 방지 | NOT_RUN |
| INPUT-012 | clipboard 기본값 | 새 설치·새 장치에서 클립보드 변경 | 자동 동기화 OFF, 사용자 명시 조작만 허용 | NOT_RUN |

## DESK — 전체 데스크톱

단계: **P6** · 근거: SPEC §14

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| DESK-001 | 전체 화면 권한 | window 권한만 있는 장치로 desktop 요청 | desktop.view/control 별도 허가 필요 | NOT_RUN |
| DESK-002 | 모니터 전환 | 두 모니터 사이 보기/제어 변경 | 이전 stream/geometry 정리, 선택 모니터로 동작 | NOT_RUN |
| DESK-003 | 음수 좌표/회전 | 왼쪽 배치·회전·다른 DPI 모니터에서 조작 | 좌표 mapping 검증 | NOT_RUN |
| DESK-004 | 해상도 변경 | 원격 연결 중 OS 해상도 변경 | source geometry 재동기화, 구 좌표 입력 거절 | NOT_RUN |
| DESK-005 | Apps/Desktop 전환 | 창 제어와 전체 화면 제어 전환 | 공유 GUI lease 준수, 이중 입력 없음 | NOT_RUN |
| DESK-006 | 잠금/로그아웃 | 잠금 및 로그아웃 조건에서 접속 | 지원 범위 명확히 표시, 로그인 세션 생성/우회 없음 | NOT_RUN |
| DESK-007 | 미사용 전체 화면 | Desktop을 보지 않고 CLI만 사용 | capture/encoder 상주하지 않음 | NOT_RUN |
| DESK-008 | 전체 정보 노출 안내 | 최초 전체 데스크톱 권한 승인 | 전체 업무 화면 노출 범위와 제어 상태가 명확히 표시 | NOT_RUN |

## MOB — 모바일 실기기

단계: **P2+** · 근거: SPEC §15

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| MOB-001 | Android IME | Android Chrome에서 한국어 입력/전송/삭제 | 조합 중 전송·중복 입력 없음 | NOT_RUN |
| MOB-002 | iOS IME | iPhone Safari에서 한국어 입력/전송/삭제 | 조합 중 전송·중복 입력 없음 | NOT_RUN |
| MOB-003 | 가상키보드 viewport | 키보드 열기/닫기·브라우저 bar 변화 | 작성창/전송 버튼을 사용할 수 있음 | NOT_RUN |
| MOB-004 | 회전 | 세로↔가로 전환 중 출력/초안 유지 | 세션/초안 유지, 강제 PTY resize 없음 | NOT_RUN |
| MOB-005 | background 복귀 | 앱 background 후 foreground 복귀 | 회사 작업 유지, snapshot/delta 재접속 | NOT_RUN |
| MOB-006 | 네트워크 전환 | Wi-Fi↔모바일 데이터 이동 | 연결 상태 표시, 입력 자동 중복 재전송 없음 | NOT_RUN |
| MOB-007 | 다중 세션 이동 | Sales/BeforeTalk 사이 이동하며 초안 작성 | 초안/입력이 session_id에 바인딩, 오발송 없음 | NOT_RUN |
| MOB-008 | 원문과 읽기 모드 | 일반 로그와 alternate TUI를 전환 | 의미를 꾸며내지 않고 불완전 projection은 원문으로 안내 | NOT_RUN |
| MOB-009 | 스크롤 유지 | 위 로그 읽는 중 새 출력 수신 | 강제 bottom jump 없이 새 출력 수 표시 | NOT_RUN |
| MOB-010 | 구독 개수 | 여러 세션이 있는 상태에서 모바일 대기 | 고빈도 terminal stream 기본 1개 | NOT_RUN |
| MOB-011 | 화면 폭 | 넓은 PTY를 좁은 휴대폰에서 열기 | 가로 이동/확대 가능, 회사 레이아웃 보존 | NOT_RUN |
| MOB-012 | 오프라인 초안 | 연결 없는 상태로 작성 후 재접속 | 초안 보존 정책 준수, 사용자 확인 없이 전송하지 않음 | NOT_RUN |

## PERF — 성능과 내구성

단계: **P1+** · 근거: SPEC §17

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| PERF-001 | Agent 무세션 유휴 | Release에서 10분, UI/영상/PTY 없음 | CPU 평균 ≤0.3%, private working set ≤70MiB 목표 | NOT_RUN |
| PERF-002 | 2세션 유휴 | PTY 2개, UI/영상 닫고 10분 | CPU 평균 ≤0.5%, Agent ≤110MiB 목표 | NOT_RUN |
| PERF-003 | 8세션 유휴 | PTY 8개, UI/영상 닫고 10분 | CPU 평균 ≤0.8%, Agent ≤220MiB 목표 | NOT_RUN |
| PERF-004 | UI 유휴 | 활성 터미널 1개와 UI, Release 상태 | 제품 합산 CPU ≤1.0%, 관련 WebView2 포함 ≤350MiB 목표 | NOT_RUN |
| PERF-005 | 입력 구간 측정 | 정상 입력 fixture 10,000회 | SPEC p95/p99 input 단계별 목표, network/child 지연 구분 | NOT_RUN |
| PERF-006 | 화면 왕복 | loopback echo fixture와 render trace | p95 ≤40ms/p99 ≤80ms 목표, parse와 paint 구분 | NOT_RUN |
| PERF-007 | 100KiB/s 일반 출력 | 총 100KiB/s + 활성 렌더 1개 | 제품 CPU 평균 ≤3%, 메모리 ≤400MiB 목표 | NOT_RUN |
| PERF-008 | 지속 출력/폭주 | 1MiB/s 10분 및 5MiB/s 5초 | 입력 스트레스 목표·bounded queue·정상 상태 복귀 | NOT_RUN |
| PERF-009 | 출력 중 Ctrl+C | 다른/같은 탭 출력 폭주 중 interrupt | PTY 전달 p95 ≤50ms 목표, 다른 세션 영향 없음 | NOT_RUN |
| PERF-010 | 미디어+프리뷰+CLI | 영상/HMR/2개 PTY 동시 동작 | 입력 p95가 정상 대비 10ms 넘게 악화되지 않는 목표 | NOT_RUN |
| PERF-011 | 영상 HW 예산 | 720p15 및 1080p30 HW 모드 각각 측정 | SPEC 호스트 CPU/RAM 목표와 GPU memory 기록 | NOT_RUN |
| PERF-012 | 24시간 soak | 실기기 24시간, 주기적 입력/연결 | 지속 메모리 증가·handle/timer/WS 누수 없음, SPEC 누수 gate 준수 | NOT_RUN |
| PERF-013 | 500회 생성종료 | PTY 생성·종료 500회, UI 재시작 50회 | 프로세스/handle/buffer 누수 없음 | NOT_RUN |
| PERF-014 | 네트워크 분리 측정 | 실제 tailnet에서 RTT·앱 처리·작업 부하 기록 | 동기화 안 된 wall clock 차감 없음, 제품/자식/Tailscale/합계 보고 | NOT_RUN |

## INSTALL — 설치·배포·업데이트

단계: **P1+** · 근거: SPEC §5, §19, §21–22

| ID | 검사 항목 | 절차 | 합격 기준 | 초기 상태 |
|---|---|---|---|---|
| INSTALL-001 | 표준 EXE 설치 | 새 Windows 테스트 사용자에게 NSIS 설치 | 일반 사용자 설치 경로와 첫 실행 정상 | NOT_RUN |
| INSTALL-002 | WebView2 없음 | WebView2 없는 허용된 테스트 VM에서 설치 | 명시한 의존성 설치/오프라인 안내, 무조건 skip하지 않음 | NOT_RUN |
| INSTALL-003 | 집 client-only | 집에서 client-only 설치 또는 브라우저 사용 | 회사 코드/Codex/개발환경 복사 불필요, 호스트 기능 자동 시작 없음 | NOT_RUN |
| INSTALL-004 | 자동 시작 동의 | 설치 후 로그인·재로그인 | 사용자가 허용한 경우에만 Agent 자동 시작 | NOT_RUN |
| INSTALL-005 | 작업 중 업데이트 | PTY가 살아 있는 동안 업데이트 시도 | 동의 없이 Agent/작업을 종료하지 않음 | NOT_RUN |
| INSTALL-006 | 삭제 범위 | 프로젝트와 다른 Serve 설정이 있는 상태에서 제거 | 제품 소유 항목만 삭제, 프로젝트/다른 서비스 보존 | NOT_RUN |
| INSTALL-007 | 서명·검사 보고 | 실제 installer 서명/해시/검사 결과 확인 | 미서명·미검증을 완료로 표기하지 않음 | NOT_RUN |
| INSTALL-008 | 라이선스 | 패키지 dependency와 재사용 소스를 조사 | LICENSE/NOTICE/SBOM과 사용 버전 기록 | NOT_RUN |
| INSTALL-009 | 로컬 테스트 경로 | CI 없이 PowerShell scripts 실행 | 문서화한 검사/빌드가 로컬에서 재현 가능 | NOT_RUN |

## 실제 결과 기록 템플릿

```yaml
requirement_test_id: MULTI-009
implementation_commit: null
run_at: null
environment:
  windows_build: null
  cpu_model: null
  logical_cpu_count: null
  ram_gib: null
  gpu_driver: null
  power_mode: null
  webview_or_browser: null
  tailscale_version: null
  codex_version: null
  build_mode: release
status: NOT_RUN
evidence_paths: []
measured_values: {}
failure_reason: null
notes: null
```

## 종합 판정

총 **161개** 제품 인수 항목(원본 139 + 추가 22). 현재 제품 통합 인수 결과: **0개 수행 / 161개 NOT_RUN**. 별도 단위 테스트 125개 PASS와 혼동하지 않는다.

P1~P6의 각 gate와 모바일 검사는 SPEC의 단계 순서를 따른다. 이 문서의 초기 NOT_RUN을 PASS로 일괄 치환하는 행위는 금지한다.


## UX — v1.1 마우스·다중 PTY·분할·모바일 추가 검사

| ID | 검사 항목 | 합격 기준 | 관련 소스 | 제품 인수 상태 |
|---|---|---|---|---|
| UX-001 | UI 탭 클릭 | 회사 OS 포인터를 움직이지 않고 표시 세션만 변경 | policy.ts / App.svelte | NOT_RUN |
| UX-002 | 일반 터미널 선택·스크롤 | mouse reporting이 꺼진 경우 로컬 조작 유지 | terminal.ts | NOT_RUN |
| UX-003 | TUI mouse click/drag/wheel | reporting이 켜진 선택 PTY에만 정상 protocol 전달 | terminal.ts / ws.rs | NOT_RUN |
| UX-004 | read-only TUI | 로컬 선택 허용, 원격 TUI 입력 차단 | TerminalPane.svelte | NOT_RUN |
| UX-005 | TUI 선택 우회 | Shift 등 로컬 텍스트 선택이 원격 클릭으로 전송되지 않음 | xterm integration | NOT_RUN |
| UX-006 | 동일 기기 두 브라우저 탭 | client_id가 같아도 connection_id별 단일 writer 보장 | lease.rs | NOT_RUN |
| UX-007 | 빠른 분할 focus 전환 | 다른 PTY에 단 한 바이트도 오배송하지 않음 | policy.ts / TerminalPane.svelte | NOT_RUN |
| UX-008 | 새 패널 첫 마우스 클릭 | UI 렌더 tick 대기 때문에 잘못된 세션으로 보내지 않음 | synchronous focus getter | NOT_RUN |
| UX-009 | 패널 숨기기 | 회사 프로세스 종료 API를 호출하지 않음 | PaneLayout.closeView | NOT_RUN |
| UX-010 | 명시적 terminal close | 선택 회사 PTY만 종료, 별도 확인 필요 | http.rs / session.rs | NOT_RUN |
| UX-011 | Ctrl+C 라우팅 | 선택 PTY에만 전달, 이미 대기한 일반 입력 처리 결과 정확히 보고 | writer_loop | NOT_RUN |
| UX-012 | 동일 프로젝트 다중 터미널 | Codex/dev-server/test shell에 같은 안정적 project_id | Store.project_for_cwd | NOT_RUN |
| UX-013 | 폰 접속 시 크기 | 열람만으로 회사 PTY cols/rows가 바뀌지 않음 | App / TerminalPane | NOT_RUN |
| UX-014 | 모바일 explicit resize | 제어권과 명시적 확인 뒤에만 변경 | resize API | NOT_RUN |
| UX-015 | 모바일 탭별 한글 초안 | composition Enter 오전송과 세션 간 초안 섞임 없음 | Drafts / IME handler | NOT_RUN |
| UX-016 | 모바일 background 복귀 | 회사 작업 유지, 새 snapshot, unknown 입력 자동 replay 금지 | Controller / TerminalView | NOT_RUN |
| UX-017 | 인증 refresh | 같은 연결의 writer 유지, ticket 재사용 거부 | refresh_auth | NOT_RUN |
| UX-018 | 두 Windows 창 제어 | 같은 desktop의 GUI 입력 제어권은 하나 | P5 global GUI lease 후속 | NOT_RUN |
| UX-019 | 로컬 물리 마우스 우선 | 원격 GUI 입력 회수, BlockInput 사용 안 함 | P5 local preemption 후속 | NOT_RUN |
| UX-020 | letterbox/DPI/음수 원점 | 검은 영역 click 거절, 물리 좌표 정확 | geometry.rs / gui_input.rs | NOT_RUN |
| UX-021 | stale source/geometry/frame | 오래된 창/PID/geometry 입력 거절 | FrameProof / GUI backend | NOT_RUN |
| UX-022 | GUI disconnect/인계 | 원격이 주입한 눌린 키/버튼만 해제 | InputBackend / P5 integration | NOT_RUN |


## V02 — 소스 0.2 추가 제품 회귀 게이트

아래 항목은 **실제 Windows/브라우저 통합 검사**다. 같은 정책의 단위 테스트가 PASS여도 이 행은 실행 증거 전 NOT_RUN이다.

| ID | 검사 항목 | 합격 기준 | 현재 상태 |
|---|---|---|---|
| V02-001 | 재접속 중 지연 응답 | 이전 host/list/socket가 새 epoch/lease를 덮어쓰지 않음 | NOT_RUN |
| V02-002 | 256KiB paste | UTF8 전체 검사 후 대상 PTY 한 곳만 순서대로 수신 | NOT_RUN |
| V02-003 | paste 중 Ctrl+C | 같은 PTY 우선 입력, 다른 PTY 입력 무지연, 중단 상태 명시 | NOT_RUN |
| V02-004 | headless sync/pending-wrap/palette | 실제 Rust model→xterm golden cell/mode 일치 | NOT_RUN |
| V02-005 | media feature off | CLI-only SDK 불필요, 캡처/encoder/helper 상주 없음 | NOT_RUN |
| V02-006 | native feature on | 실제 SDK/GPU에서 창 client-area/모니터 정상 영상 | NOT_RUN |
| V02-007 | approved tailnet ICE | 정확한 회사/집 IP와 제한 UDP만 사용, 공개 relay 없음 | NOT_RUN |
| V02-008 | 실제 frame 기반 제어 | stale/미표시/paused video에 클릭 불가 | NOT_RUN |
| V02-009 | local input vs pending Arm | 물리 입력이 queued Arm/Action보다 우선, 자동 재무장 없음 | NOT_RUN |
| V02-010 | 키 release 경합 | blur/revoke/disconnect/timeout/source-change에 고착 방지 | NOT_RUN |
| V02-011 | geometry/DPI | window client-area와 normalized 클릭 일치, 변경 시 중단 | NOT_RUN |
| V02-012 | helper 수명 | 100회 시작/종료 뒤 media process/hook/GPU resource 누수 없음 | NOT_RUN |
| V02-013 | full desktop | 일반 사용자 승인 모니터만 보기·제어, 잠금/UAC 거절 | NOT_RUN |
| V02-014 | 두 PC와 모바일 | PC video+terminal, 모바일 CLI만, PTY 크기 자동 변경 없음 | NOT_RUN |
| V02-015 | 실제 Agent fixture | 임시 기기/두 CMD 생성·왕복·분리·재접속·자체 정리 | NOT_RUN |
| V02-016 | 성능 회귀 | 원본 SPEC CPU/RAM/p95/soak 기준 유지, 영상 비용 포함 | NOT_RUN |

단위 테스트 증거: `docs/test-results/v0.2.0/all-tests.tap.txt` (96 core + 29 client). Fake transport/decoder를 실제 native/WebRTC로 표시하지 않는다.
