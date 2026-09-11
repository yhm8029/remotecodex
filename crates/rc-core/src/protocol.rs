use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_MAJOR: u16 = 1;
pub const MAX_INPUT: usize = 8 * 1024;
pub const MAX_PASTE: usize = 256 * 1024;
pub const MAX_SESSIONS: usize = 8;
pub const MAX_COLS: u16 = 400;
pub const MAX_ROWS: u16 = 150;
pub const MAX_JS_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "terminal.read")]
    TerminalRead,
    #[serde(rename = "terminal.write")]
    TerminalWrite,
    #[serde(rename = "terminal.create")]
    TerminalCreate,
    #[serde(rename = "terminal.close")]
    TerminalClose,
    #[serde(rename = "preview.read")]
    PreviewRead,
    #[serde(rename = "window.view")]
    WindowView,
    #[serde(rename = "window.control")]
    WindowControl,
    #[serde(rename = "desktop.view")]
    DesktopView,
    #[serde(rename = "desktop.control")]
    DesktopControl,
    #[serde(rename = "admin.devices")]
    AdminDevices,
    #[serde(rename = "admin.settings")]
    AdminSettings,
}
impl Scope {
    pub fn owner() -> Vec<Self> {
        vec![
            Self::TerminalRead,
            Self::TerminalWrite,
            Self::TerminalCreate,
            Self::TerminalClose,
            Self::PreviewRead,
            Self::AdminDevices,
            Self::AdminSettings,
        ]
        // GUI permissions must be approved separately after P4/P5/P6 gates.
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSession {
    pub label: String,
    pub project_id: Option<Uuid>,
    pub cwd: String,
    pub profile: String,
    pub cols: u16,
    pub rows: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Starting,
    Running,
    Closing,
    Exited,
    Lost,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: Uuid,
    pub project_id: Option<Uuid>,
    pub label: String,
    pub initial_cwd: String,
    pub profile: String,
    pub agent_epoch: Uuid,
    pub generation: u32,
    pub state: Lifecycle,
    pub cols: u16,
    pub rows: u16,
    pub output_seq: String,
    pub pid: Option<u32>,
    pub process_created: Option<String>,
    pub lease: Option<LeaseView>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseView {
    pub client_id: Uuid,
    pub connection_id: Uuid,
    pub epoch: u64,
    pub remaining_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlMessage {
    LeaseAcquire {
        request_id: Uuid,
        session_id: Uuid,
        takeover: bool,
    },
    LeaseRenew {
        request_id: Uuid,
        session_id: Uuid,
        lease_epoch: u64,
    },
    LeaseRelease {
        request_id: Uuid,
        session_id: Uuid,
        lease_epoch: u64,
    },
    Input {
        request_id: Uuid,
        session_id: Uuid,
        agent_epoch: Uuid,
        generation: u32,
        lease_epoch: u64,
        input_id: Uuid,
        input_seq: u64,
        payload: InputPayload,
    },
    Resize {
        request_id: Uuid,
        session_id: Uuid,
        agent_epoch: Uuid,
        generation: u32,
        lease_epoch: u64,
        cols: u16,
        rows: u16,
    },
    RefreshAuth {
        request_id: Uuid,
        ticket: String,
    },
    Ping {
        request_id: Uuid,
    },
}
impl ControlMessage {
    pub fn request_id(&self) -> Uuid {
        match self {
            Self::LeaseAcquire { request_id, .. }
            | Self::LeaseRenew { request_id, .. }
            | Self::LeaseRelease { request_id, .. }
            | Self::Input { request_id, .. }
            | Self::Resize { request_id, .. }
            | Self::RefreshAuth { request_id, .. }
            | Self::Ping { request_id } => *request_id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputPayload {
    Utf8 { text: String },
    Binary { base64: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamMessage {
    RefreshAuth { ticket: String },
    Applied { sequence: String, bytes: u32 },
    Ping,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub cols: u16,
    pub rows: u16,
    pub sequence: String,
    pub generation: u32,
    pub agent_epoch: Uuid,
    pub fidelity: String,
    pub warnings: Vec<String>,
    pub bytes: usize,
}
