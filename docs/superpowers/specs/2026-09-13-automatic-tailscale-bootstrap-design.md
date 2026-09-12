# Automatic Tailscale bootstrap design

## Goal

RemoteCodex should prepare Tailscale without exposing a separate install button.
On a Windows PC without Tailscale, the first RemoteCodex setup automatically
obtains and verifies the current official stable installer, then moves the user
to the Windows approval and Tailscale login steps.

## User flow

1. RemoteCodex checks whether Tailscale is already installed.
2. If it is absent, the setup panel shows progress such as “Preparing secure
   connection” while it downloads the architecture-matched official MSI.
3. RemoteCodex verifies the official checksum and the Windows Authenticode
   publisher before it invokes the MSI.
4. Windows elevation and Tailscale account login remain explicit user actions.
5. Once connected, existing host consent and Serve ownership checks apply.
6. If download or verification fails, the panel reports a retryable failure. It
   never accepts a user-provided installer URL or local executable.

The browser/mobile flow remains documentation-only and never gains privileged
installation IPC.

## Update policy

For a Tailscale installation created by RemoteCodex, the MSI invocation sets
`TS_INSTALLUPDATES="always"`. This delegates future stable-client updates to
Tailscale’s own Windows updater instead of making RemoteCodex implement a
second updater. The current official MSI documentation defines this property,
and Tailscale recommends automatic client updates.

An existing Tailscale installation is detected and preserved: no reinstallation,
downgrade, or update-policy change occurs. This avoids overwriting a user's or
organization's device-management policy.

## Installer resolution and trust boundary

The bootstrapper resolves a release only from Tailscale's official stable
package endpoint for the detected CPU architecture. It obtains the release
checksum from the corresponding official checksum resource, validates the
download SHA-256, and requires a valid Authenticode signature issued to
`Tailscale Inc.` before execution. Release metadata and URLs are constrained to
the official host and fixed filename grammar; they are not accepted through
Tauri IPC or UI input.

If official metadata cannot be fetched or validated, no installer is launched.
The cached MSI is scoped to the current operation and removed after a completed
installer process. A timed-out installer retains its exact process identity
guard, as in the existing implementation.

## Components

* The NSIS/RemoteCodex installer keeps packaging only RemoteCodex resources.
  Tailscale is fetched on first setup so a newly downloaded RemoteCodex build
  does not embed a stale third-party MSI.
* `tailscale_setup.rs` owns release resolution, state transitions, command
  allow-listing, timeout ownership, and safe IPC results.
* The PowerShell helper performs the download, verification, and interactive
  MSI launch. Its inputs are generated locally by the Rust command only.
* `TailscaleSetup.svelte` replaces the install button with non-privileged
  progress/status UI. It still allows retry after a terminal bootstrap error.

## Error handling

The UI distinguishes offline/download failure, verification failure, Windows
approval rejection, installer cancellation, reboot required, installer timeout,
and login/connection states. It must clear stale ready/Serve state after a
failure. Automatic download is started only from the trusted native main window
and only for a missing Tailscale installation; browser, preview, client-only,
and remote origins cannot trigger it.

## Verification

* Unit-test release URL/architecture/checksum parsing, official-host rejection,
  existing-install preservation, and MSI argument construction including
  `TS_INSTALLUPDATES="always"`.
* Extend the installer mock suite with metadata fetch, checksum mismatch,
  publisher mismatch, and no-install-on-invalid metadata cases.
* Update browser tests for automatic progress, retryable terminal errors,
  browser no-IPC behavior, and host/client Serve separation.
* Run Rust tests, installer mocks, Svelte check, web build, Tauri release build,
  and a read-only native UI smoke. Do not install, update, log in to Tailscale,
  alter Serve, restart the Agent, or use the ongoing soak's state.
* A real first-install/UAC/login/update test remains a separate Windows
  acceptance gate and is recorded as NOT_RUN unless performed in an authorized
  disposable environment.

## Scope exclusions

This change does not bundle a stale Tailscale MSI, bypass Windows elevation,
automate user account authentication, alter a pre-existing Tailscale policy,
or change the running 24-hour soak Agent, PTYs, configuration, or network.
