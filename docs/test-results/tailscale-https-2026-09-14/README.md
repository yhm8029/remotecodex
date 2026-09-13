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
