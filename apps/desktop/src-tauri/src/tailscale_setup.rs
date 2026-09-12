use crate::tailscale::CliRunner;
use serde::{Deserialize, Serialize};
use std::env;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    Installing,
    NotInstalled,
    CliUnavailable,
    ServiceUnavailable,
    LoginRequired,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupStatus {
    pub state: SetupState,
    pub detail: String,
}

#[derive(Debug, Deserialize)]
struct StatusPayload {
    #[serde(rename = "BackendState")]
    backend_state: String,
    #[serde(default, rename = "Self")]
    self_payload: Option<SelfInner>,
}

#[derive(Debug, Deserialize)]
struct SelfInner {
    #[serde(rename = "Online")]
    online: Option<bool>,
}

pub fn classify_status(
    executable_present: bool,
    version_ok: bool,
    status_output: Result<&str, &str>,
) -> SetupStatus {
    if !executable_present {
        return SetupStatus {
            state: SetupState::NotInstalled,
            detail: detail_not_installed(),
        };
    }
    if !version_ok {
        return SetupStatus {
            state: SetupState::CliUnavailable,
            detail: detail_cli_unavailable(),
        };
    }
    let stdout = match status_output {
        Ok(s) => s,
        Err(_) => {
            return SetupStatus {
                state: SetupState::ServiceUnavailable,
                detail: detail_service_unavailable(),
            };
        }
    };
    let payload: StatusPayload = match serde_json::from_str(stdout) {
        Ok(p) => p,
        Err(_) => {
            return SetupStatus {
                state: SetupState::CliUnavailable,
                detail: detail_cli_unavailable(),
            };
        }
    };
    let online = payload.self_payload.and_then(|s| s.online);
    match payload.backend_state.as_str() {
        "NeedsLogin" => SetupStatus {
            state: SetupState::LoginRequired,
            detail: detail_login_required(),
        },
        "NeedsMachineAuth" => SetupStatus {
            state: SetupState::Connecting,
            detail: detail_machine_approval(),
        },
        "Starting" => SetupStatus {
            state: SetupState::Connecting,
            detail: detail_connecting_starting(),
        },
        "Stopped" => SetupStatus {
            state: SetupState::Connecting,
            detail: detail_stopped(),
        },
        "Running" => match online {
            Some(true) => SetupStatus {
                state: SetupState::Connected,
                detail: detail_connected(),
            },
            _ => SetupStatus {
                state: SetupState::Connecting,
                detail: detail_offline(),
            },
        },
        _ => SetupStatus {
            state: SetupState::CliUnavailable,
            detail: detail_cli_unavailable(),
        },
    }
}

pub fn discover_cli(
    candidates: impl IntoIterator<Item = PathBuf>,
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    for cand in candidates {
        if !cand.is_absolute() {
            continue;
        }
        if is_file(&cand) {
            return Some(cand);
        }
    }
    None
}

fn detail_not_installed() -> String {
    "Tailscale CLI was not found on this system.".to_string()
}
fn detail_cli_unavailable() -> String {
    "Installed Tailscale CLI did not respond to a version check or returned an unrecognized status payload.".to_string()
}
fn detail_service_unavailable() -> String {
    "Tailscale service did not respond; the CLI could not reach the local backend.".to_string()
}
fn detail_login_required() -> String {
    "Tailscale is installed but not logged in. Please authenticate this device to continue."
        .to_string()
}
fn detail_machine_approval() -> String {
    "Tailscale is waiting for an administrator to approve this device on the admin console."
        .to_string()
}
fn detail_connecting_starting() -> String {
    "Tailscale is starting up; please wait while it connects.".to_string()
}
fn detail_stopped() -> String {
    "Tailscale is stopped. Open the Tailscale app or re-enable the service to connect.".to_string()
}
fn detail_offline() -> String {
    "Tailscale reports the backend is running but the device is offline; please check your network."
        .to_string()
}
fn detail_connected() -> String {
    "Tailscale is connected and online.".to_string()
}

pub fn classify_command_output(success: bool, stdout: &str) -> SetupStatus {
    let trimmed = stdout.trim();
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return classify_status(true, true, Ok(trimmed));
    }
    if success {
        classify_status(true, true, Ok(trimmed))
    } else {
        classify_status(true, true, Err("status"))
    }
}

pub fn status() -> SetupStatus {
    #[cfg(not(windows))]
    {
        return SetupStatus {
            state: SetupState::CliUnavailable,
            detail: detail_cli_unavailable(),
        };
    }
    #[cfg(windows)]
    {
        if INSTALLING.load(Ordering::SeqCst) {
            return SetupStatus {
                state: SetupState::Installing,
                detail: "Tailscale installation is still in progress. Close the installer, then reopen RemoteCodex if it does not finish.".into(),
            };
        }
        if resolve_cli().is_none() {
            return classify_status(false, false, Ok(""));
        }
        let runner = crate::tailscale::SystemRunner;
        if !matches!(runner.run(&["version"]), Ok(ref output) if output.success) {
            return classify_status(true, false, Ok(""));
        }
        match runner.run(&["status", "--json"]) {
            Ok(output) => classify_command_output(output.success, &output.stdout),
            Err(_) => classify_status(true, true, Err("status")),
        }
    }
}

pub fn open_login() -> Result<(), String> {
    #[cfg(not(windows))]
    {
        return Err("Tailscale login is available only on Windows".into());
    }
    #[cfg(windows)]
    {
        if !matches!(
            status().state,
            SetupState::LoginRequired | SetupState::Connecting
        ) {
            return Err("Tailscale login is not available in the current state".into());
        }
        let cli = resolve_cli().ok_or_else(|| "Tailscale CLI is not installed".to_owned())?;
        let ipn = cli
            .parent()
            .ok_or_else(|| "Tailscale installation path is invalid".to_owned())?
            .join("tailscale-ipn.exe");
        if !ipn.is_file() {
            return Err("Tailscale desktop login executable was not found".into());
        }
        Command::new(ipn)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not open Tailscale login".to_owned())?;
        Ok(())
    }
}

pub fn install() -> Result<InstallResult, String> {
    #[cfg(not(windows))]
    {
        return Err("Tailscale installation is available only on Windows".into());
    }
    #[cfg(windows)]
    {
        let mut guard = InstallGuard::acquire(&INSTALLING)?;
        if resolve_cli().is_some() {
            return Ok(InstallResult {
                code: "already_installed".into(),
                version: "installed".into(),
                source: "existing".into(),
                installer_pid: None,
                installer_start_ticks: None,
            });
        }
        let root = env::var_os("SystemRoot")
            .ok_or_else(|| "Windows installation environment is unavailable".to_owned())?;
        let powershell =
            PathBuf::from(root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        if !powershell.is_absolute() || !powershell.is_file() {
            return Err("Windows PowerShell was not found".into());
        }
        let mut command = Command::new(powershell);
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("tailscale-install.ps1"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000);
        let output = command
            .output()
            .map_err(|_| "Tailscale installer could not be started".to_owned())?;
        let parsed: InstallResult = serde_json::from_slice(&output.stdout)
            .map_err(|_| "Tailscale installer returned invalid output".to_owned())?;
        if !allowed_code(&parsed.code) {
            return Err("Tailscale installer returned an unknown result".into());
        }
        if parsed.code == "timeout" {
            guard.release = false;
            if let (Some(pid), Some(ticks)) = (
                parsed.installer_pid,
                parsed
                    .installer_start_ticks
                    .as_deref()
                    .and_then(|value| value.parse().ok()),
            ) {
                let _ = track_installer(pid, ticks);
            }
        }
        if parsed.code == "installed" && resolve_cli().is_none() {
            return Ok(InstallResult {
                code: "install_failed".into(),
                version: "unknown".into(),
                source: "official".into(),
                installer_pid: None,
                installer_start_ticks: None,
            });
        }
        Ok(parsed)
    }
}

fn allowed_code(code: &str) -> bool {
    [
        "installed",
        "already_installed",
        "reboot_required",
        "cancelled",
        "approval_denied",
        "busy",
        "install_failed",
        "download_failed",
        "verification_failed",
        "unsupported_platform",
        "timeout",
    ]
    .contains(&code)
}
struct InstallGuard<'a> {
    flag: &'a AtomicBool,
    release: bool,
}
impl<'a> InstallGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self {
                flag,
                release: true,
            })
            .map_err(|_| "Tailscale installation is already in progress".into())
    }
    #[cfg(test)]
    fn keep_installing(&mut self) {
        self.release = false;
    }
}
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        if self.release {
            self.flag.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonzero_valid_needs_login_json_is_login_required() {
        let s = classify_command_output(
            false,
            r#"{"BackendState":"NeedsLogin","Self":{"Online":false}}"#,
        );
        assert_eq!(s.state, SetupState::LoginRequired);
    }

    #[test]
    fn successful_malformed_status_is_cli_unavailable() {
        assert_eq!(
            classify_command_output(true, "garbage").state,
            SetupState::CliUnavailable
        );
    }

    #[test]
    fn failed_empty_status_is_service_unavailable() {
        assert_eq!(
            classify_command_output(false, "").state,
            SetupState::ServiceUnavailable
        );
    }

    #[test]
    fn installer_code_allowlist_is_enforced() {
        assert!(allowed_code("installed"));
        assert!(allowed_code("timeout"));
        assert!(!allowed_code("unexpected"));
    }

    #[test]
    fn absent_executable_reports_not_installed() {
        let s = classify_status(false, true, Ok("{}"));
        assert_eq!(s.state, SetupState::NotInstalled);
        assert!(!s.detail.is_empty());
    }

    #[test]
    fn present_but_invalid_version_reports_cli_unavailable() {
        let s = classify_status(true, false, Ok("{}"));
        assert_eq!(s.state, SetupState::CliUnavailable);
    }

    #[test]
    fn status_transport_error_reports_service_unavailable() {
        let s = classify_status(true, true, Err("boom"));
        assert_eq!(s.state, SetupState::ServiceUnavailable);
    }

    #[test]
    fn logged_out_nonzero_json_still_classified_as_login_required() {
        let stdout = r#"{"BackendState":"NeedsLogin","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::LoginRequired);
    }

    #[test]
    fn needs_machine_auth_is_connecting_with_approval_detail() {
        let stdout = r#"{"BackendState":"NeedsMachineAuth","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::Connecting);
        assert!(s.detail.to_lowercase().contains("approve"));
    }

    #[test]
    fn starting_state_is_connecting() {
        let stdout = r#"{"BackendState":"Starting","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::Connecting);
    }

    #[test]
    fn stopped_state_is_connecting_with_app_instruction() {
        let stdout = r#"{"BackendState":"Stopped","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::Connecting);
        assert!(s.detail.to_lowercase().contains("app"));
    }

    #[test]
    fn running_online_is_connected() {
        let stdout = r#"{"BackendState":"Running","Self":{"Online":true}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::Connected);
    }

    #[test]
    fn running_offline_is_connecting() {
        let stdout = r#"{"BackendState":"Running","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::Connecting);
    }

    #[test]
    fn malformed_json_is_cli_unavailable() {
        let s = classify_status(true, true, Ok("not json"));
        assert_eq!(s.state, SetupState::CliUnavailable);
    }

    #[test]
    fn unknown_backend_state_is_cli_unavailable() {
        let stdout = r#"{"BackendState":"WeirdState","Self":{"Online":false}}"#;
        let s = classify_status(true, true, Ok(stdout));
        assert_eq!(s.state, SetupState::CliUnavailable);
    }

    #[test]
    fn details_never_echo_inputs_or_account_data() {
        let secret = "user@example.com AKIAABCDEFGHIJKLMNOP";
        let stdout = format!(
            r#"{{"BackendState":"{}","User":"{}","Self":{{"Online":false}}}}"#,
            "NeedsLogin", secret
        );
        let s = classify_status(true, true, Ok(&stdout));
        assert!(!s.detail.contains("AKIAABCDEFGHIJKLMNOP"));
        assert!(!s.detail.contains("user@example.com"));
        assert!(!s.detail.contains("NeedsLogin"));
    }

    #[test]
    fn discover_prefers_absolute_install_path_over_path_candidate() {
        let (abs, path_abs) = if cfg!(windows) {
            (
                PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"),
                PathBuf::from(r"C:\Windows\System32\tailscale.exe"),
            )
        } else {
            (
                PathBuf::from("/opt/tailscale"),
                PathBuf::from("/usr/bin/tailscale"),
            )
        };
        let chosen = discover_cli(vec![abs.clone(), path_abs.clone()], |p| {
            p.file_name().and_then(|n| n.to_str())
                == Some(if cfg!(windows) {
                    "tailscale.exe"
                } else {
                    "tailscale"
                })
        });
        assert_eq!(chosen, Some(abs));
    }

    #[test]
    fn discover_rejects_relative_paths() {
        let rel = PathBuf::from("tailscale");
        let chosen = discover_cli(vec![rel], |_| true);
        assert!(chosen.is_none());
    }

    #[test]
    fn discover_empty_candidates_returns_none() {
        let chosen: Option<PathBuf> = discover_cli(Vec::<PathBuf>::new(), |_| true);
        assert!(chosen.is_none());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub code: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing)]
    pub installer_pid: Option<u32>,
    #[serde(default, skip_serializing)]
    pub installer_start_ticks: Option<String>,
}
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn timeout_trackable(
    expected_pid: Option<u32>,
    expected_ticks: Option<u64>,
    observed: Option<(u32, u64)>,
) -> bool {
    match (expected_pid, expected_ticks, observed) {
        (Some(pid), Some(ticks), Some((actual_pid, actual_ticks))) => {
            pid == actual_pid && ticks == actual_ticks
        }
        _ => false,
    }
}

#[cfg(windows)]
fn track_installer(pid: u32, ticks: u64) -> bool {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    const ACCESS: u32 = 0x0010_0000 | 0x1000;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetProcessTimes(
            handle: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    unsafe {
        let handle = OpenProcess(ACCESS, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut creation = FileTime { low: 0, high: 0 };
        let mut exit = FileTime { low: 0, high: 0 };
        let mut kernel = FileTime { low: 0, high: 0 };
        let mut user = FileTime { low: 0, high: 0 };
        let actual = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0;
        let actual_ticks = ((creation.high as u64) << 32) | creation.low as u64;
        if !actual || !timeout_trackable(Some(pid), Some(ticks), Some((pid, actual_ticks))) {
            CloseHandle(handle);
            return false;
        }
        let handle_value = handle as usize;
        std::thread::spawn(move || unsafe {
            let handle = handle_value as Handle;
            let wait_result = WaitForSingleObject(handle, 0xffff_ffff);
            CloseHandle(handle);
            if wait_result == 0 {
                INSTALLING.store(false, Ordering::SeqCst);
            }
        });
        true
    }
}

pub fn resolve_cli() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut candidates = Vec::new();
        for var in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = env::var_os(var) {
                candidates.push(PathBuf::from(dir).join("Tailscale").join("tailscale.exe"));
            }
        }
        if let Some(path) = env::var_os("PATH") {
            candidates.extend(env::split_paths(&path).map(|p| p.join("tailscale.exe")));
        }
        discover_cli(candidates, Path::is_file)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    #[test]
    fn guard_releases_on_normal_drop() {
        let flag = AtomicBool::new(false);
        {
            let _guard = InstallGuard::acquire(&flag).unwrap();
            assert!(flag.load(Ordering::SeqCst));
        }
        assert!(!flag.load(Ordering::SeqCst));
    }
    #[test]
    fn guard_retains_flag_on_timeout() {
        let flag = AtomicBool::new(false);
        {
            let mut guard = InstallGuard::acquire(&flag).unwrap();
            guard.release = false;
        }
        assert!(flag.load(Ordering::SeqCst));
    }
    #[test]
    fn duplicate_guard_acquisition_is_rejected() {
        let flag = AtomicBool::new(false);
        let _guard = InstallGuard::acquire(&flag).unwrap();
        assert!(InstallGuard::acquire(&flag).is_err());
    }
    #[test]
    fn timeout_tracking_requires_exact_process_identity() {
        assert!(timeout_trackable(Some(7), Some(11), Some((7, 11))));
        assert!(!timeout_trackable(Some(7), Some(11), Some((8, 11))));
        assert!(!timeout_trackable(Some(7), Some(11), Some((7, 12))));
        assert!(!timeout_trackable(Some(7), Some(11), None));
    }
}
