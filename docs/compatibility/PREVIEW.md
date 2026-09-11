# Web Preview compatibility profile

현재 코드는 HTTPS Tailscale Serve의 **다른 포트 origin**을 사용하는 숫자형 IPv4 loopback HTTP/WS 프록시다. 대상 port 등록에 관리자 승인과 process-creation identity가 필요하다. 같은 프로젝트 폴더의 ID는 SQLite에 유지한다.

| 영역 | 구현 / 제한 |
|---|---|
| 바인딩 | 관리 127.0.0.1:3847, 프리뷰 gateway 127.0.0.1:13844~13851 |
| 외부 HTTPS | 관리 기본 443, 프리뷰 8444~8451; 기존 Serve 설정을 자동 변경하지 않음 |
| 허용 대상 | 등록한 IPv4 loopback 리스너. wildcard·관리 포트·gateway 재귀·임의 URL 거절 |
| 인증 | fragment 일회용 launch → POST consume → 10분 HttpOnly Secure cookie |
| 프로젝트 경계 | 각 origin을 영구 project_id에 바인딩. 다른 프로젝트로 조용히 재사용 금지 |
| 전달 | streaming HTTP, upstream WebSocket, SSE 스트림. redirect를 서버에서 따라가지 않음 |
| 쿠키 | gateway 쿠키 미전달, upstream 쿠키 이름 namespace, Domain 제거 |
| 현재 제한 | 임의 앱 Authorization 헤더 미전달. document.cookie 이름 매핑, hardcoded absolute localhost URLs, OAuth callback, cross-origin API, HMR 실제 호환성은 추가 작업/검증 필요 |
| service worker | 초기 profile에서 worker-src none. 개발 앱이 SW를 요구하면 지원하지 않음 |
| 보안 모델 | 같은 hostname/다른 port는 쿠키·same-site까지 독립된 sandbox가 아님. 신뢰 가능한 본인 개발 앱만 사용 |

**아직 브라우저/Windows/실제 Tailscale의 end-to-end 검증을 하지 않았다.** “모든 웹앱이 설정 없이 열림”을 주장하지 않는다. 프로덕션 배포 전 per-project hostname 또는 독립된 사이트 격리 모델을 검토한다.
