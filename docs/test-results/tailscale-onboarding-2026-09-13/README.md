# Automatic Tailscale bootstrap validation — 2026-09-13

This evidence was produced in the isolated `work/tailscale-onboarding-20260912`
worktree. The frozen 24-hour soak checkout was not changed.

The implementation in commits `50151a0`, `49515dd`, and `6b9bf62` discovers the current matching stable
Windows MSI from Tailscale's official stable index, requires its adjacent SHA-256
file to contain exactly one hash, verifies that hash and the Authenticode signer,
and starts a new installation with `TS_INSTALLUPDATES="always"`. Existing
Tailscale installations return without changing their update policy.

The native setup panel opens itself on mount and automatically starts this
bootstrap once when it observes `not_installed`; it offers a retry only after a terminal failure. Browser mode
does not invoke native IPC. Login and host Serve consent remain separate actions.

No Tailscale installation, update, login, Serve change, Agent restart, or
existing account/policy change was performed by this validation. A fresh-profile
native connected-client smoke passed after host-role selection: the panel opened
and reported the already connected Tailscale client without install, login, or
retry controls. A native missing-client smoke remains `NOT_RUN` because it would
start a real download and could present an installer/UAC prompt.
