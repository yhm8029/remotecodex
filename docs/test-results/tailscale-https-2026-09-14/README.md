# Tailscale HTTPS prerequisite and retry recovery - 2026-09-14

## Observed failure

Installed build f40af80 detected Tailscale 1.102.4 as Running/online. The user's
Serve attempt failed. A receiptless enable journal remained, no public_origin
was configured, and `tailscale serve status --json` returned `{}`. Self.CapMap
and legacy Capabilities lacked the exact `https` capability; CertDomains was null.
Account, node identifiers and raw status output are intentionally omitted.

The upstream 1.102.4 Serve implementation checks the HTTPS capability, presents
an approval URL and may wait for user approval even with `--yes`. RemoteCodex's
five-second runner discarded that output on timeout. The persistent journal
also had no recovery path for a verified unapplied enable operation.
The original UI's collapsed raw error was not independently captured; the
HTTPS prerequisite is directly confirmed, while the original timeout is inferred
from the implementation and the remaining journal.

## Change

- Probe Running state, device DNS and exact HTTPS capability before new Serve
  mutations. Malformed or missing status fails closed.
- Show HTTPS approval instructions and open only the fixed Tailscale admin DNS
  page. The user enables HTTPS and returns to refresh; opening the page does not
  count as approval. Remote connection consent remains separate.
- Recover only a receiptless enable journal with no existing receipt/origin,
  matching expected DNS/backend, and a successful exclusively absent Serve
  inspection under the settings transaction guard. Remove only that journal.
- Retain journals for uncertain or nonzero CLI outcomes; status reconciliation
  must prove a safe recovery. Drain reader threads after killing/reaping a CLI.
- Preserve connected setup status when only the Serve query fails.

## Validation and limits

Desktop Rust suite: 62 tests passed, including new HTTPS and recovery predicate
regressions. The new tests failed first because the production helpers were absent.
Sol reviewed transaction ordering and exclusive-receipt boundaries; the pending
journal gate was moved ahead of the HTTPS preflight as requested.
UI/installer verification is recorded in the accompanying result files.

The attempted local Serve reproduction was rejected by automatic policy review
with no detailed reason. No retry through another mechanism was used. We did not
enable account HTTPS, alter Serve, reset foreign rules, install this fixed build,
or restart the active Agent. Real approval and an authenticated remote connection
remain user/environment verification steps. The local pending journal is preserved
until the corrected app can reconcile it.

Coding attribution: M3 supplied the two production helpers. Its two test drafts
and two UI test drafts were rejected; Luna supplied regression tests and the UI
fix. Parent integrated native commands and lifecycle changes and corrected one
Rust test type-inference issue. M3 adaptive inference exhausted one completion
budget without code; disabled mode then produced the accepted HTTPS helper.
See m3-calls.json for all eight directly observed calls and 8,925 tokens; GPT
usage is not measured by that ledger.

References:
- https://tailscale.com/docs/features/tailscale-serve
- https://tailscale.com/docs/how-to/set-up-https-certificates
- https://github.com/tailscale/tailscale/blob/v1.102.4/cmd/tailscale/cli/serve_legacy.go
- https://github.com/tailscale/tailscale/blob/v1.102.4/tailcfg/tailcfg.go

## Follow-up: explicitly requested live application

After the initial packaging verification, the user requested applying the update
also to the running desktop and Agent. With zero active terminal sessions,
we closed the desktop, requested graceful Agent shutdown, preserved a local
backup, and installed the hash-verified payload. The Agent web_dir now points to
the installed web resources instead of an older development snapshot.

During native verification the HTTPS capability changed to enabled and a matching
Serve rule/receipt/public_origin appeared through the interactive app workflow.
The desktop was closed during this interval; a stable approval-state screenshot
was not obtained, so no screenshot claim is made. The Agent was restarted again
with the new origin, and the desktop was launched normally without debugging.
The earlier no-application statement above describes the pre-application stage.

Final local checks: HTTPS root HTTP 200, HTML exactly matches installed web build,
zero active sessions, public_origin configured, no pending journal, receipt exists,
and temporary CDP port closed. This verifies this PC's HTTPS route and running
payload; an authenticated second-device session is still separate verification.

Installer: RemoteCodex-0.2.0-afb1efc-setup.exe, 16,618,017 bytes.
SHA256: 85f1e721d9f1ec0234dcbe4841d70f10393d27e9d329a106299220576c0f19c0.
Payload source commit: afb1efc09000fddc9b51120a26305a275542866f.
Installed desktop SHA256: d775ed09ee5037b892162f5cc990e617bf803bd032d61999994696d791cde865.
Agent binary remains unchanged from f40af80 and was restarted with updated config.
