use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bind: SocketAddr,
    pub public_origin: Option<String>,
    pub local_ui_origins: Vec<String>,
    pub data_dir: PathBuf,
    pub web_dir: PathBuf,
    pub max_sessions: usize,
    pub ring_bytes: usize,
    pub allow_gui_experiments: bool,
    pub media: crate::media::MediaConfig,
}
impl Default for Config {
    fn default() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join(".local/share")
            });
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3847),
            public_origin: None,
            local_ui_origins: vec!["http://tauri.localhost".into(), "tauri://localhost".into()],
            data_dir: base.join("RemoteCodex"),
            web_dir: PathBuf::from("apps/web/dist"),
            max_sessions: 8,
            ring_bytes: 8 * 1024 * 1024,
            allow_gui_experiments: false,
            media: Default::default(),
        }
    }
}
impl Config {
    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        let c: Self = if let Some(p) = path {
            toml::from_str(&std::fs::read_to_string(p).context("Read config")?)
                .context("Parse config")?
        } else {
            Self::default()
        };
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        if !self.bind.ip().is_loopback() {
            bail!("Agent must bind to loopback. Configure Tailscale Serve for remote access.");
        }
        if self.bind.port() == 0 {
            bail!("Management port cannot be zero");
        }
        if self.max_sessions == 0 || self.max_sessions > 8 {
            bail!("max_sessions must be 1..8 for the initial resource budget");
        }
        if self.ring_bytes < 32768 || self.ring_bytes > 8 * 1024 * 1024 {
            bail!("ring_bytes outside supported bounds");
        }
        if let Some(s) = &self.public_origin {
            let u = url::Url::parse(s)?;
            if u.scheme() != "https"
                || u.host_str().is_none()
                || u.username() != ""
                || u.password().is_some()
                || u.path() != "/"
                || u.query().is_some()
                || u.fragment().is_some()
            {
                bail!("public_origin must be an exact HTTPS origin");
            }
        }
        for origin in &self.local_ui_origins {
            if !matches!(
                origin.as_str(),
                "http://tauri.localhost"
                    | "tauri://localhost"
                    | "http://localhost:1420"
                    | "http://127.0.0.1:1420"
            ) {
                bail!("Only the fixed local UI origins are permitted");
            }
        }
        if self.allow_gui_experiments {
            bail!("Legacy allow_gui_experiments flag is not accepted. Use explicit [media] configuration and local source approval.");
        }
        self.media.validate()?;
        Ok(())
    }
    pub fn local_origin(&self) -> String {
        format!("http://{}", self.bind)
    }
    pub fn origins(&self) -> Vec<String> {
        let mut v = self.local_ui_origins.clone();
        v.push(self.local_origin());
        if let Some(p) = &self.public_origin {
            v.push(p.trim_end_matches('/').into());
        }
        v
    }
    pub fn origin_allowed(&self, s: &str) -> bool {
        self.origins().iter().any(|v| v == s)
    }
    pub fn host_allowed(&self, s: &str) -> bool {
        if s == self.bind.to_string() {
            return true;
        }
        self.public_origin
            .as_ref()
            .and_then(|p| url::Url::parse(p).ok())
            .is_some_and(|u| {
                let h = u.host_str().unwrap_or("");
                let authority = u
                    .port()
                    .map(|p| format!("{h}:{p}"))
                    .unwrap_or_else(|| h.into());
                s == authority
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_bind_public() {
        let mut c = Config::default();
        c.bind = "0.0.0.0:3847".parse().unwrap();
        assert!(c.validate().is_err());
    }
    #[test]
    fn exact_origin_not_prefix() {
        let c = Config::default();
        assert!(!c.origin_allowed("http://127.0.0.1:3847.attacker.test"));
        assert!(!c.host_allowed("attacker.test"));
    }
}
