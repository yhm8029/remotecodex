//! Versioned, bounded media-control protocol. Video bytes never travel in these messages.
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_SIGNAL: usize = 96 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeSource {
    Window {
        handle: String,
        pid: u32,
        created: String,
    },
    Monitor {
        handle: String,
        device: String,
    },
}
impl NativeSource {
    pub fn handle(&self) -> Result<u64, String> {
        let s = match self {
            Self::Window { handle, .. } | Self::Monitor { handle, .. } => handle,
        };
        let h = s.parse::<u64>().map_err(|_| "Invalid native handle")?;
        if h == 0 || h > usize::MAX as u64 {
            return Err("Invalid native handle".into());
        }
        Ok(h)
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Window { .. } => "window",
            Self::Monitor { .. } => "monitor",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSource {
    pub native: NativeSource,
    pub label: String,
    pub rect: Rect,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceView {
    pub id: Uuid,
    pub generation: u32,
    pub geometry_version: String,
    pub label: String,
    pub kind: String,
    pub control_allowed: bool,
    pub rect: Rect,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}
impl Default for VideoProfile {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 15,
            bitrate_kbps: 2500,
        }
    }
}
impl VideoProfile {
    pub fn validate(&self) -> Result<(), String> {
        if !(64..=1920).contains(&self.width)
            || !(64..=1080).contains(&self.height)
            || self.width % 2 != 0
            || self.height % 2 != 0
            || !(1..=30).contains(&self.fps)
            || !(128..=12000).contains(&self.bitrate_kbps)
        {
            return Err("Unsupported video resource profile".into());
        }
        Ok(())
    }
    pub fn fit(mut self, r: Rect) -> Result<Self, String> {
        self.validate()?;
        if !r.valid() || r.width < 64 || r.height < 64 {
            return Err("Capture source too small".into());
        }
        let k = (self.width as f64 / r.width as f64)
            .min(self.height as f64 / r.height as f64)
            .min(1.0);
        self.width = ((r.width as f64 * k) as u32 / 2 * 2).max(64);
        self.height = ((r.height as f64 * k) as u32 / 2 * 2).max(64);
        Ok(self)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaInit {
    pub protocol: u16,
    pub source: LocalSource,
    pub profile: VideoProfile,
    pub tailnet_ip: String,
    pub min_port: u16,
    pub max_port: u16,
}
impl MediaInit {
    pub fn validate(&self) -> Result<(), String> {
        self.profile.validate()?;
        self.source.native.handle()?;
        if self.protocol != 1
            || !self.source.rect.valid()
            || !tailnet_ip(&self.tailnet_ip)
            || self.min_port < 1024
            || self.max_port < self.min_port
            || self.max_port - self.min_port > 31
        {
            return Err("Invalid media configuration".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HelperIn {
    Init { config: MediaInit },
    Answer { sdp: String },
    Ice { candidate: String, mline: u32 },
    Stop,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HelperOut {
    Ready { protocol: u16 },
    Offer { sdp: String },
    Ice { candidate: String, mline: u32 },
    Frame { sequence: String },
    Error { code: String },
    Stopped,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaClientMessage {
    Answer {
        sdp: String,
    },
    Ice {
        candidate: String,
        mline: u32,
    },
    Presented {
        generation: u32,
        geometry_version: String,
    },
    Acquire {
        generation: u32,
        geometry_version: String,
    },
    Renew {
        lease_epoch: String,
    },
    Release,
    Input {
        lease_epoch: String,
        generation: u32,
        geometry_version: String,
        seq: String,
        action: GuiAction,
    },
    RefreshAuth {
        ticket: String,
    },
    Ping,
    Stop,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GuiAction {
    Move {
        x: f64,
        y: f64,
    },
    Button {
        x: f64,
        y: f64,
        button: u8,
        down: bool,
    },
    Wheel {
        x: f64,
        y: f64,
        delta: i32,
    },
    Key {
        scan: u16,
        extended: bool,
        down: bool,
    },
    Text {
        text: String,
    },
}
impl GuiAction {
    pub fn validate(&self) -> Result<(), String> {
        let point = |x: f64, y: f64| {
            if x.is_finite()
                && y.is_finite()
                && (0.0..=1.0).contains(&x)
                && (0.0..=1.0).contains(&y)
            {
                Ok(())
            } else {
                Err("Invalid normalized point".into())
            }
        };
        match self {
            Self::Move { x, y } => point(*x, *y),
            Self::Button { x, y, button, .. } => {
                point(*x, *y)?;
                if *button > 2 {
                    Err("Unsupported button".into())
                } else {
                    Ok(())
                }
            }
            Self::Wheel { x, y, delta } => {
                point(*x, *y)?;
                if !(-1200..=1200).contains(delta) {
                    Err("Wheel out of range".into())
                } else {
                    Ok(())
                }
            }
            Self::Key { scan, .. } => {
                if (1..=88).contains(scan) {
                    Ok(())
                } else {
                    Err("Unsupported scan code".into())
                }
            }
            Self::Text { text } => {
                if text.encode_utf16().count() > 1024 || text.contains('\0') {
                    Err("Text too large/invalid".into())
                } else {
                    Ok(())
                }
            }
        }
    }
}
pub fn tailnet_ip(ip: &str) -> bool {
    let parts: Vec<_> = ip.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let mut n = [0u8; 4];
    for (i, s) in parts.iter().enumerate() {
        if s.is_empty()
            || s.len() > 3
            || (s.len() > 1 && s.starts_with('0'))
            || !s.bytes().all(|x| x.is_ascii_digit())
        {
            return false;
        }
        let Ok(v) = s.parse() else { return false };
        n[i] = v;
    }
    n[0] == 100 && (64..=127).contains(&n[1])
}
pub fn candidate_allowed(s: &str, exact_ip: Option<&str>) -> bool {
    if s.len() > 2048 || s.contains(['\r', '\n', '\0']) {
        return false;
    }
    let p: Vec<_> = s.split_whitespace().collect();
    p.len() >= 8
        && p[0].starts_with("candidate:")
        && p[0].len() > 10
        && p[1] == "1"
        && p[2].eq_ignore_ascii_case("udp")
        && p[3].parse::<u32>().is_ok()
        && tailnet_ip(p[4])
        && exact_ip.map_or(true, |ip| ip == p[4])
        && p[5].parse::<u16>().is_ok_and(|n| n > 0)
        && p[6] == "typ"
        && p[7] == "host"
}
pub fn sanitize_sdp(s: &str) -> Result<String, String> {
    if s.len() > 65536 || s.contains('\0') {
        return Err("Invalid SDP size".into());
    }
    let lines: Vec<_> = s.lines().map(|s| s.trim_end_matches('\r')).collect();
    if lines.iter().filter(|s| s.starts_with("m=")).count() != 1
        || !lines.iter().any(|s| s.starts_with("m=video "))
    {
        return Err("Video only".into());
    }
    let fingerprint = lines.iter().any(|s| {
        s.strip_prefix("a=fingerprint:sha-256 ").is_some_and(|fp| {
            let b: Vec<_> = fp.split(':').collect();
            b.len() == 32
                && b.iter()
                    .all(|v| v.len() == 2 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        })
    });
    if !fingerprint
        || !lines
            .iter()
            .any(|s| s.starts_with("a=rtpmap:") && s.to_ascii_uppercase().ends_with(" H264/90000"))
    {
        return Err("DTLS-SRTP H264 required".into());
    }
    Ok(lines
        .into_iter()
        .filter(|s| !s.starts_with("a=candidate:") && *s != "a=end-of-candidates")
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
}
/// Bounded line protocol: read_until without a bound permits hostile/out-of-control child output to allocate forever.
pub fn read_json_line<R: std::io::BufRead, T: serde::de::DeserializeOwned>(
    r: &mut R,
) -> std::io::Result<Option<T>> {
    use std::io::{BufRead, Read};
    let mut line = Vec::new();
    let n = r
        .take((MAX_SIGNAL + 1) as u64)
        .read_until(b'\n', &mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if n > MAX_SIGNAL || line.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Oversized/partial media frame",
        ));
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_tailnet_only() {
        for ip in ["100.64.0.1", "100.127.255.254"] {
            assert!(tailnet_ip(ip));
        }
        for ip in [
            "127.0.0.1",
            "100.128.0.1",
            "100.064.0.1",
            "host.local",
            "::1",
        ] {
            assert!(!tailnet_ip(ip));
        }
    }
    #[test]
    fn no_relay_or_public_candidate() {
        assert!(candidate_allowed(
            "candidate:1 1 udp 1 100.64.0.2 50000 typ host",
            None
        ));
        assert!(!candidate_allowed(
            "candidate:1 1 udp 1 8.8.8.8 50000 typ relay",
            None
        ));
    }
    #[test]
    fn handle_strings_do_not_round() {
        let n = NativeSource::Window {
            handle: "9007199254740993".into(),
            pid: 1,
            created: "2".into(),
        };
        assert_eq!(n.handle().unwrap(), 9007199254740993);
    }
    #[test]
    fn bounds() {
        assert!(GuiAction::Move {
            x: f64::NAN,
            y: 0.0
        }
        .validate()
        .is_err());
        assert!(VideoProfile {
            fps: 60,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn oversize_helper_line() {
        let d = vec![b'x'; MAX_SIGNAL + 2];
        let mut r = std::io::Cursor::new(d);
        assert!(read_json_line::<_, HelperOut>(&mut r).is_err());
    }
}
