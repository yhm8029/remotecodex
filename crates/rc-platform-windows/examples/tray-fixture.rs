use anyhow::{anyhow, Context, Result};
use rc_platform_windows::tray::{run_tray, TrayAction, TraySnapshot, TrayState};
use serde_json::json;
use std::{
    env,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

fn action_name(action: TrayAction) -> &'static str {
    match action {
        TrayAction::OpenDesktop => "OpenDesktop",
        TrayAction::BlockRemote => "BlockRemote",
        TrayAction::ResumeRemote => "ResumeRemote",
        TrayAction::StopGui => "StopGui",
        TrayAction::ShutdownAgent => "ShutdownAgent",
    }
}

fn output_path() -> Result<PathBuf> {
    env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("usage: tray-fixture <exclusive-action.ndjson>"))
}

fn open_exclusive(path: &PathBuf) -> Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create exclusive action log {}", path.display()))?;
    Ok(BufWriter::new(file))
}

fn unix_millis() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before Unix epoch")?
        .as_millis())
}

fn main() -> Result<()> {
    let path = output_path()?;
    let log = Arc::new(Mutex::new(open_exclusive(&path)?));
    let snapshot = Arc::new(|| TraySnapshot {
        state: TrayState::Idle,
        live_ptys: 0,
    });
    let action_log = Arc::clone(&log);
    let action = Arc::new(move |selected: TrayAction| {
        let record = json!({
            "action": action_name(selected),
            "selected_at_unix_ms": unix_millis()?,
        });
        let mut output = action_log
            .lock()
            .map_err(|_| anyhow!("action log mutex poisoned"))?;
        writeln!(output, "{}", record).context("write selected tray action")?;
        output.flush().context("flush selected tray action")?;
        Ok(())
    });

    // The fixture owns only this tray loop. ShutdownAgent exits run_tray after
    // recording the selection; it does not stop another Agent or use IPC.
    run_tray(snapshot, action)
}
