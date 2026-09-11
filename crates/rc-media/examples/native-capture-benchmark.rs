use anyhow::Result;
#[cfg(all(windows, feature = "native-media"))]
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let handle: u64 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing handle"))?
        .parse()?;
    let width: u32 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing width"))?
        .parse()?;
    let height: u32 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing height"))?
        .parse()?;
    let fps: u32 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing fps"))?
        .parse()?;
    let bitrate: u32 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing bitrate"))?
        .parse()?;
    let seconds: u64 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing seconds"))?
        .parse()?;
    if args.next().is_some() {
        return Err(anyhow::anyhow!("expected exactly 6 positional arguments"));
    }
    let report = rc_media::native::run_owned_capture_encode_benchmark(
        handle, width, height, fps, bitrate, seconds,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
#[cfg(not(all(windows, feature = "native-media")))]
fn main() {
    eprintln!("native-media feature not enabled: unsupported");
    std::process::exit(2);
}
