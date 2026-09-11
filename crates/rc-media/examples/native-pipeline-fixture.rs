#[cfg(all(windows, feature = "native-media"))]
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let handle = args
        .next()
        .as_deref()
        .context("usage: native-pipeline-fixture <owned-window-handle>")?
        .parse::<u64>()?;
    let report = rc_media::native::run_owned_capture_encode_fixture(handle)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(all(windows, feature = "native-media"))]
use anyhow::Context;

#[cfg(not(all(windows, feature = "native-media")))]
fn main() {
    eprintln!("native-pipeline-fixture requires Windows and rc-media/native-media");
    std::process::exit(2);
}
