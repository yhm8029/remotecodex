use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const BACKEND: &str = "http://127.0.0.1:3847";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServeOwnership {
    Absent,
    Owned,
    Conflict,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliState {
    Installed,
    NotInstalled,
}
#[derive(Debug, Clone, Serialize)]
pub struct ServeInspection {
    pub installed: bool,
    pub cli: CliState,
    pub ownership: ServeOwnership,
    pub detail: String,
    pub dns_name: Option<String>,
    pub proxy: Option<String>,
    pub public_origin: Option<String>,
    pub restart_required: bool,
    pub https_ready: Option<bool>,
    pub preview_ports: BTreeMap<u16, u16>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServePlan {
    Apply,
    Remove,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeError {
    NotInstalled,
    CommandFailed,
    Failed(String),
}
impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("Tailscale CLI is not installed"),
            Self::CommandFailed => f.write_str("Tailscale Serve command failed"),
            Self::Failed(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for ServeError {}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServeReceipt {
    pub dns_name: String,
    pub proxy: String,
    pub https_port: u16,
    pub public_origin: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingServe {
    pub enabled: bool,
    pub receipt: Option<ServeReceipt>,
    #[serde(default)]
    pub expected_dns_name: Option<String>,
    #[serde(default = "default_backend")]
    pub expected_proxy: String,
}
fn default_backend() -> String {
    BACKEND.to_owned()
}
#[derive(Debug, Clone, Deserialize)]
pub struct ServeRequest {
    pub enabled: bool,
    pub consent: bool,
}
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
pub trait CliRunner {
    fn run(&self, args: &[&str]) -> Result<CommandOutput, String>;
}
pub struct SystemRunner;
impl CliRunner for SystemRunner {
    fn run(&self, args: &[&str]) -> Result<CommandOutput, String> {
        let executable = crate::tailscale_setup::resolve_cli()
            .ok_or_else(|| "Tailscale executable not found".to_owned())?;
        let mut c = Command::new(executable);
        c.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let mut child = c.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Tailscale executable not found".to_owned()
            } else {
                "Could not start Tailscale CLI".to_owned()
            }
        })?;
        let stdout = child.stdout.take().ok_or("Missing Tailscale stdout")?;
        let stderr = child.stderr.take().ok_or("Missing Tailscale stderr")?;
        let out = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stdout.take(256 * 1024).read_to_end(&mut b);
            b
        });
        let err = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stderr.take(64 * 1024).read_to_end(&mut b);
            b
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if start.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out.join();
                    let _ = err.join();
                    return Err("Tailscale CLI timed out".into());
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out.join();
                    let _ = err.join();
                    return Err("Could not wait for Tailscale CLI".into());
                }
            }
        };
        Ok(CommandOutput {
            success: status.success(),
            stdout: String::from_utf8(out.join().unwrap_or_default())
                .map_err(|_| "Tailscale output was not UTF-8".to_owned())?,
            stderr: String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned(),
        })
    }
}
fn valid_dns_name(name: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".ts.net") else {
        return false;
    };
    !prefix.is_empty()
        && prefix.split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !p.starts_with('-')
                && !p.ends_with('-')
        })
}
fn public_origin(name: &str) -> Option<String> {
    valid_dns_name(name).then(|| format!("https://{name}"))
}
pub fn preview_ports() -> BTreeMap<u16, u16> {
    (8444..=8451).map(|p| (p, p + 5400)).collect()
}
pub fn not_installed() -> ServeInspection {
    ServeInspection {
        installed: false,
        cli: CliState::NotInstalled,
        ownership: ServeOwnership::Absent,
        detail: "Tailscale CLI is not installed".into(),
        dns_name: None,
        proxy: None,
        public_origin: None,
        restart_required: false,
        https_ready: None,
        preview_ports: preview_ports(),
    }
}
#[derive(Debug, Clone)]
struct ObservedServe {
    dns_name: Option<String>,
    proxy: Option<String>,
    occupied: bool,
    funnel_enabled: bool,
}
fn parse_status(v: &serde_json::Value) -> Result<ObservedServe, ServeError> {
    let root = v
        .as_object()
        .ok_or_else(|| ServeError::Failed("Tailscale Serve status must be a JSON object".into()))?;
    for key in ["TCP", "Web", "Foreground"] {
        if let Some(x) = root.get(key) {
            if !x.is_object() {
                return Err(ServeError::Failed(format!(
                    "Tailscale Serve field {key} has an invalid type"
                )));
            }
        }
    }
    let funnel_enabled = if let Some(x) = root.get("AllowFunnel") {
        let Some(o) = x.as_object() else {
            return Err(ServeError::Failed(
                "Tailscale AllowFunnel has an invalid type".into(),
            ));
        };
        let mut enabled = false;
        for value in o.values() {
            let Some(v) = value.as_bool() else {
                return Err(ServeError::Failed(
                    "Tailscale AllowFunnel value has an invalid type".into(),
                ));
            };
            enabled |= v;
        }
        enabled
    } else {
        false
    };
    let tcp = root.get("TCP").and_then(|x| x.as_object());
    let web = root.get("Web").and_then(|x| x.as_object());
    let tcp443 = tcp.and_then(|m| m.get("443"));
    let (tcp443_occupied, tcp443_forward) = if let Some(x) = tcp443 {
        let o = x
            .as_object()
            .ok_or_else(|| ServeError::Failed("Tailscale TCP 443 has an invalid type".into()))?;
        let https = match o.get("HTTPS") {
            Some(h) => Some(h.as_bool().ok_or_else(|| {
                ServeError::Failed("Tailscale TCP 443 HTTPS has an invalid type".into())
            })?),
            None => None,
        };
        let forwarding = match o.get("TCPForward") {
            Some(f) => {
                if !f.is_string() {
                    return Err(ServeError::Failed(
                        "Tailscale TCP 443 TCPForward has an invalid type".into(),
                    ));
                }
                true
            }
            None => false,
        };
        (https == Some(true) || forwarding, forwarding)
    } else {
        (false, false)
    };
    let foreground_occupied = if let Some(foreground) = root.get("Foreground") {
        let Some(sessions) = foreground.as_object() else {
            return Err(ServeError::Failed(
                "Tailscale Foreground has an invalid type".into(),
            ));
        };
        for config in sessions.values() {
            if !config.is_object() {
                return Err(ServeError::Failed(
                    "Tailscale Foreground session has an invalid type".into(),
                ));
            }
        }
        !sessions.is_empty()
    } else {
        false
    };
    let Some(web) = web else {
        return Ok(ObservedServe {
            dns_name: None,
            proxy: None,
            occupied: tcp443_occupied || foreground_occupied || funnel_enabled,
            funnel_enabled: funnel_enabled || tcp443_forward || foreground_occupied,
        });
    };
    let mut rules = Vec::new();
    for (host, rule) in web {
        if !host.ends_with(":443") {
            continue;
        }
        let name = host.trim_end_matches(":443");
        if !valid_dns_name(name) {
            return Err(ServeError::Failed(
                "Tailscale Serve returned an invalid DNS name".into(),
            ));
        }
        let ro = rule
            .as_object()
            .ok_or_else(|| ServeError::Failed("Tailscale Web rule has an invalid type".into()))?;
        let handlers = ro
            .get("Handlers")
            .and_then(|x| x.as_object())
            .ok_or_else(|| {
                ServeError::Failed("Tailscale Web Handlers has an invalid type".into())
            })?;
        if handlers.len() != 1 || !handlers.contains_key("/") {
            return Err(ServeError::Failed(
                "Tailscale Web Handlers must contain only the root handler".into(),
            ));
        }
        let root = handlers
            .get("/")
            .and_then(|x| x.as_object())
            .ok_or_else(|| {
                ServeError::Failed("Tailscale Web root handler has an invalid type".into())
            })?;
        if root.len() != 1 || !root.contains_key("Proxy") {
            return Err(ServeError::Failed(
                "Tailscale Web root handler must contain only Proxy".into(),
            ));
        }
        let proxy = root
            .get("Proxy")
            .and_then(|x| x.as_str())
            .ok_or_else(|| ServeError::Failed("Tailscale Web Proxy has an invalid type".into()))?;
        rules.push((name.to_owned(), proxy.to_owned()));
    }
    if rules.len() > 1 || (!rules.is_empty() && !tcp443_occupied) {
        Err(ServeError::Failed(
            "Tailscale Serve 443 configuration is mixed or incomplete".into(),
        ))
    } else if let Some((dns_name, proxy)) = rules.into_iter().next() {
        Ok(ObservedServe {
            dns_name: Some(dns_name),
            proxy: Some(proxy),
            occupied: true,
            funnel_enabled: funnel_enabled || tcp443_forward || foreground_occupied,
        })
    } else {
        Ok(ObservedServe {
            dns_name: None,
            proxy: None,
            occupied: tcp443_occupied || foreground_occupied || funnel_enabled,
            funnel_enabled: funnel_enabled || tcp443_forward || foreground_occupied,
        })
    }
}
fn inspection_from(
    v: &serde_json::Value,
    receipt: Option<&ServeReceipt>,
) -> Result<ServeInspection, ServeError> {
    let observed = parse_status(v)?;
    let mut i = ServeInspection {
        installed: true,
        cli: CliState::Installed,
        ownership: ServeOwnership::Absent,
        detail: "HTTPS 443 is unused".into(),
        dns_name: None,
        proxy: None,
        public_origin: None,
        restart_required: false,
        https_ready: None,
        preview_ports: preview_ports(),
    };
    if !observed.occupied {
        return Ok(i);
    }
    let Some(dns) = observed.dns_name.clone() else {
        i.ownership = ServeOwnership::Conflict;
        i.detail = "HTTPS 443 contains an existing or foreign Serve rule".into();
        return Ok(i);
    };
    let Some(proxy) = observed.proxy.clone() else {
        i.ownership = ServeOwnership::Conflict;
        i.detail = "HTTPS 443 contains an existing or foreign Serve rule".into();
        return Ok(i);
    };
    i.dns_name = Some(dns.clone());
    i.proxy = Some(proxy.clone());
    i.public_origin = public_origin(&dns);
    let owned = !observed.funnel_enabled
        && receipt.is_some_and(|r| {
            r.https_port == 443
                && r.dns_name == dns
                && r.proxy == proxy
                && r.proxy == BACKEND
                && r.public_origin == i.public_origin.clone().unwrap_or_default()
        });
    i.ownership = if owned {
        ServeOwnership::Owned
    } else {
        ServeOwnership::Conflict
    };
    i.detail = if owned {
        "RemoteCodex owns HTTPS 443"
    } else {
        "HTTPS 443 contains an existing or foreign Serve rule"
    }
    .into();
    Ok(i)
}
pub fn inspect_with_receipt(
    r: &impl CliRunner,
    receipt: Option<&ServeReceipt>,
) -> Result<ServeInspection, ServeError> {
    match r.run(&["version"]) {
        Ok(v) if !v.success => {
            return Err(ServeError::Failed(
                "Tailscale CLI version check failed".into(),
            ))
        }
        Ok(_) => {}
        Err(e) if e.to_ascii_lowercase().contains("not found") => {
            return Err(ServeError::NotInstalled)
        }
        Err(e) => return Err(ServeError::Failed(e)),
    }
    let o = r.run(&["serve", "status", "--json"]).map_err(|e| {
        if e.to_ascii_lowercase().contains("not found") {
            ServeError::NotInstalled
        } else {
            ServeError::Failed(e)
        }
    })?;
    if !o.success {
        return Err(ServeError::Failed("Tailscale Serve status failed".into()));
    }
    let v = serde_json::from_str(&o.stdout)
        .map_err(|_| ServeError::Failed("Tailscale returned invalid Serve status JSON".into()))?;
    inspection_from(&v, receipt)
}
pub fn inspect(r: &impl CliRunner) -> Result<ServeInspection, ServeError> {
    inspect_with_receipt(r, None)
}
pub fn https_ready(r: &impl CliRunner) -> Result<bool, String> {
    let out = r.run(&["status", "--json"])?;
    if !out.success {
        return Err("tailscale status command failed".to_string());
    }
    let v: serde_json::Value = serde_json::from_str(&out.stdout)
        .map_err(|_| "tailscale status output is not valid JSON".to_string())?;
    let obj = v
        .as_object()
        .ok_or("tailscale status JSON is not an object")?;

    let backend_state = obj.get("BackendState").ok_or("missing BackendState")?;
    if backend_state.as_str() != Some("Running") {
        return Err("BackendState is not Running".to_string());
    }

    let self_node = obj.get("Self").ok_or("missing Self node")?;
    let self_obj = self_node.as_object().ok_or("Self is not an object")?;

    let dns_name = self_obj.get("DNSName").ok_or("missing Self.DNSName")?;
    let dns_str = dns_name.as_str().ok_or("Self.DNSName is not a string")?;
    let stripped = dns_str.strip_suffix('.').unwrap_or(dns_str);
    if !valid_dns_name(stripped) {
        return Err("invalid DNS name".to_string());
    }

    let mut map_present = false;
    let mut map_has_https = false;
    if let Some(cap_map) = self_obj.get("CapMap") {
        if cap_map.is_null() {
            // null counts as not present
        } else {
            let map = cap_map.as_object().ok_or("Self.CapMap is not an object")?;
            map_present = true;
            if let Some(https_val) = map.get("https") {
                if https_val.is_null() || https_val.is_array() {
                    map_has_https = true;
                } else {
                    return Err("Self.CapMap.https has wrong type".to_string());
                }
            }
        }
    }

    let mut arr_present = false;
    let mut arr_has_https = false;
    if let Some(caps) = self_obj.get("Capabilities") {
        if caps.is_null() {
            // null counts as not present
        } else {
            let arr = caps.as_array().ok_or("Self.Capabilities is not an array")?;
            arr_present = true;
            for item in arr {
                if !item.is_string() {
                    return Err("Self.Capabilities contains non-string item".to_string());
                }
                if item.as_str() == Some("https") {
                    arr_has_https = true;
                }
            }
        }
    }

    if map_has_https || arr_has_https {
        return Ok(true);
    }
    if map_present || arr_present {
        return Ok(false);
    }
    Err("Tailscale HTTPS capability status is unavailable".to_string())
}

pub fn can_discard_unapplied(
    p: &PendingServe,
    current: &ServeInspection,
    receipt: Option<&ServeReceipt>,
    stored_origin: Option<&str>,
    dns: &str,
) -> bool {
    p.enabled
        && p.receipt.is_none()
        && receipt.is_none()
        && stored_origin.is_none()
        && current.ownership == ServeOwnership::Absent
        && p.expected_proxy == BACKEND
        && valid_dns_name(dns)
        && p.expected_dns_name.as_deref() == Some(dns)
}

pub fn expected_dns_name(r: &impl CliRunner) -> Result<String, String> {
    let output = r.run(&["status", "--json"])?;
    if !output.success {
        return Err("Tailscale status query failed".into());
    }
    let value: serde_json::Value = serde_json::from_str(&output.stdout)
        .map_err(|_| "Tailscale returned invalid status JSON".to_owned())?;
    let raw = value
        .get("Self")
        .and_then(|v| v.get("DNSName"))
        .and_then(|v| v.as_str())
        .or_else(|| value.get("DNSName").and_then(|v| v.as_str()))
        .ok_or_else(|| "Tailscale status has no DNS name".to_owned())?;
    let name = raw.strip_suffix('.').unwrap_or(raw);
    if !valid_dns_name(name) {
        return Err("Tailscale status has an invalid DNS name".into());
    }
    Ok(name.to_owned())
}
pub fn receipt_path(config: &Path) -> PathBuf {
    config.with_file_name("tailscale-serve-receipt.json")
}
pub fn pending_path(config: &Path) -> PathBuf {
    config.with_file_name("tailscale-serve-pending.json")
}
pub fn preflight_config(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Agent configuration is missing".into());
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map(|_| ())
        .map_err(|_| "Agent configuration is not writable".into())
}
pub fn read_public_origin(path: &Path) -> Result<Option<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Invalid Agent configuration: {e}"))?;
    let Some(item) = doc.get("public_origin") else {
        return Ok(None);
    };
    let Some(value) = item.as_str() else {
        return Err("Agent public_origin has an invalid type".into());
    };
    let Some(host) = value.strip_prefix("https://") else {
        return Err("Agent public_origin is invalid".into());
    };
    if public_origin(host).as_deref() != Some(value) {
        return Err("Agent public_origin is invalid".into());
    }
    Ok(Some(value.to_owned()))
}
pub fn restart_required(stored: Option<&str>, running: Option<&str>) -> bool {
    stored != running
}
pub fn load_receipt(path: &Path) -> Result<Option<ServeReceipt>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| format!("Invalid RemoteCodex Serve receipt: {e}"))
}
pub fn write_receipt(path: &Path, r: &ServeReceipt) -> Result<(), String> {
    let b = serde_json::to_vec_pretty(r).map_err(|e| e.to_string())?;
    let t = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&t, b).map_err(|e| e.to_string())?;
    fs::rename(t, path).map_err(|e| e.to_string())
}
pub fn load_pending(path: &Path) -> Result<Option<PendingServe>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| format!("Invalid RemoteCodex Serve transaction journal: {e}"))
}
pub fn write_pending(path: &Path, pending: &PendingServe) -> Result<(), String> {
    let b = serde_json::to_vec_pretty(pending).map_err(|e| e.to_string())?;
    let t = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&t, b).map_err(|e| e.to_string())?;
    fs::rename(t, path).map_err(|e| e.to_string())
}
pub fn remove_pending(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
pub fn remove_receipt(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
pub fn receipt_from(i: &ServeInspection) -> Result<ServeReceipt, String> {
    Ok(ServeReceipt {
        dns_name: i.dns_name.clone().ok_or("Serve status has no DNS name")?,
        proxy: BACKEND.into(),
        https_port: 443,
        public_origin: i
            .public_origin
            .clone()
            .ok_or("Serve status has no validated public origin")?,
    })
}
pub fn apply(
    r: &impl CliRunner,
    plan: ServePlan,
    current: &ServeInspection,
) -> Result<ServeInspection, ServeError> {
    let args: Vec<&str> = match (plan, current.ownership) {
        (ServePlan::Apply, ServeOwnership::Absent) => {
            vec!["serve", "--bg", "--yes", "--https=443", BACKEND]
        }
        (ServePlan::Remove, ServeOwnership::Owned) => vec!["serve", "--yes", "--https=443", "off"],
        _ => {
            return Err(ServeError::Failed(
                "Serve configuration changed or is not exclusively owned by RemoteCodex".into(),
            ))
        }
    };
    let o = r.run(&args).map_err(ServeError::Failed)?;
    if !o.success {
        return Err(ServeError::CommandFailed);
    }
    let a = inspect_with_receipt(r, None)?;
    if plan == ServePlan::Apply {
        let candidate = receipt_from(&a).map_err(ServeError::Failed)?;
        let verified = inspect_with_receipt(r, Some(&candidate))?;
        if verified.ownership == ServeOwnership::Owned {
            return Ok(verified);
        }
        return Err(ServeError::Failed(
            "Tailscale Serve postcondition was not an exclusive RemoteCodex rule".into(),
        ));
    }
    if plan == ServePlan::Remove && a.ownership == ServeOwnership::Absent {
        return Ok(a);
    }
    Err(ServeError::Failed(
        "Tailscale Serve postcondition was not met".into(),
    ))
}
pub fn update_public_origin(path: &Path, origin: Option<&str>) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Invalid Agent configuration: {e}"))?;
    if let Some(o) = origin {
        let Some(host) = o.strip_prefix("https://") else {
            return Err("Invalid public_origin".into());
        };
        if public_origin(host) != Some(o.to_owned()) {
            return Err("Invalid public_origin".into());
        }
        doc["public_origin"] = toml_edit::value(o);
    } else {
        doc.remove("public_origin");
    }
    let t = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&t, doc.to_string()).map_err(|e| e.to_string())?;
    fs::rename(t, path).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Mock {
        outputs: Mutex<Vec<Result<CommandOutput, String>>>,
        calls: Mutex<Vec<Vec<String>>>,
    }
    impl Mock {
        fn new(o: Vec<Result<CommandOutput, String>>) -> Self {
            Self {
                outputs: Mutex::new(o),
                calls: Default::default(),
            }
        }
    }
    impl CliRunner for Mock {
        fn run(&self, a: &[&str]) -> Result<CommandOutput, String> {
            self.calls
                .lock()
                .unwrap()
                .push(a.iter().map(|x| x.to_string()).collect());
            self.outputs.lock().unwrap().remove(0)
        }
    }
    fn out(s: &str) -> Result<CommandOutput, String> {
        Ok(CommandOutput {
            success: true,
            stdout: s.into(),
            stderr: String::new(),
        })
    }
    fn status(h: &str, p: &str) -> String {
        format!(
            r#"{{"TCP":{{"443":{{"HTTPS":true}}}},"Web":{{"{h}:443":{{"Handlers":{{"/":{{"Proxy":"{p}"}}}}}}}}}}"#
        )
    }
    fn rec(h: &str) -> ServeReceipt {
        ServeReceipt {
            dns_name: h.into(),
            proxy: BACKEND.into(),
            https_port: 443,
            public_origin: format!("https://{h}"),
        }
    }
    #[test]
    fn exact_proxy_without_receipt_is_foreign() {
        let i = inspection_from(
            &serde_json::from_str(&status("host.ts.net", BACKEND)).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(i.ownership, ServeOwnership::Conflict)
    }
    #[test]
    fn matching_receipt_owned() {
        let v = serde_json::from_str(&status("host.ts.net", BACKEND)).unwrap();
        assert_eq!(
            inspection_from(&v, Some(&rec("host.ts.net")))
                .unwrap()
                .ownership,
            ServeOwnership::Owned
        )
    }
    #[test]
    fn occupied_without_web_is_conflict() {
        let v = serde_json::json!({"TCP":{"443":{"HTTPS":true}}});
        assert_eq!(
            inspection_from(&v, None).unwrap().ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn tcp_forward_without_web_is_conflict() {
        let v = serde_json::json!({"TCP":{"443":{"TCPForward":"localhost:22"}}});
        assert_eq!(
            inspection_from(&v, None).unwrap().ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn https_false_with_tcp_forward_is_conflict() {
        let v = serde_json::json!({
            "TCP":{"443":{"HTTPS":false,"TCPForward":"localhost:22"}}
        });
        assert_eq!(
            inspection_from(&v, None).unwrap().ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn foreground_443_is_conflict() {
        let v = serde_json::json!({
            "Foreground":{"session":{"TCP":{"443":{"HTTPS":true}}}}
        });
        assert_eq!(
            inspection_from(&v, None).unwrap().ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn nested_foreground_session_is_conflict() {
        let v = serde_json::json!({
            "Foreground":{"session":{"TCP":{"443":{"HTTPS":true}},"Web":{}}}
        });
        assert_eq!(
            inspection_from(&v, None).unwrap().ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn foreground_443_blocks_matching_receipt() {
        let v = serde_json::json!({
            "Foreground":{"session":{
                "TCP":{"443":{"HTTPS":true}},
                "Web":{"host.ts.net:443":{"Handlers":{"/":{"Proxy":BACKEND}}}}
            }}
        });
        assert_eq!(
            inspection_from(&v, Some(&rec("host.ts.net")))
                .unwrap()
                .ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn funnel_enabled_is_conflict_even_with_receipt() {
        let v = serde_json::json!({
            "TCP":{"443":{"HTTPS":true}},
            "Web":{"host.ts.net:443":{"Handlers":{"/":{"Proxy":BACKEND}}}},
            "AllowFunnel":{"host.ts.net:443":true}
        });
        assert_eq!(
            inspection_from(&v, Some(&rec("host.ts.net")))
                .unwrap()
                .ownership,
            ServeOwnership::Conflict
        );
    }
    #[test]
    fn additional_handler_paths_are_rejected() {
        let v = serde_json::json!({
            "TCP":{"443":{"HTTPS":true}},
            "Web":{"host.ts.net:443":{"Handlers":{
                "/":{"Proxy":BACKEND}, "/foreign":{"Proxy":"http://localhost:9999"}
            }}}
        });
        assert!(inspection_from(&v, Some(&rec("host.ts.net"))).is_err());
    }
    #[test]
    fn root_handler_extra_fields_are_rejected() {
        let v = serde_json::json!({
            "TCP":{"443":{"HTTPS":true}},
            "Web":{"host.ts.net:443":{"Handlers":{
                "/":{"Proxy":BACKEND,"Other":"foreign"}
            }}}
        });
        assert!(inspection_from(&v, Some(&rec("host.ts.net"))).is_err());
    }
    #[test]
    fn apply_rejects_foreign_postcondition() {
        let m = Mock::new(vec![
            out("ok"),
            out("1"),
            out(&status("foreign.ts.net", "http://127.0.0.1:9999")),
            out("1"),
            out(&status("foreign.ts.net", "http://127.0.0.1:9999")),
        ]);
        let current = inspection_from(&serde_json::json!({}), None).unwrap();
        assert!(apply(&m, ServePlan::Apply, &current).is_err());
    }
    #[test]
    fn nonzero_apply_is_distinguished_from_indeterminate_postcondition() {
        let m = Mock::new(vec![Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })]);
        let current = inspection_from(&serde_json::json!({}), None).unwrap();
        assert!(matches!(
            apply(&m, ServePlan::Apply, &current),
            Err(ServeError::CommandFailed)
        ));
    }
    #[test]
    fn malformed_shapes_rejected() {
        for s in [
            r#"{"TCP":null}"#,
            r#"{"Web":[]}"#,
            r#"{"Foreground":true}"#,
            r#"{"TCP":{"443":{"HTTPS":"yes"}}}"#,
        ] {
            assert!(inspection_from(&serde_json::from_str(s).unwrap(), None).is_err())
        }
    }
    #[test]
    fn version_checked() {
        let m = Mock::new(vec![out("1"), out("{}")]);
        assert_eq!(inspect(&m).unwrap().ownership, ServeOwnership::Absent);
        let first = m.calls.lock().unwrap()[0].clone();
        assert_eq!(first, vec!["version"])
    }
    #[test]
    fn expected_dns_name_accepts_tailscale_trailing_dot() {
        let m = Mock::new(vec![out(r#"{"Self":{"DNSName":"host.ts.net."}}"#)]);
        assert_eq!(expected_dns_name(&m).unwrap(), "host.ts.net");
    }
    #[test]
    fn missing_cli_is_typed() {
        let m = Mock::new(vec![Err("executable not found".into())]);
        assert!(matches!(inspect(&m), Err(ServeError::NotInstalled)));
    }
    #[test]
    fn preview_ports_bounded() {
        let p = preview_ports();
        assert_eq!(p.get(&8444), Some(&13844));
        assert_eq!(p.len(), 8)
    }
    #[test]
    fn restart_required_tracks_verified_agent_origin() {
        assert!(!restart_required(
            Some("https://host.ts.net"),
            Some("https://host.ts.net")
        ));
        assert!(restart_required(Some("https://host.ts.net"), None));
        assert!(restart_required(None, Some("https://host.ts.net")));
    }
    #[test]
    fn pending_journal_roundtrip() {
        let root = std::env::temp_dir().join(format!("rc-serve-pending-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("pending.json");
        let pending = PendingServe {
            enabled: true,
            receipt: Some(rec("host.ts.net")),
            expected_dns_name: Some("host.ts.net".into()),
            expected_proxy: BACKEND.into(),
        };
        write_pending(&path, &pending).unwrap();
        assert_eq!(load_pending(&path).unwrap(), Some(pending));
        remove_pending(&path).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    fn https_status(body: &str) -> String {
        body.to_owned()
    }

    #[test]
    fn https_ready_uses_only_running_json_status_and_valid_dns_name() {
        let m = Mock::new(vec![out(&https_status(
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net.","CapMap":{"https":null}}}"#,
        ))]);

        assert_eq!(https_ready(&m), Ok(true));
        assert_eq!(
            *m.calls.lock().unwrap(),
            vec![vec!["status".to_string(), "--json".to_string()]]
        );
    }

    #[test]
    fn https_ready_falls_back_to_capabilities_and_reports_false_when_empty() {
        let fallback = Mock::new(vec![out(
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net.","Capabilities":["funnel","https"]}}"#,
        )]);
        assert_eq!(https_ready(&fallback), Ok(true));

        let empty = Mock::new(vec![out(
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net.","CapMap":{},"Capabilities":[]}}"#,
        )]);
        assert_eq!(https_ready(&empty), Ok(false));
    }

    #[test]
    fn https_ready_rejects_failed_or_malformed_status_shapes() {
        let failed = Mock::new(vec![Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })]);
        assert!(https_ready(&failed).is_err());

        for body in [
            r#"[]"#,
            r#"{"BackendState":"Stopped","Self":{"DNSName":"host.ts.net.","CapMap":{"https":null}}}"#,
            r#"{"BackendState":"Running","Self":null}"#,
            r#"{"BackendState":"Running","Self":{"DNSName":"host.example.","CapMap":{"https":null}}}"#,
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net.","CapMap":{"https":true}}}"#,
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net.","Capabilities":["https",3]}}"#,
            r#"{"BackendState":"Running","Self":{"DNSName":"host.ts.net."}}"#,
        ] {
            let m = Mock::new(vec![out(body)]);
            assert!(https_ready(&m).is_err(), "unexpectedly accepted {body}");
        }
    }

    #[test]
    fn https_can_discard_unapplied_requires_unambiguous_pending_backend_state() {
        fn pending() -> PendingServe {
            PendingServe {
                enabled: true,
                receipt: None,
                expected_dns_name: Some("host.ts.net".into()),
                expected_proxy: BACKEND.into(),
            }
        }
        fn absent() -> ServeInspection {
            inspection_from(&serde_json::json!({}), None).unwrap()
        }

        assert!(can_discard_unapplied(
            &pending(),
            &absent(),
            None,
            None,
            "host.ts.net",
        ));

        let mut p = pending();
        p.enabled = false;
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "host.ts.net"
        ));

        let mut p = pending();
        p.receipt = Some(rec("host.ts.net"));
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "host.ts.net"
        ));

        let p = pending();
        let external = rec("host.ts.net");
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            Some(&external),
            None,
            "host.ts.net",
        ));
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            Some("https://host.ts.net"),
            "host.ts.net",
        ));

        let mut current = absent();
        current.ownership = ServeOwnership::Owned;
        assert!(!can_discard_unapplied(
            &p,
            &current,
            None,
            None,
            "host.ts.net"
        ));

        let mut p = pending();
        p.expected_proxy = "http://127.0.0.1:9999".into();
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "host.ts.net"
        ));

        let mut p = pending();
        p.expected_dns_name = None;
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "host.ts.net"
        ));

        let p = pending();
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "other.ts.net"
        ));
        assert!(!can_discard_unapplied(
            &p,
            &absent(),
            None,
            None,
            "host.example"
        ));
    }

    #[test]
    fn receipt_roundtrip_and_config_preservation() {
        let root = std::env::temp_dir().join(format!("rc-serve-test-{}", std::process::id()));
        let receipt = root.join("receipt.json");
        let config = root.join("agent.toml");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            &config,
            "bind = \"127.0.0.1:3847\"\nweb_dir = \"C:\\\\web\"\n\n[media]\nenabled = true\n",
        )
        .unwrap();
        write_receipt(&receipt, &rec("host.ts.net")).unwrap();
        assert_eq!(load_receipt(&receipt).unwrap(), Some(rec("host.ts.net")));
        update_public_origin(&config, Some("https://host.ts.net")).unwrap();
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(text.contains("bind = \"127.0.0.1:3847\""));
        assert!(text.contains("public_origin = \"https://host.ts.net\""));
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(doc["media"]["enabled"].as_bool(), Some(true));
        assert_eq!(doc["public_origin"].as_str(), Some("https://host.ts.net"));
        remove_receipt(&receipt).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
