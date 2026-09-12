# Automatic Tailscale bootstrap validation — 2026-09-13

This evidence was produced in the isolated `work/tailscale-onboarding-20260912`
worktree. The frozen 24-hour soak checkout was not changed.

The implementation in commit `50151a0` discovers the current matching stable
Windows MSI from Tailscale's official stable index, requires its adjacent SHA-256
file to contain exactly one hash, verifies that hash and the Authenticode signer,
and starts a new installation with `TS_INSTALLUPDATES="always"`. Existing
Tailscale installations return without changing their update policy.

The native setup panel automatically starts this bootstrap once when it observes
`not_installed`; it offers a retry only after a terminal failure. Browser mode
does not invoke native IPC. Login and host Serve consent remain separate actions.

No real Tailscale installation, update, login, Serve change, Agent restart, or
existing account/policy change was performed on this PC. A native missing-client
smoke is deliberately `NOT_RUN`: it would start the real automatic download and
could present an installer/UAC prompt while the independent soak is active.
