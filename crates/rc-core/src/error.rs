use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    #[error("Authentication is required")] Unauthenticated,
    #[error("This device does not have permission")] Forbidden,
    #[error("Device authorization was revoked")] DeviceRevoked,
    #[error("Input control is required")] LeaseRequired,
    #[error("The input lease is no longer valid")] LeaseStale,
    #[error("Another connection owns input control")] LeaseBusy,
    #[error("Session generation mismatch")] SessionGenerationMismatch,
    #[error("Host restarted; reconnect before sending input")] HostRestarted,
    #[error("Session is not running")] SessionLost,
    #[error("Input may have been partially delivered; do not replay automatically")] InputDeliveryUnknown,
    #[error("Input ID or sequence was already consumed")] DuplicateInput,
    #[error("Queued input was canceled before PTY write")] InputCanceled,
    #[error("Message or buffer exceeds its limit")] FrameTooLarge,
    #[error("Rate or resource limit reached")] RateLimited,
    #[error("A new terminal snapshot is required")] ResyncRequired,
    #[error("Preview is not approved")] PreviewNotApproved,
    #[error("Upstream process identity changed")] UpstreamChanged,
    #[error("Capture is not available on this build")] CaptureUnsupported,
    #[error("Source geometry or frame is stale")] SourceStale,
    #[error("The interactive desktop is locked or unavailable")] DesktopLocked,
    #[error("Target integrity or foreground does not permit input")] ElevationRequired,
    #[error("The media pipeline has not passed the platform gate")] MediaUnavailable,
    #[error("Origin or Host is not approved")] OriginRejected,
    #[error("Invalid request")] InvalidRequest,
    #[error("Service failure; see local diagnostic metadata")] Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub request_id: Option<String>,
}
impl From<ErrorCode> for ApiError {
    fn from(code: ErrorCode) -> Self {
        Self { message: code.to_string(), retryable: matches!(code, ErrorCode::RateLimited | ErrorCode::ResyncRequired), code, request_id: None }
    }
}
