//! Windows-only operations. Unsupported targets are explicit, never success-shaped mocks.
use std::path::Path;
pub mod autostart;
#[cfg(windows)]
pub mod gui_input;
#[cfg(windows)]
mod transaction;
pub mod tray;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use transaction::TransactionGuard;
#[cfg(windows)]
pub use windows::*;
#[cfg(not(windows))]
pub fn process_created(_: u32) -> anyhow::Result<u64> {
    anyhow::bail!("Windows process identity unavailable")
}
#[cfg(not(windows))]
pub fn process_image_path(_: u32) -> anyhow::Result<std::path::PathBuf> {
    anyhow::bail!("Windows process identity unavailable")
}
#[cfg(not(windows))]
pub fn monotonic_millis() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}
#[cfg(not(windows))]
pub fn pipe_name() -> anyhow::Result<String> {
    anyhow::bail!("Local administration requires Windows named pipes")
}
#[cfg(not(windows))]
pub fn restrict_data_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}
#[cfg(not(windows))]
pub fn loopback_listener_identity(_: u16) -> anyhow::Result<(u32, u64)> {
    anyhow::bail!("Loopback listener ownership is only implemented on Windows")
}

#[cfg(windows)]
mod dpi;
#[cfg(windows)]
mod sources;
#[cfg(windows)]
pub use sources::*;
#[cfg(windows)]
pub mod gui_session;
#[cfg(not(windows))]
pub fn enumerate_sources() -> anyhow::Result<Vec<rc_core::media_wire::LocalSource>> {
    anyhow::bail!("Windows capture sources unavailable")
}
#[cfg(not(windows))]
pub fn validate_source(
    _: &rc_core::media_wire::LocalSource,
) -> anyhow::Result<rc_core::geometry::Rect> {
    anyhow::bail!("Windows source validation unavailable")
}
#[cfg(not(windows))]
pub fn input_desktop_available() -> bool {
    false
}
