use crate::http::AppState;
use rc_core::protocol::Lifecycle;
use rc_platform_windows::tray::{run_tray, TrayAction, TraySnapshot, TrayState};
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

pub fn start(state: Arc<AppState>) -> anyhow::Result<()> {
    let snapshot_state = state.clone();
    let snapshot = Arc::new(move || snapshot(&snapshot_state));
    let action_state = state;
    let action = Arc::new(move |action| handle_action(&action_state, action));
    std::thread::Builder::new()
        .name("remotecodex-tray".to_owned())
        .spawn(move || {
            if let Err(error) = run_tray(snapshot, action) {
                tracing::error!(%error, "Agent tray stopped");
            }
        })?;
    Ok(())
}

fn snapshot(state: &AppState) -> TraySnapshot {
    let live_ptys = state
        .sessions
        .list()
        .into_iter()
        .filter(|session| {
            matches!(
                session.state,
                Lifecycle::Starting | Lifecycle::Running | Lifecycle::Closing
            )
        })
        .count();
    let tray_state = if state.auth.is_blocked() {
        TrayState::Blocked
    } else if state.media.gui_controlled() {
        TrayState::GuiControlled
    } else if state.remote_connections.active() > 0 {
        TrayState::RemoteConnected
    } else {
        TrayState::Idle
    };
    TraySnapshot {
        state: tray_state,
        live_ptys,
    }
}

fn handle_action(state: &AppState, action: TrayAction) -> anyhow::Result<()> {
    match action {
        TrayAction::OpenDesktop => open_desktop(),
        TrayAction::BlockRemote => {
            state.auth.block_all();
            state.sessions.block_input();
            state.previews.block_all();
            state.media.cancel_all();
            Ok(())
        }
        TrayAction::ResumeRemote => {
            state.auth.resume_local();
            Ok(())
        }
        TrayAction::StopGui => {
            state.media.cancel_all();
            Ok(())
        }
        TrayAction::ShutdownAgent => {
            state.shutdown.cancel();
            Ok(())
        }
    }
}

fn desktop_candidates(agent: &Path) -> anyhow::Result<[PathBuf; 2]> {
    let directory = agent
        .parent()
        .ok_or_else(|| anyhow::anyhow!("The Agent executable has no parent directory"))?;
    Ok([
        directory.join("remotecodex-desktop.exe"),
        directory.join("resources").join("remotecodex-desktop.exe"),
    ])
}

fn open_desktop() -> anyhow::Result<()> {
    let agent = std::env::current_exe()?;
    let candidates = desktop_candidates(&agent)?;
    let desktop = candidates
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not locate remotecodex-desktop.exe. Checked: {}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let child = Command::new(desktop)
        .current_dir(desktop.parent().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()?;
    drop(child);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_candidates_are_deterministic() {
        let agent = Path::new(r"C:\Program Files\RemoteCodex\rc-agent.exe");
        assert_eq!(
            desktop_candidates(agent).unwrap(),
            [
                PathBuf::from(r"C:\Program Files\RemoteCodex\remotecodex-desktop.exe"),
                PathBuf::from(r"C:\Program Files\RemoteCodex\resources\remotecodex-desktop.exe")
            ]
        );
    }
}
