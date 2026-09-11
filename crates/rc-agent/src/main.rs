mod auth;
mod config;
mod http;
mod local;
mod media;
mod preview;
mod profiles;
mod session;
mod store;
mod terminal;
#[cfg(windows)]
mod tray;
mod ws;
use clap::{Parser, Subcommand};
use fs2::FileExt;
use std::future::IntoFuture;
use std::{path::PathBuf, sync::Arc};
#[derive(Parser)]
#[command(
    name = "rc-agent",
    version,
    about = "Visible, per-user RemoteCodex PTY host"
)]
struct Args {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Run,
    Pair {
        #[arg(long)]
        reader: bool,
        #[arg(long, conflicts_with = "reader")]
        gui: bool,
    },
    MediaSources,
    MediaApprove {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        handle: String,
        #[arg(long)]
        control: bool,
    },
    MediaClear,
    Status,
    StatusProbe,
    BlockRemote,
    ResumeRemote,
    Shutdown,
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if matches!(&args.command, Command::StatusProbe) {
        std::process::exit(match local::probe_status().await {
            local::ProbeStatus::Active => 0,
            local::ProbeStatus::Missing => 3,
            local::ProbeStatus::Unsafe => 4,
        });
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let cmd = match args.command {
        Command::Run => None,
        Command::Pair { reader, gui } => Some(if gui {
            local::LocalCommand::PairGui
        } else if reader {
            local::LocalCommand::PairReader
        } else {
            local::LocalCommand::PairOwner
        }),
        Command::MediaSources => Some(local::LocalCommand::MediaSources),
        Command::MediaApprove {
            kind,
            handle,
            control,
        } => Some(local::LocalCommand::MediaApprove {
            kind,
            handle,
            control,
        }),
        Command::MediaClear => Some(local::LocalCommand::MediaClear),
        Command::Status => Some(local::LocalCommand::Status),
        Command::BlockRemote => Some(local::LocalCommand::BlockRemote),
        Command::ResumeRemote => Some(local::LocalCommand::ResumeRemote),
        Command::Shutdown => Some(local::LocalCommand::Shutdown),
        Command::StatusProbe => unreachable!(),
    };
    if let Some(cmd) = cmd {
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(5), local::request(cmd)).await??;
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }
    #[cfg(not(windows))]
    anyhow::bail!("The production host is Windows-only. Portable unit tests remain available with cargo test.");
    #[cfg(windows)]
    {
        let config = config::Config::load(args.config.as_ref())?;
        rc_platform_windows::restrict_data_dir(&config.data_dir)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(config.data_dir.join("agent.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| anyhow::anyhow!("An Agent already owns this user data directory"))?;
        let store = Arc::new(store::Store::open(&config.data_dir.join("state.sqlite3"))?);
        let epoch = uuid::Uuid::new_v4();
        let audience = config
            .public_origin
            .clone()
            .unwrap_or_else(|| config.local_origin());
        let state = Arc::new(http::AppState {
            auth: auth::Auth::new(store.clone(), audience)?,
            sessions: session::SessionManager::new(config.clone(), store.clone(), epoch),
            previews: Default::default(),
            media: media::MediaManager::new(config.media.clone()),
            shutdown: Default::default(),
            remote_connections: Default::default(),
            config: config.clone(),
            store,
        });
        let listener = tokio::net::TcpListener::bind(config.bind).await?;
        tray::start(state.clone())?;
        let local_state = state.clone();
        let pipe = tokio::spawn(async move { local::serve(local_state).await });
        println!(
            "RemoteCodex Agent visible and running at {}. UI closure does NOT stop PTYs.",
            config.local_origin()
        );
        println!("Run rc-agent pair locally to approve this browser. Ctrl+C here explicitly stops the Agent and its PTYs.");
        let cancel = state.shutdown.clone();
        let signal = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        });
        let serve = axum::serve(listener, http::router(state.clone()))
            .with_graceful_shutdown(state.shutdown.clone().cancelled_owned())
            .into_future();
        tokio::pin!(serve);
        // A failed local administration channel is fatal, rather than leaving an unmanageable host.
        tokio::select! {r=&mut serve=>{r?;},r=pipe=>{match r{Ok(Ok(()))=>{},Ok(Err(e))=>tracing::error!(error=%e,"Local administration stopped"),Err(e)=>tracing::error!(error=%e,"Local task failed")};state.shutdown.cancel();}}
        state.media.cancel_all();
        state.previews.shutdown();
        let s = state.clone();
        tokio::task::spawn_blocking(move || s.sessions.shutdown()).await?;
        signal.abort();
        drop(lock);
        Ok(())
    }
}
