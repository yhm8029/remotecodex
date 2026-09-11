use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAttach {
    Attached,
    Started,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HostPaths {
    pub agent: PathBuf,
    pub config: PathBuf,
    pub web_dir: PathBuf,
}

pub(crate) fn discover_paths(
    exe: &Path,
    local_app_data: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Result<HostPaths, String> {
    let exe_dir = exe.parent().ok_or_else(|| {
        format!(
            "Cannot determine the application directory from {}",
            exe.display()
        )
    })?;
    let agent_candidates = [
        exe_dir.join("rc-agent.exe"),
        exe_dir.join("resources").join("rc-agent.exe"),
    ];
    let web_candidates = [
        exe_dir.join("web").join("index.html"),
        exe_dir.join("resources").join("web").join("index.html"),
    ];
    let agent = agent_candidates
        .iter()
        .find(|path| exists(path))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Could not locate rc-agent.exe. Checked: {}",
                agent_candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let web_index = web_candidates
        .iter()
        .find(|path| exists(path))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Could not locate web/index.html. Checked: {}",
                web_candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    Ok(HostPaths {
        agent,
        config: local_app_data.join("RemoteCodex").join("agent.toml"),
        web_dir: web_index
            .parent()
            .ok_or_else(|| "web/index.html has no parent directory".to_owned())?
            .to_path_buf(),
    })
}

pub(crate) fn agent_args(paths: &HostPaths) -> Vec<OsString> {
    vec![
        OsString::from("--config"),
        paths.config.as_os_str().to_owned(),
        OsString::from("run"),
    ]
}

pub(crate) fn render_config(web_dir: &Path) -> Result<String, String> {
    let value = web_dir
        .to_str()
        .ok_or_else(|| "The web asset path is not valid Unicode".to_owned())?;
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    Ok(format!("web_dir = \"{escaped}\"\n"))
}

#[cfg(windows)]
pub(crate) fn installed_host_paths() -> Result<HostPaths, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is unavailable".to_owned())?;
    discover_paths(&exe, &local_app_data, Path::is_file)
}

#[cfg(windows)]
pub fn start_detached_host() -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let paths = installed_host_paths()?;
    let config_parent = paths
        .config
        .parent()
        .ok_or_else(|| "The Agent config path has no parent directory".to_owned())?;
    std::fs::create_dir_all(config_parent)
        .map_err(|error| format!("Could not create {}: {error}", config_parent.display()))?;
    let config_contents = render_config(&paths.web_dir)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.config)
    {
        Ok(mut file) => file
            .write_all(config_contents.as_bytes())
            .map_err(|error| format!("Could not write {}: {error}", paths.config.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(format!(
                "Could not create {}: {error}",
                paths.config.display()
            ));
        }
    }
    let agent_dir = paths
        .agent
        .parent()
        .ok_or_else(|| "The Agent executable path has no parent directory".to_owned())?;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let child = Command::new(&paths.agent)
        .args(agent_args(&paths))
        .current_dir(agent_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", paths.agent.display()))?;
    drop(child);
    Ok(())
}

#[cfg(not(windows))]
pub fn start_detached_host() -> Result<(), String> {
    Err("Hosting is available only on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("rc-lifecycle-{name}-{}", std::process::id()));
        (root.join("RemoteCodex.exe"), root.join("LocalAppData"))
    }

    #[test]
    fn selects_primary_runtime_and_static_paths() {
        let (exe, local) = fixture("primary");
        let agent = exe.parent().unwrap().join("rc-agent.exe");
        let index = exe.parent().unwrap().join("web").join("index.html");
        let paths = discover_paths(&exe, &local, |path| path == agent || path == index).unwrap();
        assert_eq!(paths.agent, agent);
        assert_eq!(paths.web_dir, exe.parent().unwrap().join("web"));
    }

    #[test]
    fn selects_resource_fallbacks() {
        let (exe, local) = fixture("resources");
        let agent = exe.parent().unwrap().join("resources").join("rc-agent.exe");
        let index = exe
            .parent()
            .unwrap()
            .join("resources")
            .join("web")
            .join("index.html");
        let paths = discover_paths(&exe, &local, |path| path == agent || path == index).unwrap();
        assert_eq!(paths.agent, agent);
        assert_eq!(
            paths.web_dir,
            exe.parent().unwrap().join("resources").join("web")
        );
    }

    #[test]
    fn derives_per_user_config_without_requiring_it_to_exist() {
        let (exe, local) = fixture("config");
        let agent = exe.parent().unwrap().join("rc-agent.exe");
        let index = exe.parent().unwrap().join("web").join("index.html");
        let paths = discover_paths(&exe, &local, |path| path == agent || path == index).unwrap();
        assert_eq!(paths.config, local.join("RemoteCodex").join("agent.toml"));
    }

    #[test]
    fn builds_exact_agent_arguments() {
        let paths = HostPaths {
            agent: PathBuf::from("rc-agent.exe"),
            config: PathBuf::from(r"C:\Users\test\agent.toml"),
            web_dir: PathBuf::from(r"C:\Program Files\RemoteCodex\web"),
        };
        assert_eq!(
            agent_args(&paths),
            vec![
                OsString::from("--config"),
                paths.config.as_os_str().to_owned(),
                OsString::from("run")
            ]
        );
    }

    #[test]
    fn renders_windows_paths_as_toml_basic_strings() {
        assert_eq!(
            render_config(Path::new(r#"C:\Program Files\Remote "Codex"\web"#)).unwrap(),
            "web_dir = \"C:\\\\Program Files\\\\Remote \\\"Codex\\\"\\\\web\"\n"
        );
    }

    #[test]
    fn escapes_toml_control_characters() {
        assert_eq!(
            render_config(Path::new("line\nreturn\rtab\tend")).unwrap(),
            "web_dir = \"line\\nreturn\\rtab\\tend\"\n"
        );
    }

    #[test]
    fn missing_agent_error_lists_all_candidates() {
        let (exe, local) = fixture("missing-agent");
        let err = discover_paths(&exe, &local, |_| false).unwrap_err();
        assert!(err.contains("rc-agent.exe"));
        assert!(err.contains(
            &exe.parent()
                .unwrap()
                .join("rc-agent.exe")
                .display()
                .to_string()
        ));
        assert!(err.contains(
            &exe.parent()
                .unwrap()
                .join("resources")
                .join("rc-agent.exe")
                .display()
                .to_string()
        ));
    }

    #[test]
    fn missing_web_error_lists_all_candidates() {
        let (exe, local) = fixture("missing-web");
        let agent = exe.parent().unwrap().join("rc-agent.exe");
        let err = discover_paths(&exe, &local, |path| path == agent).unwrap_err();
        assert!(err.contains("web/index.html"));
        assert!(err.contains(
            &exe.parent()
                .unwrap()
                .join("web")
                .join("index.html")
                .display()
                .to_string()
        ));
        assert!(err.contains(
            &exe.parent()
                .unwrap()
                .join("resources")
                .join("web")
                .join("index.html")
                .display()
                .to_string()
        ));
    }
}
