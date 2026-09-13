#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod lifecycle;
mod tailscale;
mod tailscale_setup;
use tauri_plugin_opener::OpenerExt;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaApproval {
    kind: String,
    handle: String,
    control: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct MediaSettings {
    enabled: bool,
    allow_control: bool,
    tailnet_ip: Option<String>,
    allowed_peer_ips: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct MediaConfigStatus {
    stored: MediaSettings,
    live: Option<MediaSettings>,
    agent_running: bool,
    restart_required: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaConfigRequest {
    enabled: bool,
    allow_control: bool,
    tailnet_ip: Option<String>,
    allowed_peer_ips: Vec<String>,
    consent: bool,
}

fn validate_media_settings(settings: &MediaSettings) -> Result<(), String> {
    if settings.allowed_peer_ips.len() > 16
        || settings
            .allowed_peer_ips
            .iter()
            .any(|ip| !rc_core::media_wire::tailnet_ip(ip))
    {
        return Err("Allowed peers must be numeric 100.64.0.0/10 Tailscale IPv4 addresses".into());
    }
    if let Some(ip) = settings.tailnet_ip.as_deref() {
        if !rc_core::media_wire::tailnet_ip(ip) {
            return Err("The host Tailscale IPv4 address is invalid".into());
        }
    }
    if settings.enabled && (settings.tailnet_ip.is_none() || settings.allowed_peer_ips.is_empty()) {
        return Err("Enabled media needs a host Tailscale IP and at least one allowed peer".into());
    }
    if !settings.enabled && settings.allow_control {
        return Err("GUI control can only be enabled with media".into());
    }
    Ok(())
}

fn media_from_document(doc: &toml_edit::DocumentMut) -> Result<MediaSettings, String> {
    let table = doc.get("media").and_then(|item| item.as_table());
    let bool_field = |name: &str, default| -> Result<bool, String> {
        match table.and_then(|t| t.get(name)) {
            None => Ok(default),
            Some(item) => item
                .as_bool()
                .ok_or_else(|| format!("media.{name} must be a boolean")),
        }
    };
    let tailnet_ip = match table.and_then(|t| t.get("tailnet_ip")) {
        None => None,
        Some(item) => Some(
            item.as_str()
                .ok_or_else(|| "media.tailnet_ip must be a string".to_owned())?
                .to_owned(),
        ),
    };
    let allowed_peer_ips = match table.and_then(|t| t.get("allowed_peer_ips")) {
        None => Vec::new(),
        Some(item) => item
            .as_array()
            .ok_or_else(|| "media.allowed_peer_ips must be an array".to_owned())?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "media.allowed_peer_ips must contain strings".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let settings = MediaSettings {
        enabled: bool_field("enabled", false)?,
        allow_control: bool_field("allow_control", false)?,
        tailnet_ip,
        allowed_peer_ips,
    };
    validate_media_settings(&settings)?;
    Ok(settings)
}

fn set_media_document(path: &std::path::Path, settings: &MediaSettings) -> Result<(), String> {
    validate_media_settings(settings)?;
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("Invalid Agent configuration: {error}"))?;
    if doc.get("media").is_none() {
        doc["media"] = toml_edit::table();
    }
    let media = doc["media"]
        .as_table_mut()
        .ok_or_else(|| "media must be a TOML table".to_owned())?;
    media["enabled"] = toml_edit::value(settings.enabled);
    media["allow_control"] = toml_edit::value(settings.allow_control);
    if let Some(ip) = settings.tailnet_ip.as_deref() {
        media["tailnet_ip"] = toml_edit::value(ip);
    } else {
        media.remove("tailnet_ip");
    }
    let mut peers = toml_edit::Array::default();
    for ip in &settings.allowed_peer_ips {
        peers.push(ip);
    }
    media["allowed_peer_ips"] = toml_edit::Item::Value(toml_edit::Value::Array(peers));
    media["min_port"] = toml_edit::value(50000i64);
    media["max_port"] = toml_edit::value(50010i64);
    let temporary = path.with_extension(format!("toml.tmp-{}", std::process::id()));
    std::fs::write(&temporary, doc.to_string()).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
struct AutostartResponse {
    status: &'static str,
}

fn authorize_setup_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Tailscale setup is only available to the main window".into());
    }
    let url = window
        .url()
        .map_err(|_| "Could not verify setup window origin".to_owned())?;
    if allowed_setup_origin(&url) {
        Ok(())
    } else {
        Err("Tailscale setup is unavailable from this origin".into())
    }
}

fn allowed_setup_origin(url: &url::Url) -> bool {
    let production = url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost")));
    #[cfg(debug_assertions)]
    let development = url.scheme() == "http"
        && url.port() == Some(1420)
        && matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    #[cfg(not(debug_assertions))]
    let development = false;
    production || development
}

#[tauri::command]
async fn tailscale_setup_status(
    window: tauri::WebviewWindow,
) -> Result<tailscale_setup::SetupStatus, String> {
    authorize_setup_window(&window)?;
    tauri::async_runtime::spawn_blocking(tailscale_setup::status)
        .await
        .map_err(|_| "Tailscale status task failed".into())
}

#[tauri::command]
async fn tailscale_setup_install(
    window: tauri::WebviewWindow,
) -> Result<tailscale_setup::InstallResult, String> {
    authorize_setup_window(&window)?;
    tauri::async_runtime::spawn_blocking(tailscale_setup::install)
        .await
        .map_err(|_| "Tailscale installer task failed".to_owned())?
}

#[tauri::command]
async fn tailscale_setup_login(window: tauri::WebviewWindow) -> Result<(), String> {
    authorize_setup_window(&window)?;
    tauri::async_runtime::spawn_blocking(tailscale_setup::open_login)
        .await
        .map_err(|_| "Tailscale login task failed".to_owned())?
}

#[tauri::command]
async fn tailscale_status() -> Result<tailscale::ServeInspection, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let transaction = rc_platform_windows::TransactionGuard::acquire()
                .map_err(|error| error.to_string())?;
            let paths = lifecycle::installed_host_paths()?;
            let receipt_file = tailscale::receipt_path(&paths.config);
            let pending_file = tailscale::pending_path(&paths.config);
            let mut receipt = tailscale::load_receipt(&receipt_file)?;
            let pending = tailscale::load_pending(&pending_file)?;
            if transaction.was_abandoned() && pending.is_none() {
                return Err(
                    "A previous settings transaction was interrupted without a recovery journal"
                        .into(),
                );
            }
            match tailscale::inspect_with_receipt(&tailscale::SystemRunner, receipt.as_ref()) {
                Ok(mut value) => {
                    let https_ready = tailscale::https_ready(&tailscale::SystemRunner)?;
                    if let Some(p) = pending {
                        if p.enabled {
                            let stored = tailscale::read_public_origin(&paths.config)?;
                            let dns = tailscale::expected_dns_name(&tailscale::SystemRunner)?;
                            // The transaction guard excludes another RemoteCodex settings change.
                            // SystemRunner reaps its child before returning from a failed command.
                            if tailscale::can_discard_unapplied(
                                &p,
                                &value,
                                receipt.as_ref(),
                                stored.as_deref(),
                                &dns,
                            ) {
                                tailscale::remove_pending(&pending_file)?;
                            }
                            let candidate = p.receipt.or_else(|| {
                                (value.proxy.as_deref() == Some(p.expected_proxy.as_str())
                                    && value.dns_name.as_deref() == p.expected_dns_name.as_deref())
                                .then(|| tailscale::receipt_from(&value).ok())
                                .flatten()
                            });
                            if let Some(candidate) = candidate {
                                let verified = tailscale::inspect_with_receipt(
                                    &tailscale::SystemRunner,
                                    Some(&candidate),
                                )
                                .map_err(|error| error.to_string())?;
                                if verified.ownership == tailscale::ServeOwnership::Owned {
                                    tailscale::write_receipt(&receipt_file, &candidate)?;
                                    tailscale::update_public_origin(
                                        &paths.config,
                                        verified.public_origin.as_deref(),
                                    )?;
                                    tailscale::remove_pending(&pending_file)?;
                                    receipt = Some(candidate);
                                    value = verified;
                                }
                            }
                        } else if value.ownership == tailscale::ServeOwnership::Absent {
                            tailscale::remove_receipt(&receipt_file)?;
                            tailscale::update_public_origin(&paths.config, None)?;
                            tailscale::remove_pending(&pending_file)?;
                            receipt = None;
                        }
                    }
                    let stored = tailscale::read_public_origin(&paths.config)?;
                    let running = tauri::async_runtime::block_on(send_local(
                        serde_json::json!({"operation":"status"}),
                    ))
                    .ok()
                    .and_then(|v| {
                        v.get("public_origin")
                            .and_then(|x| x.as_str())
                            .map(str::to_owned)
                    });
                    value.restart_required =
                        tailscale::restart_required(stored.as_deref(), running.as_deref());
                    value.https_ready = Some(https_ready);
                    if !https_ready && value.ownership == tailscale::ServeOwnership::Absent {
                        value.detail = "Tailscale HTTPS approval is required".into();
                    }
                    let _ = receipt;
                    Ok(value)
                }
                Err(tailscale::ServeError::NotInstalled) => Ok(tailscale::not_installed()),
                Err(error) => Err(error.to_string()),
            }
        })
        .await
        .map_err(|error| error.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("Tailscale Serve administration is available only on Windows".into())
    }
}

#[tauri::command]
fn tailscale_https_settings(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("https://login.tailscale.com/admin/dns", None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_tailscale_serve(
    request: tailscale::ServeRequest,
) -> Result<tailscale::ServeInspection, String> {
    if !request.consent {
        return Err("Explicit consent is required".into());
    }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let transaction = rc_platform_windows::TransactionGuard::acquire()
                .map_err(|error| error.to_string())?;
            let paths = lifecycle::installed_host_paths()?;
            tailscale::preflight_config(&paths.config)?;
            let receipt_path = tailscale::receipt_path(&paths.config);
            let pending_path = tailscale::pending_path(&paths.config);
            if transaction.was_abandoned() && tailscale::load_pending(&pending_path)?.is_none() {
                return Err(
                    "A previous settings transaction was interrupted without a recovery journal"
                        .into(),
                );
            }
            if tailscale::load_pending(&pending_path)?.is_some() {
                return Err(
                    "A previous settings transaction requires recovery before a new change".into(),
                );
            }
            if request.enabled && !tailscale::https_ready(&tailscale::SystemRunner)? {
                return Err("Tailscale HTTPS approval is required".into());
            }
            let receipt = tailscale::load_receipt(&receipt_path)?;
            let runner = tailscale::SystemRunner;
            let current = tailscale::inspect_with_receipt(&runner, receipt.as_ref())
                .map_err(|error| error.to_string())?;
            if request.enabled && current.ownership != tailscale::ServeOwnership::Absent {
                return Err("Serve 443 is not exclusively unused".into());
            }
            let expected_dns_name = if request.enabled {
                Some(tailscale::expected_dns_name(&runner)?)
            } else {
                current.dns_name.clone()
            };
            if request.enabled {
                tailscale::write_pending(
                    &pending_path,
                    &tailscale::PendingServe {
                        enabled: true,
                        receipt: None,
                        expected_dns_name: expected_dns_name.clone(),
                        expected_proxy: "http://127.0.0.1:3847".into(),
                    },
                )?;
                let after = match tailscale::apply(&runner, tailscale::ServePlan::Apply, &current) {
                    Ok(value) => value,
                    Err(error) => return Err(error.to_string()),
                };
                let new_receipt = tailscale::receipt_from(&after)?;
                tailscale::write_pending(
                    &pending_path,
                    &tailscale::PendingServe {
                        enabled: true,
                        receipt: Some(new_receipt.clone()),
                        expected_dns_name: expected_dns_name.clone(),
                        expected_proxy: "http://127.0.0.1:3847".into(),
                    },
                )?;
                tailscale::write_receipt(&receipt_path, &new_receipt)?;
                tailscale::update_public_origin(&paths.config, after.public_origin.as_deref())?;
                tailscale::remove_pending(&pending_path)?;
                let stored = tailscale::read_public_origin(&paths.config)?;
                let running = tauri::async_runtime::block_on(send_local(
                    serde_json::json!({"operation":"status"}),
                ))
                .ok()
                .and_then(|v| {
                    v.get("public_origin")
                        .and_then(|x| x.as_str())
                        .map(str::to_owned)
                });
                Ok(tailscale::ServeInspection {
                    restart_required: tailscale::restart_required(
                        stored.as_deref(),
                        running.as_deref(),
                    ),
                    ..after
                })
            } else {
                if current.ownership != tailscale::ServeOwnership::Owned || receipt.is_none() {
                    return Err("Only a matching RemoteCodex Serve receipt can be removed".into());
                }
                tailscale::write_pending(
                    &pending_path,
                    &tailscale::PendingServe {
                        enabled: false,
                        receipt: receipt.clone(),
                        expected_dns_name,
                        expected_proxy: "http://127.0.0.1:3847".into(),
                    },
                )?;
                let after = match tailscale::apply(&runner, tailscale::ServePlan::Remove, &current)
                {
                    Ok(value) => value,
                    Err(error) => return Err(error.to_string()),
                };
                tailscale::remove_receipt(&receipt_path)?;
                tailscale::update_public_origin(&paths.config, None)?;
                tailscale::remove_pending(&pending_path)?;
                let stored = tailscale::read_public_origin(&paths.config)?;
                let running = tauri::async_runtime::block_on(send_local(
                    serde_json::json!({"operation":"status"}),
                ))
                .ok()
                .and_then(|v| {
                    v.get("public_origin")
                        .and_then(|x| x.as_str())
                        .map(str::to_owned)
                });
                Ok(tailscale::ServeInspection {
                    restart_required: tailscale::restart_required(
                        stored.as_deref(),
                        running.as_deref(),
                    ),
                    ..after
                })
            }
        })
        .await
        .map_err(|error| error.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Err("Tailscale Serve administration is available only on Windows".into())
    }
}

#[cfg(windows)]
fn autostart_response(
    status: rc_platform_windows::autostart::AutostartStatus,
) -> AutostartResponse {
    use rc_platform_windows::autostart::AutostartStatus;
    AutostartResponse {
        status: match status {
            AutostartStatus::Disabled => "disabled",
            AutostartStatus::Enabled => "enabled",
            AutostartStatus::Conflict => "conflict",
        },
    }
}

#[tauri::command]
fn autostart_status() -> Result<AutostartResponse, String> {
    #[cfg(windows)]
    {
        let paths = lifecycle::installed_host_paths()?;
        rc_platform_windows::autostart::status(&paths.agent, &paths.config).map(autostart_response)
    }
    #[cfg(not(windows))]
    {
        Err("Autostart is available only on Windows".into())
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<AutostartResponse, String> {
    #[cfg(windows)]
    {
        let paths = lifecycle::installed_host_paths()?;
        rc_platform_windows::autostart::set_enabled(&paths.agent, &paths.config, enabled)
            .map(autostart_response)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("Autostart is available only on Windows".into())
    }
}

#[cfg(windows)]
#[derive(Debug)]
enum LocalIpcError {
    Missing,
    Other(String),
}

#[cfg(windows)]
async fn send_local(payload: serde_json::Value) -> Result<serde_json::Value, LocalIpcError> {
    use std::os::windows::io::AsRawHandle;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };

    let operation = async move {
        let name = rc_platform_windows::pipe_name()
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        let mut pipe = ClientOptions::new().open(name).map_err(|error| {
            if error.raw_os_error() == Some(2) {
                LocalIpcError::Missing
            } else {
                LocalIpcError::Other(error.to_string())
            }
        })?;
        rc_platform_windows::verify_pipe_peer(pipe.as_raw_handle().cast(), false)
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        let data = serde_json::to_vec(&payload)
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        pipe.write_u32(data.len() as u32)
            .await
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        pipe.write_all(&data)
            .await
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        let size = pipe
            .read_u32()
            .await
            .map_err(|error| LocalIpcError::Other(error.to_string()))? as usize;
        if size > 128 * 1024 {
            return Err(LocalIpcError::Other("Oversized IPC response".to_owned()));
        }
        let mut bytes = vec![0; size];
        pipe.read_exact(&mut bytes)
            .await
            .map_err(|error| LocalIpcError::Other(error.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|error| LocalIpcError::Other(error.to_string()))
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), operation)
        .await
        .map_err(|_| LocalIpcError::Other("Agent IPC timed out".to_owned()))?
}
#[tauri::command]
async fn local_admin(
    operation: String,
    request: Option<MediaApproval>,
) -> Result<serde_json::Value, String> {
    if ![
        "status",
        "pair_owner",
        "pair_reader",
        "pair_gui",
        "media_sources",
        "media_approve",
        "media_clear",
        "block_remote",
        "resume_remote",
    ]
    .contains(&operation.as_str())
    {
        return Err("Unsupported local operation".into());
    }
    let payload = if operation == "media_approve" {
        let r = request.ok_or_else(|| "Missing source approval".to_string())?;
        if !["window", "monitor"].contains(&r.kind.as_str())
            || r.handle.len() > 20
            || r.handle.parse::<u64>().ok().is_none_or(|n| n == 0)
        {
            return Err("Invalid native source".into());
        }
        serde_json::json!({"operation":operation,"kind":r.kind,"handle":r.handle,"control":r.control})
    } else {
        if request.is_some() {
            return Err("Unexpected local arguments".into());
        }
        serde_json::json!({"operation":operation})
    };
    #[cfg(windows)]
    {
        send_local(payload).await.map_err(|error| match error {
            LocalIpcError::Missing => "Agent is not running".to_owned(),
            LocalIpcError::Other(message) => message,
        })
    }
    #[cfg(not(windows))]
    {
        Err("Company host administration requires Windows".into())
    }
}

#[tauri::command]
async fn media_config_status() -> Result<MediaConfigStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let transaction = rc_platform_windows::TransactionGuard::acquire()
                .map_err(|error| error.to_string())?;
            if transaction.was_abandoned() {
                return Err(
                    "A previous settings transaction requires recovery before media changes".into(),
                );
            }
            let paths = lifecycle::installed_host_paths()?;
            tailscale::preflight_config(&paths.config)?;
            let document = std::fs::read_to_string(&paths.config)
                .map_err(|error| error.to_string())?
                .parse::<toml_edit::DocumentMut>()
                .map_err(|error| format!("Invalid Agent configuration: {error}"))?;
            let stored = media_from_document(&document)?;
            let live = match tauri::async_runtime::block_on(send_local(
                serde_json::json!({"operation":"status"}),
            )) {
                Ok(value) => Some(
                    serde_json::from_value::<MediaSettings>(
                        value
                            .get("media")
                            .cloned()
                            .ok_or_else(|| "Agent status omitted media settings".to_owned())?,
                    )
                    .map_err(|error| format!("Agent media status is invalid: {error}"))?,
                ),
                Err(LocalIpcError::Missing) => None,
                Err(LocalIpcError::Other(error)) => return Err(error),
            };
            let restart_required = live.as_ref().is_some_and(|value| value != &stored);
            Ok(MediaConfigStatus {
                stored,
                agent_running: live.is_some(),
                live,
                restart_required,
            })
        })
        .await
        .map_err(|error| error.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("Company host administration requires Windows".into())
    }
}

#[tauri::command]
async fn set_media_config(request: MediaConfigRequest) -> Result<MediaConfigStatus, String> {
    if !request.consent {
        return Err("Explicit consent is required".into());
    }
    let settings = MediaSettings {
        enabled: request.enabled,
        allow_control: request.allow_control,
        tailnet_ip: request.tailnet_ip,
        allowed_peer_ips: request.allowed_peer_ips,
    };
    validate_media_settings(&settings)?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let transaction = rc_platform_windows::TransactionGuard::acquire()
                .map_err(|error| error.to_string())?;
            if transaction.was_abandoned() {
                return Err(
                    "A previous settings transaction requires recovery before media changes".into(),
                );
            }
            let paths = lifecycle::installed_host_paths()?;
            tailscale::preflight_config(&paths.config)?;
            set_media_document(&paths.config, &settings)?;
            let live = match tauri::async_runtime::block_on(send_local(
                serde_json::json!({"operation":"status"}),
            )) {
                Ok(value) => Some(
                    serde_json::from_value::<MediaSettings>(
                        value
                            .get("media")
                            .cloned()
                            .ok_or_else(|| "Agent status omitted media settings".to_owned())?,
                    )
                    .map_err(|error| format!("Agent media status is invalid: {error}"))?,
                ),
                Err(LocalIpcError::Missing) => None,
                Err(LocalIpcError::Other(error)) => return Err(error),
            };
            Ok(MediaConfigStatus {
                restart_required: live.as_ref().is_some_and(|value| value != &settings),
                agent_running: live.is_some(),
                stored: settings,
                live,
            })
        })
        .await
        .map_err(|error| error.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("Company host administration requires Windows".into())
    }
}

#[tauri::command]
async fn ensure_host() -> Result<lifecycle::HostAttach, String> {
    #[cfg(windows)]
    {
        let status = serde_json::json!({"operation":"status"});
        match send_local(status.clone()).await {
            Ok(_) => return Ok(lifecycle::HostAttach::Attached),
            Err(LocalIpcError::Missing) => {}
            Err(LocalIpcError::Other(message)) => return Err(message),
        }
        lifecycle::start_detached_host()?;
        for _ in 0..30 {
            match send_local(status.clone()).await {
                Ok(_) => return Ok(lifecycle::HostAttach::Started),
                Err(LocalIpcError::Missing) => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(LocalIpcError::Other(message)) => return Err(message),
            }
        }
        Err("Agent started, but its SID-verified IPC did not become ready".to_owned())
    }
    #[cfg(not(windows))]
    {
        Err("Hosting is available only on Windows".to_owned())
    }
}
#[tauri::command]
fn open_preview(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let u = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if u.scheme() != "https"
        || !u.host_str().is_some_and(|h| h.ends_with(".ts.net"))
        || !u.port().is_some_and(|p| (8444..=8451).contains(&p))
        || u.path() != "/_rc/bootstrap"
        || u.query().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_none_or(|f| {
            f.len() > 128
                || !f
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
    {
        return Err("Not an approved preview URL shape".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

fn reject_unresolved_uninstall_pending(
    pending: Option<&tailscale::PendingServe>,
) -> Result<(), String> {
    if pending.is_some() {
        Err("A pending Serve transaction requires recovery before uninstall cleanup".into())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn uninstall_cleanup() -> Result<(), String> {
    let transaction =
        rc_platform_windows::TransactionGuard::acquire().map_err(|error| error.to_string())?;
    let paths = lifecycle::installed_host_paths()?;
    let autostart = rc_platform_windows::autostart::status(&paths.agent, &paths.config)?;
    if autostart == rc_platform_windows::autostart::AutostartStatus::Conflict {
        return Err("A foreign autostart value is present".into());
    }
    let receipt_path = tailscale::receipt_path(&paths.config);
    let pending_path = tailscale::pending_path(&paths.config);
    if transaction.was_abandoned() && tailscale::load_pending(&pending_path)?.is_none() {
        return Err(
            "A previous settings transaction was interrupted without a recovery journal".into(),
        );
    }
    let pending = tailscale::load_pending(&pending_path)?;
    reject_unresolved_uninstall_pending(pending.as_ref())?;
    let receipt = tailscale::load_receipt(&receipt_path)?;
    let runner = tailscale::SystemRunner;
    let current = match tailscale::inspect_with_receipt(&runner, receipt.as_ref()) {
        Ok(value) => Some(value),
        Err(tailscale::ServeError::NotInstalled) if receipt.is_none() => None,
        Err(error) => return Err(error.to_string()),
    };
    if let Some(current) = current {
        if receipt.is_some() {
            tailscale::preflight_config(&paths.config)?;
            if current.ownership == tailscale::ServeOwnership::Owned {
                let matching = receipt.clone().ok_or("Missing Serve receipt")?;
                tailscale::write_pending(
                    &pending_path,
                    &tailscale::PendingServe {
                        enabled: false,
                        receipt: Some(matching.clone()),
                        expected_dns_name: Some(matching.dns_name.clone()),
                        expected_proxy: matching.proxy.clone(),
                    },
                )?;
                let after = tailscale::apply(&runner, tailscale::ServePlan::Remove, &current)
                    .map_err(|error| error.to_string())?;
                if after.ownership != tailscale::ServeOwnership::Absent {
                    return Err("Owned Serve rule did not clear".into());
                }
                tailscale::remove_receipt(&receipt_path)?;
                tailscale::update_public_origin(&paths.config, None)?;
                tailscale::remove_pending(&pending_path)?;
            } else if current.ownership == tailscale::ServeOwnership::Absent {
                tailscale::remove_receipt(&receipt_path)?;
                tailscale::update_public_origin(&paths.config, None)?;
                tailscale::remove_pending(&pending_path)?;
            } else {
                return Err("Serve ownership could not be verified".into());
            }
        }
    } else if receipt.is_some() {
        return Err("Serve ownership could not be verified".into());
    }
    if autostart == rc_platform_windows::autostart::AutostartStatus::Enabled {
        rc_platform_windows::autostart::set_enabled(&paths.agent, &paths.config, false)?;
    }
    Ok(())
}

fn main() {
    #[cfg(windows)]
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--uninstall-cleanup")
    {
        std::process::exit(if uninstall_cleanup().is_ok() { 0 } else { 4 });
    }
    // Only explicit host onboarding may start the detached Agent. UI closure never terminates it.
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            local_admin,
            ensure_host,
            autostart_status,
            set_autostart,
            media_config_status,
            set_media_config,
            tailscale_setup_status,
            tailscale_setup_install,
            tailscale_setup_login,
            tailscale_status,
            set_tailscale_serve,
            tailscale_https_settings,
            open_preview
        ])
        .run(tauri::generate_context!())
        .expect("RemoteCodex UI could not start");
}

#[cfg(test)]
mod media_config_tests {
    use super::*;

    #[test]
    fn setup_origin_accepts_production_localhost() {
        assert!(allowed_setup_origin(
            &url::Url::parse("tauri://localhost").unwrap()
        ));
        assert!(allowed_setup_origin(
            &url::Url::parse("http://tauri.localhost").unwrap()
        ));
    }

    #[test]
    fn setup_origin_rejects_remote_and_preview_origins() {
        for value in [
            "https://evil.example",
            "http://127.0.0.1:1421",
            "tauri://evil",
            "http://tauri.localhost:9333",
            "tauri://localhost:9333",
            "http://user@tauri.localhost",
        ] {
            assert!(!allowed_setup_origin(&url::Url::parse(value).unwrap()));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn setup_origin_allows_dev_loopback_only_in_debug() {
        assert!(allowed_setup_origin(
            &url::Url::parse("http://127.0.0.1:1420").unwrap()
        ));
        assert!(!allowed_setup_origin(
            &url::Url::parse("http://192.168.1.10:1420").unwrap()
        ));
    }

    #[test]
    fn media_validation_rejects_non_tailnet_peers() {
        let settings = MediaSettings {
            enabled: true,
            allow_control: false,
            tailnet_ip: Some("192.168.1.10".into()),
            allowed_peer_ips: vec!["100.100.100.20".into()],
        };
        assert!(validate_media_settings(&settings).is_err());
    }

    #[test]
    fn media_document_preserves_profile_and_other_tables() {
        let root = std::env::temp_dir().join(format!("rc-media-config-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("agent.toml");
        std::fs::write(
            &path,
            "bind = \"127.0.0.1:3847\"\n[media]\nprofile = \"preserve\"\n[other]\nkeep = true\n",
        )
        .unwrap();
        let settings = MediaSettings {
            enabled: true,
            allow_control: true,
            tailnet_ip: Some("100.100.100.10".into()),
            allowed_peer_ips: vec!["100.100.100.20".into()],
        };
        set_media_document(&path, &settings).unwrap();
        let doc = std::fs::read_to_string(&path)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(doc["media"]["profile"].as_str(), Some("preserve"));
        assert_eq!(doc["media"]["min_port"].as_integer(), Some(50000));
        assert_eq!(doc["media"]["max_port"].as_integer(), Some(50010));
        assert_eq!(doc["other"]["keep"].as_bool(), Some(true));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn uninstall_cleanup_refuses_unresolved_serve_transaction() {
        let pending = tailscale::PendingServe {
            enabled: true,
            receipt: None,
            expected_dns_name: Some("fixture.ts.net".into()),
            expected_proxy: "http://127.0.0.1:3000".into(),
        };
        assert!(reject_unresolved_uninstall_pending(Some(&pending)).is_err());
        assert!(reject_unresolved_uninstall_pending(None).is_ok());
    }
}
