# Guided Tailscale setup — local validation

Status: implementation and local verification complete; external installation
and connectivity gates remain open. Based on `5116879`.

Implementation commits: `6698c33` (desktop backend and installer mocks),
`f960c71` (UI and browser behavior tests). The release smoke used this source
content before commits; the only subsequent source edit removed a trailing
blank line from the Svelte file. No executable logic changed afterward.

This branch is built in a separate worktree. It does not update the Agent,
frontend assets, Tailscale, or configuration used by the continuing 24-hour soak
in the original checkout. That run uses the frozen Agent SHA-256
`325463e81bd11e06dbe5665bfe5c87f88ed73c35892102523720056ece04bedd`
and a temporary resource sampler with only its WMI timeout increased from 5 to
15 seconds. Concurrent development builds are outside the measured product
process group; the soak is not an otherwise idle-machine benchmark.

## Installer provenance

The fixed Windows MSI release is 1.102.4. The official package page lists amd64,
arm64, and x86 installers. The selected URL and version are returned in the
installation result. Existing installations are not automatically upgraded.

- [Official packages and SHA-256 convention](https://pkgs.tailscale.com/stable/)
- [Official Windows MSI instructions](https://tailscale.com/docs/install/windows/msi)
- [Official Windows login flow](https://tailscale.com/docs/install/windows)

`installer-verification.json` records an actual download of the amd64 MSI:
its SHA-256 matched the official `.sha256` file, and Windows Authenticode
reported `Valid`, with signer `Tailscale Inc.`. The installer was not executed.
The other architecture hashes were retrieved from the official release URLs;
their MSI binaries have not been run or independently signature-checked here.

## Verification boundaries

Baseline before implementation: Svelte check passed with zero errors/warnings,
web production build passed, and all 32 existing desktop Rust tests passed.
The pure setup classifier/discovery tests were observed failing against an
unimplemented classifier, then all 15 passed with the implementation. One M3
test's candidate ordering was corrected during GPT integration; no assertion
was relaxed. Cargo's copied-source timestamp initially reused the earlier
test executable; rewriting the source forced recompilation and the passing run.

Mocked installer and native-IPC browser checks do not prove a real Windows
installation, UAC interaction, login, device approval, or two-PC connectivity.
Those require a separately authorized disposable Windows environment and
interactive account login. This development session preserves the ongoing
soak's network configuration and therefore does not install Tailscale here.

## Model attribution

GPT designed and reviewed the change. M3 generated the pure detection module,
installer script, and initial OS/UI/test drafts. Repeated incorrect corrections
in OS/UI/test drafts triggered the user-approved Luna fallback. Luna owns the
initial fallback integration and behavioral tests. GPT root completed the UI
rewrite, corrected fixture assertions, added deadline/host-readiness coverage,
and fixed the reviewed pre-Agent integration issue. The feature is not represented as
M3-only implementation. `m3-calls.json` records returned usage and outcomes;
timeouts without usage are recorded as unknown, not zero. GPT usage is not
available from these calls.

The parent initially described the UI as Svelte 4, although this checkout uses
Svelte 5. That framework context error and the fixture mount correction belong
to orchestration, not to M3 capability failures. Separately, rejected UI drafts
contained a mutating Serve call during refresh and invalid asynchronous state
handling; rejected test drafts contained invalid syntax and mock scopes.

## Final local results

- Desktop Rust tests: 58 PASS, including origin restrictions and installer
  concurrency/process-identity handling; cargo fmt check PASS.
- Installer production-script mock suite: 15 PASS. No MSI was executed.
- Browser component tests: nine behavioral groups PASS (`ui.json`), including
  actual 120-second polling deadline with a controlled clock, cleanup,
  post-login host address inspection, installer outcomes, and stale readiness.
- `npm run check:web`: zero errors/warnings. `npm run build:web`: PASS
  (existing mixed static/dynamic Tauri import bundling advisory).
- Tauri CLI `build --no-bundle --ci`: PASS. Existing platform/dead-code and
  redundant unsafe warnings remain. Release executable SHA-256:
  `53786f04cd23d0c7118b063fceda3207c5dd77d6416bae082c630f80f1b78b21`.
- Actual WebView2 production UI: PASS for client and host missing-installation
  detection, using a fresh isolated WebView2 profile. See `native-ui.json` and
  `native-client.png` / `native-host.png`. No Agent start/pair/install/login/Serve
  action was invoked. The test-owned UI was closed afterward.
- Sol reviewed backend safety and final UI. Its pre-Agent Serve-readiness issue
  was corrected by deferring configuration to connected host settings.

Standard installation directories are rechecked on every request, so a newly
installed standard-path CLI is detected despite this process's stale PATH.
Arbitrary nonstandard changes to the Windows registry PATH are not dynamically
imported by the Rust detector. Installer timeout recovery follows the exact
PID and creation time; if that identity cannot be verified, it stays blocked
until the user finishes/closes the installer and reopens RemoteCodex.

The Windows MSI/UAC/account-login and already-installed account/Serve
preservation scenarios remain NOT_RUN on real installations. Mocks and this
read-only native smoke are not substitutes for those release checks.
