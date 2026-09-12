# Automatic Tailscale bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically obtain, verify, and launch the current official stable Tailscale MSI for a missing Windows installation, then enable Tailscale-managed updates for that newly created installation.

**Architecture:** The existing native-only `tailscale_setup_install` command remains the sole privileged entry point. Its PowerShell helper discovers the newest architecture-specific MSI from the official stable package index, constrains the filename and host, validates the matching official SHA-256 plus Authenticode publisher, and invokes the MSI with the official auto-update policy. The Svelte setup panel calls that command once automatically after it observes `not_installed`; it presents progress and retry instead of an install button.

**Tech Stack:** Rust/Tauri, PowerShell 5.1, Svelte 5, Playwright component harness, Windows MSI and Authenticode.

---

## File structure

| File | Responsibility |
| --- | --- |
| `apps/desktop/src-tauri/src/tailscale-install.ps1` | Fetch the official stable index, select an exact MSI URL, verify it, launch MSI with the update policy, and emit one constrained JSON result. |
| `tests/windows/tailscale-installer.test.ps1` | Mock every effectful PowerShell command and prove release selection, trust checks, policy argument, and failure paths. |
| `apps/desktop/src-tauri/src/tailscale_setup.rs` | Preserve native command guard, installer result allow-list, timeout process ownership, and stable public state contract. |
| `apps/web/src/TailscaleSetup.svelte` | Automatically invoke the existing native bootstrap exactly once per open panel when missing; present progress and explicit retry after terminal error. |
| `tests/e2e/tailscale-setup.mjs` | Exercise automatic bootstrap, retry, no browser IPC, and existing host/client/Serve boundaries. |
| `docs/test-results/tailscale-onboarding-2026-09-13/` | Record the exact test commands, PASS/FAIL status, executable hash, and real-install limits. |

### Task 1: Prove official release selection and auto-update policy in the installer helper

**Files:**
- Modify: `tests/windows/tailscale-installer.test.ps1`
- Modify: `apps/desktop/src-tauri/src/tailscale-install.ps1`

- [ ] **Step 1: Add failing mock cases for the package index and MSI command line**

Extend the mock HTTP response to distinguish the stable package index, the selected MSI, and its `.sha256` file. The fixture index must include old and current versions for every architecture plus adversarial links. Add these assertions before changing the production helper:

```powershell
$index = @'
<a href="tailscale-setup-1.100.0-amd64.msi">old</a>
<a href="tailscale-setup-1.102.3-amd64.msi">new</a>
<a href="tailscale-setup-9.999.0-amd64.msi.exe">bad suffix</a>
<a href="https://evil.example/tailscale-setup-9.999.0-amd64.msi">bad host</a>
'@

Assert-Equal $result.code 'installed'
Assert-Equal $result.version '1.102.3'
Assert-Equal $result.source 'https://pkgs.tailscale.com/stable/tailscale-setup-1.102.3-amd64.msi'
Assert-True ($script:StartProcessCalls[0].ArgumentList -contains 'TS_INSTALLUPDATES="always"')
```

Add three failing cases with no MSI launch: invalid index content, a checksum response that is not exactly a 64-hex digest for the selected filename, and an index containing only a foreign absolute URL. Preserve existing cases for existing installation, hash/signature/publisher failure, UAC denial, cancellation, timeout, and cleanup.

- [ ] **Step 2: Run the installer mock suite and confirm the new assertions fail**

Run:

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File tests/windows/tailscale-installer.test.ps1
```

Expected: FAIL because the helper still uses the pinned `1.102.4` source and does not pass `TS_INSTALLUPDATES="always"`.

- [ ] **Step 3: Replace pinned release metadata with constrained stable-index resolution**

In `tailscale-install.ps1`, remove `$version` and `$hashes`. Add only these trusted constants:

```powershell
$packageRoot = 'https://pkgs.tailscale.com/stable/'
$publisher = 'Tailscale Inc.'
$filenamePattern = '^tailscale-setup-(\d+)\.(\d+)\.(\d+)-(amd64|arm64|x86)\.msi$'
```

Add a pure local selector which accepts anchor href values, rejects every value that is not a bare filename matching the pattern and selected architecture, converts the three captures to `[Version]`, and returns the maximum version filename. It must not accept query strings, directory separators, absolute URLs, prerelease names, or a different architecture.

Fetch `$packageRoot` with the existing TLS, timeout, and no-redirect settings. Feed only `<a>` href values into the selector. Construct the MSI URI and checksum URI with:

```powershell
$filename = Select-StableMsiFilename -Href $hrefs -Architecture $arch
if (-not $filename) { $code = 'download_failed'; return }
$source = $packageRoot + $filename
$checksumUrl = $source + '.sha256'
```

Fetch the checksum separately. Require its trimmed value to match exactly `^[a-fA-F0-9]{64}$`; do not parse a filename column or accept whitespace-separated alternatives. Continue to hash the downloaded file and require equality, then require `Get-AuthenticodeSignature` status `Valid` and the exact simple publisher name. Keep the second `Test-Installed` check immediately before launch.

Launch only the absolute System32 `msiexec.exe` with these arguments:

```powershell
@('/i', "`"$msiPath`"", 'TS_INSTALLUPDATES="always"', '/norestart')
```

Do not add any UI/IPC-supplied URL, version, command argument, silent-install option, registry write, or update action for an already installed client.

- [ ] **Step 4: Run the installer mock suite and verify it passes**

Run the command from Step 2.

Expected: PASS with the prior cases plus the new release-selection and auto-update-policy cases. Each run must emit exactly one JSON object and perform no real download, MSI launch, Tailscale login, or VPN change.

- [ ] **Step 5: Commit the installer behavior**

```powershell
git add apps/desktop/src-tauri/src/tailscale-install.ps1 tests/windows/tailscale-installer.test.ps1
git commit -m "feat: bootstrap latest verified Tailscale MSI"
```

### Task 2: Preserve the native command boundary and result contract

**Files:**
- Modify: `apps/desktop/src-tauri/src/tailscale_setup.rs`
- Test: `apps/desktop/src-tauri/src/tailscale_setup.rs`

- [ ] **Step 1: Add failing contract tests for automatic bootstrap results**

Add tests that assert `allowed_code` accepts the existing terminal results used by automatic bootstrap (`download_failed`, `verification_failed`, `approval_denied`, `cancelled`, `timeout`) and rejects arbitrary input. Add a test for a result with a dynamic official source and semantic version:

```rust
#[test]
fn bootstrap_result_keeps_only_expected_metadata() {
    let result: InstallResult = serde_json::from_str(
        r#"{"code":"installed","version":"1.102.3","source":"https://pkgs.tailscale.com/stable/tailscale-setup-1.102.3-amd64.msi"}"#,
    ).unwrap();
    assert_eq!(result.code, "installed");
    assert_eq!(result.version, "1.102.3");
    assert!(result.installer_pid.is_none());
}
```

- [ ] **Step 2: Run the focused Rust tests and confirm a failing assertion before implementation if required**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml tailscale_setup -- --nocapture
```

Expected: the new dynamic-result test either fails against the old fixed assumptions or documents that no Rust behavior change is required. Do not change an assertion merely to make it pass.

- [ ] **Step 3: Make the smallest Rust contract change needed**

Keep `install()` as the only code path that starts PowerShell. It must retain all of these existing properties:

```rust
let mut guard = InstallGuard::acquire(&INSTALLING)?;
if resolve_cli().is_some() { return Ok(existing_install_result()); }
// PowerShell path is SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe
// stdout is parsed as InstallResult; stderr is not exposed through IPC.
// timeout retains guard only after exact PID/creation-time validation.
```

If a helper is added for the repeated existing-install result, make it private and return `source: "existing"`; do not expose installer PID/ticks through serialization. Do not relax `allowed_setup_origin`, main-window checks, or timeout fail-closed behavior.

- [ ] **Step 4: Run Rust formatting and all desktop tests**

Run:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --quiet
```

Expected: formatting succeeds and all desktop tests pass.

- [ ] **Step 5: Commit the native contract tests**

```powershell
git add apps/desktop/src-tauri/src/tailscale_setup.rs
git commit -m "test: cover automatic Tailscale bootstrap contract"
```

### Task 3: Replace the manual install button with one-shot automatic bootstrap

**Files:**
- Modify: `tests/e2e/tailscale-setup.mjs`
- Modify: `apps/web/src/TailscaleSetup.svelte`

- [ ] **Step 1: Add failing browser behavior tests**

Replace the manual-install expectation with these tests:

```javascript
await test('native missing Tailscale bootstraps once per opened panel', async () => {
  const p = await page('role=client');
  await seen(p, 'tailscale_setup_status');
  await seen(p, 'tailscale_setup_install');
  assert.equal(await p.getByTestId('install').count(), 0);
  assert.match(await p.getByTestId('state').textContent(), /원격 연결 준비 중|설치 중/);
  await p.getByTestId('refresh').click();
  assert.equal(await p.evaluate(() => window.calls.filter(c => c.c === 'tailscale_setup_install').length), 1);
  await p.close();
});

await test('bootstrap failure offers retry but browser never invokes IPC', async () => {
  const p = await page('i=download_failed');
  await seen(p, 'tailscale_setup_install');
  await p.getByTestId('retry-bootstrap').click();
  await seen(p, 'tailscale_setup_install', 2);
  await p.close();
  const browser = await page('n=0');
  assert.equal(await browser.evaluate(() => window.calls.length), 0);
  await browser.close();
});
```

Retain tests for explicit Tailscale GUI login, deadline cleanup, client no-Serve, host consent, conflict preservation, stale ready clearing, and reboot/timeout latches.

- [ ] **Step 2: Run the browser harness and confirm it fails against the install button**

Run:

```powershell
$env:RC_PLAYWRIGHT_MODULE='file:///C:/Users/user/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs'
node tests/e2e/tailscale-setup.mjs
```

Expected: FAIL because `tailscale_setup_install` is not invoked automatically and `data-testid="retry-bootstrap"` does not exist.

- [ ] **Step 3: Implement one-shot bootstrap state in `TailscaleSetup.svelte`**

Add a panel-lifetime `bootstrapAttempted` boolean and an `async bootstrap(id)` function. Its guards must be exactly equivalent to:

```ts
if (!native || !current(id) || busy || setup?.state !== 'not_installed' || bootstrapAttempted || rebootRequired || installBlocked) return;
bootstrapAttempted = true;
busy = true;
installing = true;
const result = await invoke<{ code: string }>('tailscale_setup_install');
```

After a successful or already-installed result, clear busy/installing and call `refresh(id)`. Map `download_failed` and `verification_failed` to a retryable Korean message; map cancellation and approval denial to retryable messages; preserve the current timeout and reboot latches. Clear `bootstrapAttempted` only from an explicit retry button, never from refresh polling.

Call `void bootstrap(id)` after `refresh` has safely stored a native `not_installed` status. Do not call it from browser mode, client/host Serve paths, a stale epoch, close, destroy, or while a command is busy. Replace the `Tailscale 설치` control with state text `원격 연결 준비 중` while automatic bootstrap is running and a `data-testid="retry-bootstrap"` button only after a terminal retryable error. Keep the existing login button as the explicit user action after installation.

- [ ] **Step 4: Run Svelte and browser tests**

Run:

```powershell
npm run check:web
$env:RC_PLAYWRIGHT_MODULE='file:///C:/Users/user/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs'
node tests/e2e/tailscale-setup.mjs
```

Expected: `svelte-check` reports zero errors/warnings and all browser groups pass.

- [ ] **Step 5: Commit the UI behavior**

```powershell
git add apps/web/src/TailscaleSetup.svelte tests/e2e/tailscale-setup.mjs
git commit -m "feat(web): bootstrap missing Tailscale automatically"
```

### Task 4: Build a fresh native artifact and record bounded evidence

**Files:**
- Create: `docs/test-results/tailscale-onboarding-2026-09-13/README.md`
- Create: `docs/test-results/tailscale-onboarding-2026-09-13/installer-mock.json`
- Create: `docs/test-results/tailscale-onboarding-2026-09-13/ui.json`
- Create: `docs/test-results/tailscale-onboarding-2026-09-13/desktop-tests.json`
- Modify: `IMPLEMENTATION_STATUS.md`
- Modify: `ACCEPTANCE_MATRIX.md`
- Modify: `CODEX_CONTINUE.md`

- [ ] **Step 1: Add the evidence directory before final commands**

Create the directory and record that this validation runs in the isolated
`work/tailscale-onboarding-20260912` worktree, not the frozen soak checkout.
The README must state that no real Tailscale installation, update, login, Serve
change, Agent restart, or existing account/policy test is performed.

- [ ] **Step 2: Run final validation**

Run:

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File tests/windows/tailscale-installer.test.ps1
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --quiet
npm run check:web
npm run build:web
node ../../../node_modules/@tauri-apps/cli/tauri.js build --no-bundle --ci
```

Expected: all commands pass. Record any existing compiler/bundler advisory
separately from failures. Hash the produced `remotecodex-desktop.exe`.

- [ ] **Step 3: Run a read-only native UI smoke**

Launch only the newly built Tauri executable with a fresh WebView2 profile and
loopback CDP port. Confirm the initial missing-Tailscale state automatically
enters preparation; do not approve UAC, click retry after a real download,
start an Agent, pair, log in, or enable Serve. Close only the test-owned desktop
PID after the check. Verify the frozen soak Agent PID, start ticks, and SHA-256
are unchanged.

- [ ] **Step 4: Update project records**

Update `IMPLEMENTATION_STATUS.md`, `ACCEPTANCE_MATRIX.md`, and
`CODEX_CONTINUE.md` with the commit hashes, exact commands, artifact hash,
automatic-bootstrap scope, Tailscale-managed auto-update scope for new installs,
and NOT_RUN external gates. Do not state that automatic installation or updates
were executed on this PC.

- [ ] **Step 5: Commit and push the evidence**

```powershell
git add docs/test-results/tailscale-onboarding-2026-09-13 IMPLEMENTATION_STATUS.md ACCEPTANCE_MATRIX.md CODEX_CONTINUE.md
git commit -m "docs: record automatic Tailscale bootstrap validation"
git push origin work/tailscale-onboarding-20260912
git ls-remote origin refs/heads/work/tailscale-onboarding-20260912
```

Expected: the remote branch head matches local `git rev-parse HEAD`.

## Self-review

* Spec coverage: Tasks 1–2 cover current official release selection, host/filename/checksum/signature restrictions, UAC/error paths, and new-install-only update policy. Task 3 covers the automatic UI flow and browser/native boundary. Task 4 covers builds, native smoke, provenance, and external limits.
* Placeholder scan: no unfinished markers or generic test steps remain; every code change lists a path, a concrete behavior, and a command.
* Type consistency: `InstallResult`, `tailscale_setup_install`, `SetupState::NotInstalled`, `data-testid="retry-bootstrap"`, and `TS_INSTALLUPDATES="always"` are used consistently throughout.
