# Five-hour one-use invitations and compact links

User amendment: the invitation must remain usable during the commute, expire after five hours without registration, and become unusable after one registration. The host UI removes used/expired QR and link on a three-second authenticated status poll or its own deadline. Registered devices retain their existing authentication; this is not a five-hour session timeout.

HTTP issuance accepts optional expires_in (1..18000); omitted requests and existing local/GUI commands retain 300 seconds. New HostInvite requests 18000. The server enforces expiry in the existing pair -> anonymous_limit -> prune chain under the same mutex that consumes tickets. The new status route requires admin.devices and accepts the secret only in the POST body.

Compact v2 removes duplicate origin and JSON field names while preserving full ticket entropy. Legacy v1 parsing remains. Both forms bind to canonical HTTPS origin and browser current origin. No third-party shortener is contacted. Share through Kakao self-chat; both devices still need Tailscale. Unused invitations are in-memory and are invalidated by an Agent restart.

Validation: 9 Auth tests passed, including direct expired redemption and concurrent one-use enforcement; 11 Node invitation/storage tests passed; Svelte check 0 errors/0 warnings; 6 HostInvite component E2E groups passed with virtual time, status/expiry/clipboard, stale-response isolation, auth retry and disposal; 4 actual App E2E groups passed using compact v2 on a 390px viewport. Astra reviewed auth/HTTP/codec and final polling/lifetime changes with no blocking findings.

The reviewer initially overlooked the existing expiry prune call chain and corrected that finding. M3 delegation problems are recorded candidly in m3-calls.json: two compact requests were incorrectly framed as guidance and repeated without narrowing, and one UI draft invented source context. These are delegation failures, not evidence of model coding inability. The parent subsequently gave M3 an actual single polling function and integrated the returned implementation with one missing lifetime guard. AGENTS.md now forbids those vague/repeated delegation patterns.

## Package and applied runtime

Payload source commit: 8748c4b60b86af235f9c2cac328da37e56e59df1. Standard Windows NSIS build exited 0. Archive integrity passed; 61 inventory files and 1,064 license/SBOM files matched, and 637 SBOM components validated without schema errors. Native binary differs from the build output only by the expected Tauri NSIS marker.

Installer: RemoteCodex-0.2.0-8748c4b-setup.exe, 16,659,451 bytes. SHA256: ba7a2812c2438b8cf34b7fc155fae3a9d9ee571e68a10e28d93e7a3b01df861a. A verified copy and checksum sidecar are in this PC Downloads. The verified extracted payload was applied directly with existing application files backed up; the installer itself was not executed.

Agent and desktop were restarted with configuration preserved. The installed native UI actually requested an invitation and the real Agent returned expires_in18000; compact link length111 and pending status were verified without recording its secret. The final desktop is running normally, the temporary debug listener is closed, and the HTTPS root returns the installed HTML. There were zero active PTYs at replacement.

No authenticated connection from a second physical PC or phone is claimed. Other PC apps must update to understand newly generated compact links. Existing registered device keys and sessions are separate from invitation expiry.
