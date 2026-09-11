use std::io::{self, Write};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--stdio"] {
        #[cfg(all(windows, feature = "native-media"))]
        {
            return rc_media::native::run();
        }
        #[cfg(not(all(windows, feature = "native-media")))]
        anyhow::bail!("Native media feature is unavailable");
    }
    if args.is_empty() || args == ["--probe"] {
        serde_json::to_writer(io::stdout().lock(), &rc_media::capabilities())?;
        println!();
        return Ok(());
    }
    writeln!(io::stderr(), "Usage: rc-media --probe | --stdio")?;
    std::process::exit(2);
}
