//! Per-user, per-Windows-session administration. Never expose this on HTTP.
use crate::http::AppState;
use rc_core::protocol::Scope;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalCommand {
    PairOwner,
    PairReader,
    PairGui,
    MediaSources,
    MediaApprove {
        kind: String,
        handle: String,
        control: bool,
    },
    MediaClear,
    BlockRemote,
    ResumeRemote,
    Status,
    Shutdown,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    Active,
    Missing,
    Unsafe,
}

fn classify_open_error(raw_os_error: Option<i32>) -> ProbeStatus {
    if raw_os_error == Some(2) {
        ProbeStatus::Missing
    } else {
        ProbeStatus::Unsafe
    }
}

#[cfg(windows)]
pub async fn serve(state: Arc<AppState>) -> anyhow::Result<()> {
    use std::{os::windows::io::AsRawHandle, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ServerOptions,
    };
    let name = rc_platform_windows::pipe_name()?;
    let make = |first: bool| -> anyhow::Result<_> {
        let security = rc_platform_windows::LocalSecurity::new()?;
        let mut attributes = security.attributes();
        Ok(unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .max_instances(4)
                .in_buffer_size(4096)
                .out_buffer_size(4096)
                .create_with_security_attributes_raw(
                    &name,
                    std::ptr::from_mut(&mut attributes).cast(),
                )?
        })
    };
    let mut server = make(true)?;
    loop {
        tokio::select! {_ = state.shutdown.cancelled()=>return Ok(()), result=server.connect()=>result?}
        let verified = rc_platform_windows::verify_pipe_peer(server.as_raw_handle().cast(), true);
        let mut connected = server;
        server = make(false)?;
        if verified.is_err() {
            tracing::warn!("Rejected local IPC peer");
            continue;
        }
        // One tiny request per connection, bounded time and size. No unsolicited private output.
        let result=tokio::time::timeout(Duration::from_secs(3),async{
            let len=connected.read_u32().await? as usize;if len>2048{anyhow::bail!("Local request too large");}
            let mut data=vec![0;len];connected.read_exact(&mut data).await?;
            let cmd:LocalCommand=serde_json::from_slice(&data)?;
            let reply=match cmd{
                LocalCommand::PairOwner=>serde_json::json!({"ticket":state.auth.issue_pair_ticket(Scope::owner()).map_err(|e|anyhow::anyhow!("{e:?}"))?,"expires_in":300}),
                LocalCommand::PairGui=>{if !state.config.media.enabled{anyhow::bail!("Media not configured");}let mut scopes=Scope::owner();scopes.extend([Scope::WindowView,Scope::DesktopView]);if state.config.media.allow_control{scopes.extend([Scope::WindowControl,Scope::DesktopControl]);}serde_json::json!({"ticket":state.auth.issue_pair_ticket(scopes).map_err(|e|anyhow::anyhow!("{e:?}"))?,"expires_in":300})},
                LocalCommand::MediaSources=>serde_json::json!(rc_platform_windows::enumerate_sources()?),
                LocalCommand::MediaApprove{kind,handle,control}=>serde_json::json!(state.media.approve(&kind,&handle,control)?),
                LocalCommand::MediaClear=>{state.media.clear_sources();serde_json::json!({"cleared":true})},
                LocalCommand::PairReader=>serde_json::json!({"ticket":state.auth.issue_pair_ticket(vec![Scope::TerminalRead]).map_err(|e|anyhow::anyhow!("{e:?}"))?,"expires_in":300}),
                LocalCommand::BlockRemote=>{state.auth.block_all();state.sessions.block_input();state.previews.block_all();state.media.cancel_all();serde_json::json!({"blocked":true})},
                LocalCommand::ResumeRemote=>{state.auth.resume_local();serde_json::json!({"blocked":state.auth.is_blocked()})},
                LocalCommand::Status=>serde_json::json!({"epoch":state.sessions.epoch,"blocked":state.auth.is_blocked(),"sessions":state.sessions.list(),"local_origin":state.config.local_origin(),"public_origin":state.config.public_origin,"media":{"enabled":state.config.media.enabled,"allow_control":state.config.media.allow_control,"tailnet_ip":state.config.media.tailnet_ip,"allowed_peer_ips":state.config.media.allowed_peer_ips}}),
                LocalCommand::Shutdown=>{state.shutdown.cancel();serde_json::json!({"shutdown_requested":true})},
            };
            let bytes=serde_json::to_vec(&reply)?;connected.write_u32(bytes.len()as u32).await?;connected.write_all(&bytes).await?;connected.flush().await?;Ok::<_,anyhow::Error>(())
        }).await;
        if !matches!(result, Ok(Ok(()))) {
            tracing::warn!("Local IPC request rejected or timed out");
        }
    }
}
#[cfg(windows)]
pub async fn request(cmd: LocalCommand) -> anyhow::Result<serde_json::Value> {
    use std::os::windows::io::AsRawHandle;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };
    let mut pipe = ClientOptions::new().open(rc_platform_windows::pipe_name()?)?;
    rc_platform_windows::verify_pipe_peer(pipe.as_raw_handle().cast(), false)?;
    let bytes = serde_json::to_vec(&cmd)?;
    pipe.write_u32(bytes.len() as u32).await?;
    pipe.write_all(&bytes).await?;
    let len = pipe.read_u32().await? as usize;
    if len > 128 * 1024 {
        anyhow::bail!("Oversized local response");
    }
    let mut bytes = vec![0; len];
    pipe.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(windows)]
pub async fn probe_status() -> ProbeStatus {
    use std::os::windows::io::AsRawHandle;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };

    let name = match rc_platform_windows::pipe_name() {
        Ok(name) => name,
        Err(_) => return ProbeStatus::Unsafe,
    };
    let mut pipe = match ClientOptions::new().open(name) {
        Ok(pipe) => pipe,
        Err(error) => return classify_open_error(error.raw_os_error()),
    };
    if rc_platform_windows::verify_pipe_peer(pipe.as_raw_handle().cast(), false).is_err() {
        return ProbeStatus::Unsafe;
    }
    let operation = async {
        let bytes = serde_json::to_vec(&LocalCommand::Status)?;
        pipe.write_u32(bytes.len() as u32).await?;
        pipe.write_all(&bytes).await?;
        let len = pipe.read_u32().await? as usize;
        if len > 128 * 1024 {
            anyhow::bail!("Oversized local response");
        }
        let mut bytes = vec![0; len];
        pipe.read_exact(&mut bytes).await?;
        serde_json::from_slice::<serde_json::Value>(&bytes)?;
        Ok::<(), anyhow::Error>(())
    };
    match tokio::time::timeout(std::time::Duration::from_secs(5), operation).await {
        Ok(Ok(())) => ProbeStatus::Active,
        _ => ProbeStatus::Unsafe,
    }
}
#[cfg(not(windows))]
pub async fn serve(_: Arc<AppState>) -> anyhow::Result<()> {
    anyhow::bail!("Host administration is Windows-only")
}
#[cfg(not(windows))]
pub async fn request(_: LocalCommand) -> anyhow::Result<serde_json::Value> {
    anyhow::bail!("Host administration is Windows-only")
}
#[cfg(not(windows))]
pub async fn probe_status() -> ProbeStatus {
    ProbeStatus::Unsafe
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    #[test]
    fn only_file_not_found_means_missing() {
        assert_eq!(classify_open_error(Some(2)), ProbeStatus::Missing);
        for error in [None, Some(5), Some(231)] {
            assert_eq!(classify_open_error(error), ProbeStatus::Unsafe);
        }
    }
}
