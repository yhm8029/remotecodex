# Five-hour one-use invitations and compact links

User amendment: the invitation must remain usable during the commute, expire after five hours without registration, and become unusable after one registration. The host UI removes used/expired QR and link on a three-second authenticated status poll or its own deadline. Registered devices retain their existing authentication; this is not a five-hour session timeout.

HTTP issuance accepts optional expires_in (1..18000); omitted requests and existing local/GUI commands retain 300 seconds. New HostInvite requests 18000. The server enforces expiry in the existing pair -> anonymous_limit -> prune chain under the same mutex that consumes tickets. The new status route requires admin.devices and accepts the secret only in the POST body.

Compact v2 removes duplicate origin and JSON field names while preserving full ticket entropy. Legacy v1 parsing remains. Both forms bind to canonical HTTPS origin and browser current origin. No third-party shortener is contacted. Share through Kakao self-chat; both devices still need Tailscale. Unused invitations are in-memory and are invalidated by an Agent restart.

Validation: 9 Auth tests passed, including direct expired redemption and concurrent one-use enforcement; 11 Node invitation/storage tests passed; Svelte check 0 errors/0 warnings; 6 HostInvite component E2E groups passed with virtual time, status/expiry/clipboard, stale-response isolation, auth retry and disposal; 4 actual App E2E groups passed using compact v2 on a 390px viewport. Astra reviewed auth/HTTP/codec and final polling/lifetime changes with no blocking findings.

The reviewer initially overlooked the existing expiry prune call chain and corrected that finding. M3 delegation problems are recorded candidly in m3-calls.json: two compact requests were incorrectly framed as guidance and repeated without narrowing, and one UI draft invented source context. These are delegation failures, not evidence of model coding inability. The parent subsequently gave M3 an actual single polling function and integrated the returned implementation with one missing lifetime guard. AGENTS.md now forbids those vague/repeated delegation patterns.

Package and installed-runtime results will be appended after verification. No physical second-PC or phone authenticated connection is claimed by these automated checks.
