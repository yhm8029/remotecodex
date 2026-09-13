# Simple Host Connection Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development with M3 coding units and GPT reviews.

**Goal:** QR/mobile and invitation-paste/PC pairing plus saved host aliases without separately typing address and ticket.
**Architecture:** Typed frontend invitation codec and metadata-only host storage. HostInvite renders a locally generated QR from the existing authenticated one-use ticket API. InvitationConnect emits imported/selected metadata to App, which retains existing explicit pairing/authentication.
**Tech Stack:** TypeScript, Svelte 5 legacy syntax, qrcode, existing Rust Agent and Tauri IPC.

## 1. Codec and host metadata
Files: apps/web/src/invitations.ts, apps/web/src/saved-hosts.ts, tests/web/invitations.test.mjs, tests/web/saved-hosts.test.mjs.
- [x] M3 writes regression tests; node --experimental-strip-types --test tests/web/*.test.mjs fails before modules exist.
- [x] Implement exports Invitation={v:1,origin:string,label:string,ticket:string}; createInvitation({origin,label,ticket}):string; parseInvitation(link:string,currentOrigin?:string):Invitation; normalizeHostOrigin(value:string):string.
- [x] Enforce HTTPS DNS labels ending .ts.net, 443, root, no credentials/query; strict version/keys, URL<=2048, bounded UTF8 payload, label1..80, URLsafe ticket32..128, matching envelope origin and optional browserorigin. No network/I/O in codec.
- [x] loadHosts(storage:Pick<Storage,'getItem'>):SavedHost[]; saveHost(storage:Pick<Storage,'getItem'|'setItem'>,host:{origin,label}):SavedHost[]; removeHost(...,origin):SavedHost[]. Versioned key, cap32, dedupeorigin, canonical allowlist, persist ONLY origin/label. Storagefailure becomes user message without breaking authentication.
- [x] Verify roundtrip Unicode, malformed/oversized/foreign invites, secret exclusion, damaged storage and duplicates.

## 2. HostInvite component
Files: apps/web/src/HostInvite.svelte, apps/web/package.json, package-lock.json.
- [x] M3 receives exact AgentApi.request<T>, native invoke status, qrcode.toDataURL APIs and component props api, scopes.
- [x] Show host-name input; explicit create checks fresh tailscale_status (owned/httpsready/no restart), local_admin status public_origin exactmatches; create existing /pair-tickets with fixed non-GUI permission list intersecting owner scopes; expiry300seconds.
- [x] QR/clipboard locally, countdown, hide atdeadline andonunmount, no stored invite or network QR service, no quiet renewal or autoapproval.
- [x] Browser harness verifies unavailable state causes no ticketrequest, payload reflects running host, copy/expiry/error cleanup.

## 3. InvitationConnect and App integration
Files: apps/web/src/InvitationConnect.svelte, apps/web/src/App.svelte, tests/e2e/invitations.mjs.
- [x] M3 component props oninvite(invitation), onselect(savedhost), disabled, currentOrigin optional, native. Textarea paste uses explicit button; optional clipboard-read button onlygesture; no automatic API. Load saved origins/labels, select/remove buttons.
- [x] App scrubs rc-invite fragment synchronously before await, validates currentorigin inbrowser, fills base/ticket/hostlabel, choosesclient mode. Keeps manualfields and experimentalconsent; shows target and fixed remote-terminal permission summary. Explicit pairbutton only.
- [x] Save alias AFTER successful pairing/connect; clear invitationticket onterminalfailed pairing/mode/host change; manual host edits cannot silently carry tickettoanotherorigin.
- [x] Render HostInvite only connected nativehost admin devices; no nativehostadmin calls on mobile/clientimport.
- [x] Browser tests: desktoppaste and App QR URL consumption/scrubbing, explicit registration, mixed origins, invalid invitation clearing and host transitions; server one-use/expiry enforcement remains the existing Agent implementation.

## 4. Review, package, apply
- [x] Astra reviews authentication boundaries and Sol/Terra reviews integration as needed; fix concrete findings in small M3 units, fallback only after repeated specific failure.
- [x] npm run check:web, focused Node tests, meaningful browser E2E, cargo tests ifnativebehavior changes. Record direct M3 results/tokens separately from GPT usage.
- [ ] Commit/push source, build Windows NSIS with scripts/package-windows.ps1. Verify archive/inventory/license/SBOM hashes and copy installer to Downloads.
- [ ] User already authorized live replacement/restart. Check processidentities and active sessions, preservebackup, gracefullystop, replaceverifiedpayload, restart. Verify currentnativeUI and HTTPSweb route, no realinvitations/tokens inreports.
- [ ] Record actualPC/mobile vsautomatedvalidation limits and updateprojectmemory.
