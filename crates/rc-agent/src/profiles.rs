use rc_core::error::ErrorCode;
use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct LaunchProfile {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) managed_codex: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProfileInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub program_path: String,
    pub args: Vec<String>,
    pub managed_codex: bool,
    pub composer_allowed: bool,
}

impl From<&LaunchProfile> for ProfileInfo {
    fn from(profile: &LaunchProfile) -> Self {
        Self {
            id: profile.id,
            label: profile.label,
            program_path: profile.program.to_string_lossy().into_owned(),
            args: profile
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            managed_codex: profile.managed_codex,
            composer_allowed: profile.managed_codex,
        }
    }
}

pub(crate) fn available() -> Vec<ProfileInfo> {
    installed().iter().map(ProfileInfo::from).collect()
}

pub(crate) fn resolve(name: &str) -> Result<LaunchProfile, ErrorCode> {
    installed()
        .into_iter()
        .find(|profile| profile.id == name)
        .ok_or(ErrorCode::InvalidRequest)
}

fn installed() -> Vec<LaunchProfile> {
    #[cfg(windows)]
    {
        let mut profiles = Vec::new();
        let root = env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
        let cmd = PathBuf::from(&root).join(r"System32\cmd.exe");
        if cmd.is_file() {
            profiles.push(LaunchProfile {
                id: "cmd",
                label: "CMD",
                program: cmd,
                args: vec![OsString::from("/D")],
                managed_codex: false,
            });
        }
        let powershell =
            PathBuf::from(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        if powershell.is_file() {
            profiles.push(LaunchProfile {
                id: "powershell",
                label: "Windows PowerShell",
                program: powershell,
                args: vec![OsString::from("-NoLogo")],
                managed_codex: false,
            });
        }
        if let Some(pwsh) = resolve_on_path("pwsh.exe") {
            profiles.push(LaunchProfile {
                id: "pwsh",
                label: "PowerShell 7",
                program: pwsh,
                args: vec![OsString::from("-NoLogo")],
                managed_codex: false,
            });
        }
        if let Some(codex) = resolve_codex() {
            profiles.push(LaunchProfile {
                id: "codex",
                label: "Managed Codex",
                program: codex,
                args: Vec::new(),
                managed_codex: true,
            });
        }
        profiles
    }
    #[cfg(not(windows))]
    {
        vec![LaunchProfile {
            id: "test-shell",
            label: "POSIX fixture ONLY",
            program: PathBuf::from("/bin/sh"),
            args: Vec::new(),
            managed_codex: false,
        }]
    }
}

#[cfg(windows)]
fn resolve_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| PathBuf::from(entry).join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn resolve_codex() -> Option<PathBuf> {
    let shim = resolve_on_path("codex.ps1")
        .or_else(|| env::var_os("APPDATA").map(|v| PathBuf::from(v).join(r"npm\codex.ps1")))?;
    let package = shim.parent()?.join(r"node_modules\@openai\codex");
    let native_package = package.join(r"node_modules\@openai\codex-win32-x64");
    find_native_codex(&native_package, 8)
}

#[cfg(windows)]
fn find_native_codex(root: &Path, depth: usize) -> Option<PathBuf> {
    if depth == 0 || !root.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("codex.exe"))
            && path
                .components()
                .any(|component| component.as_os_str().eq_ignore_ascii_case("vendor"))
        {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_native_codex(&path, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn codex_resolution_requires_native_vendor_executable() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory
            .path()
            .join(r"node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin");
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("codex.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        assert_eq!(find_native_codex(directory.path(), 8), Some(exe));

        let shim = directory.path().join("codex.ps1");
        std::fs::write(&shim, b"wrapper").unwrap();
        let wrapper = directory.path().join("codex.cmd");
        std::fs::write(&wrapper, b"wrapper").unwrap();
        assert!(find_native_codex(&directory.path().join("missing"), 6).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn native_codex_search_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let deep = directory.path().join(r"vendor\a\b\c\d\e\f");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("codex.exe"), b"fixture").unwrap();
        assert!(find_native_codex(directory.path(), 3).is_none());
    }
}
