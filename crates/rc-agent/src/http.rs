use crate::{
    auth::{Auth, Principal},
    config::Config,
    preview::PreviewManager,
    session::SessionManager,
    store::Store,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use rc_core::{error::ErrorCode, protocol::*};
use serde::Deserialize;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct AppState {
    pub config: Config,
    pub auth: Auth,
    pub store: Arc<Store>,
    pub sessions: SessionManager,
    pub previews: PreviewManager,
    pub media: crate::media::MediaManager,
    pub shutdown: CancellationToken,
    pub remote_connections: RemoteConnectionCounter,
}

#[derive(Default)]
pub struct RemoteConnectionCounter(AtomicUsize);

impl RemoteConnectionCounter {
    pub fn enter(&self) -> RemoteConnectionGuard<'_> {
        self.0.fetch_add(1, Ordering::AcqRel);
        RemoteConnectionGuard(&self.0)
    }

    pub fn active(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

pub struct RemoteConnectionGuard<'a>(&'a AtomicUsize);

impl Drop for RemoteConnectionGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod connection_counter_tests {
    use super::RemoteConnectionCounter;

    #[test]
    fn guards_track_active_connections_and_release_on_drop() {
        let counter = RemoteConnectionCounter::default();
        let first = counter.enter();
        let second = counter.enter();
        assert_eq!(counter.active(), 2);
        drop(first);
        assert_eq!(counter.active(), 1);
        drop(second);
        assert_eq!(counter.active(), 0);
    }
}
pub type Shared = Arc<AppState>;
pub struct Failure(pub ErrorCode);
impl From<ErrorCode> for Failure {
    fn from(value: ErrorCode) -> Self {
        Self(value)
    }
}
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ErrorCode::Unauthenticated | ErrorCode::DeviceRevoked => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden | ErrorCode::OriginRejected => StatusCode::FORBIDDEN,
            ErrorCode::SessionLost => StatusCode::NOT_FOUND,
            ErrorCode::LeaseBusy
            | ErrorCode::LeaseRequired
            | ErrorCode::LeaseStale
            | ErrorCode::DuplicateInput => StatusCode::CONFLICT,
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::CaptureUnsupported | ErrorCode::MediaUnavailable => {
                StatusCode::NOT_IMPLEMENTED
            }
            _ => StatusCode::BAD_REQUEST,
        };
        (
            status,
            Json(serde_json::json!({"code":self.0,"request_id":Uuid::new_v4()})),
        )
            .into_response()
    }
}
pub fn principal(
    state: &AppState,
    headers: &HeaderMap,
    scope: Scope,
) -> Result<Principal, Failure> {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ErrorCode::Unauthenticated)?;
    let p = state.auth.authorize(bearer)?;
    p.require(scope)?;
    Ok(p)
}
pub fn router(state: Shared) -> Router {
    let api = Router::new()
        .route("/auth/pair", post(pair))
        .route("/auth/challenge", post(challenge))
        .route("/auth/verify", post(verify))
        .route("/host", get(host))
        .route("/profiles", get(profiles))
        .route("/sessions", get(sessions).post(create_session))
        .route("/sessions/{id}", delete(close_session))
        .route("/sessions/{id}/projection", get(session_projection))
        .route("/sessions/{id}/rename", post(rename_session))
        .route("/history", get(history))
        .route("/tickets/ws", post(ws_ticket))
        .route("/devices", get(devices))
        .route("/devices/{id}", delete(revoke_device))
        .route("/pair-tickets", post(pair_ticket))
        .route("/block-remote", post(block_remote))
        .route(
            "/previews",
            get(crate::preview::list).post(crate::preview::register),
        )
        .route("/previews/{id}", delete(crate::preview::remove))
        .route("/previews/{id}/launch", post(crate::preview::launch))
        .route("/media/capabilities", get(crate::media::capabilities))
        .route("/media/sources", get(crate::media::sources))
        .route("/ws/media", get(crate::media::upgrade))
        .route("/ws/control", get(crate::ws::control_upgrade))
        .route("/ws/terminal", get(crate::ws::terminal_upgrade))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .route(
            "/sessions/{id}/paste",
            post(paste_session).layer(DefaultBodyLimit::max(1600 * 1024)),
        );
    let static_files = tower_http::services::ServeDir::new(&state.config.web_dir)
        .append_index_html_on_directories(true);
    Router::new()
        .nest("/api/v1", api)
        .fallback_service(static_files)
        .layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state)
}
async fn boundary(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !state.config.host_allowed(host) {
        return Failure(ErrorCode::OriginRejected).into_response();
    }
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if origin
        .as_ref()
        .is_some_and(|v| !state.config.origin_allowed(v))
    {
        return Failure(ErrorCode::OriginRejected).into_response();
    }
    // All API callers including native clients send an explicit allowlisted Origin.
    if request.uri().path().starts_with("/api/")
        && origin.is_none()
        && request.method() != Method::GET
        && request.method() != Method::HEAD
    {
        return Failure(ErrorCode::OriginRejected).into_response();
    }
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let h = response.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("cache-control", HeaderValue::from_static("no-store"));
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    let ws_source = state
        .config
        .public_origin
        .as_ref()
        .map(|s| s.replacen("https://", "wss://", 1))
        .unwrap_or_else(|| state.config.local_origin().replacen("http://", "ws://", 1));
    let policy=format!("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' {ws_source}; img-src 'self' data:; media-src 'self' blob:; font-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; worker-src 'self'");
    if let Ok(value) = HeaderValue::from_str(&policy) {
        h.insert("content-security-policy", value);
    }
    if let Some(origin) = origin {
        if let Ok(v) = HeaderValue::from_str(&origin) {
            h.insert("access-control-allow-origin", v);
            h.insert("vary", HeaderValue::from_static("Origin"));
            h.insert(
                "access-control-allow-methods",
                HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
            );
            h.insert(
                "access-control-allow-headers",
                HeaderValue::from_static("Authorization, Content-Type"),
            );
        }
    }
    response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    ticket: String,
    public_key: String,
    label: String,
}
async fn pair(
    State(s): State<Shared>,
    Json(body): Json<Pair>,
) -> Result<Json<serde_json::Value>, Failure> {
    let audience = s.auth.audience.clone();
    let d = tokio::task::spawn_blocking(move || {
        s.auth.pair(&body.ticket, &body.public_key, &body.label)
    })
    .await
    .map_err(|_| ErrorCode::Internal)??;
    Ok(Json(
        serde_json::json!({"client_id":d.client_id,"scopes":d.scopes,"audience":audience}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    client_id: Uuid,
}
async fn challenge(
    State(s): State<Shared>,
    Json(body): Json<Challenge>,
) -> Result<Json<serde_json::Value>, Failure> {
    let (id, message) = s.auth.challenge(body.client_id)?;
    Ok(Json(
        serde_json::json!({"challenge_id":id,"message":message,"audience":s.auth.audience,"expires_in":30}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verify {
    challenge_id: String,
    signature: String,
}
async fn verify(
    State(s): State<Shared>,
    Json(body): Json<Verify>,
) -> Result<Json<serde_json::Value>, Failure> {
    let token =
        tokio::task::spawn_blocking(move || s.auth.verify(&body.challenge_id, &body.signature))
            .await
            .map_err(|_| ErrorCode::Internal)??;
    Ok(Json(
        serde_json::json!({"access_token":token,"expires_in":600}),
    ))
}
async fn host(State(s): State<Shared>, h: HeaderMap) -> Result<Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::TerminalRead)?;
    Ok(Json(
        serde_json::json!({"name":"RemoteCodex Host","agent_epoch":s.sessions.epoch,"protocol":1,"client_id":p.client_id,"scopes":p.scopes,"public_origin":s.config.public_origin,"snapshot_profile":"experimental_alacritty_vt","windows_verified":false,"capabilities":{"terminal":true,"multi_pty":true,"preview":cfg!(windows),"media_configured":s.config.media.enabled,"media_probe_endpoint":"/api/v1/media/capabilities","gui_control_configured":s.config.media.allow_control}}),
    ))
}
async fn profiles(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, Failure> {
    principal(&s, &h, Scope::TerminalRead)?;
    Ok(Json(
        serde_json::to_value(crate::profiles::available()).map_err(|_| ErrorCode::Internal)?,
    ))
}
async fn sessions(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<Vec<SessionInfo>>, Failure> {
    principal(&s, &h, Scope::TerminalRead)?;
    Ok(Json(s.sessions.list()))
}
async fn session_projection(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, Failure> {
    principal(&s, &h, Scope::TerminalRead)?;
    let session = s.sessions.get(id)?;
    let value = tokio::task::spawn_blocking(move || session.readable_projection())
        .await
        .map_err(|_| ErrorCode::Internal)?;
    Ok(Json(value))
}
async fn create_session(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(body): Json<CreateSession>,
) -> Result<Json<SessionInfo>, Failure> {
    principal(&s, &h, Scope::TerminalCreate)?;
    let info = tokio::task::spawn_blocking(move || s.sessions.create(body))
        .await
        .map_err(|_| ErrorCode::Internal)??;
    Ok(Json(info))
}
async fn rename_session(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameSession>,
) -> Result<Json<SessionInfo>, Failure> {
    let p = principal(&s, &h, Scope::TerminalCreate)?;
    let info = tokio::task::spawn_blocking(move || s.sessions.rename(&p, id, body.label))
        .await
        .map_err(|_| ErrorCode::Internal)??;
    Ok(Json(info))
}
async fn close_session(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Failure> {
    principal(&s, &h, Scope::TerminalClose)?;
    let session = s.sessions.get(id)?;
    tokio::task::spawn_blocking(move || session.close())
        .await
        .map_err(|_| ErrorCode::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn history(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, Failure> {
    principal(&s, &h, Scope::TerminalRead)?;
    let rows = tokio::task::spawn_blocking(move || s.store.history())
        .await
        .map_err(|_| ErrorCode::Internal)?
        .map_err(|_| ErrorCode::Internal)?;
    Ok(Json(serde_json::json!(rows)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WsRequest {
    channel: String,
    session_id: Option<Uuid>,
}
async fn ws_ticket(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(body): Json<WsRequest>,
) -> Result<Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::TerminalRead)?;
    if body.channel == "media" {
        s.media
            .get(body.session_id.ok_or(ErrorCode::InvalidRequest)?, &p)?;
    } else if let Some(id) = body.session_id {
        s.sessions.get(id)?;
    }
    let ticket = s.auth.issue_ws(p, body.channel, body.session_id)?;
    Ok(Json(serde_json::json!({"ticket":ticket,"expires_in":30})))
}
async fn devices(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, Failure> {
    principal(&s, &h, Scope::AdminDevices)?;
    Ok(Json(serde_json::json!(s.auth.devices_public())))
}
async fn revoke_device(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Failure> {
    principal(&s, &h, Scope::AdminDevices)?;
    tokio::task::spawn_blocking(move || {
        s.auth.revoke(id)?;
        s.sessions.revoke_client(id);
        s.previews.revoke_client(id);
        s.media.revoke_client(id);
        Ok::<_, ErrorCode>(())
    })
    .await
    .map_err(|_| ErrorCode::Internal)??;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairScopes {
    scopes: Vec<Scope>,
}
async fn pair_ticket(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(body): Json<PairScopes>,
) -> Result<Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::AdminDevices)?;
    if body.scopes.iter().any(|v| !p.scopes.contains(v)) {
        return Err(ErrorCode::Forbidden.into());
    }
    let t = s.auth.issue_pair_ticket(body.scopes)?;
    Ok(Json(serde_json::json!({"ticket":t,"expires_in":300})))
}
async fn block_remote(State(s): State<Shared>, h: HeaderMap) -> Result<StatusCode, Failure> {
    principal(&s, &h, Scope::AdminSettings)?;
    s.auth.block_all();
    s.sessions.block_input();
    s.previews.block_all();
    s.media.cancel_all();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasteRequest {
    agent_epoch: Uuid,
    generation: u32,
    connection_id: Uuid,
    lease_epoch: u64,
    input_id: Uuid,
    input_seq: u64,
    text: String,
    append_enter: bool,
}
async fn paste_session(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(b): Json<PasteRequest>,
) -> Result<Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::TerminalWrite)?;
    let session = s.sessions.get(id)?;
    session.validate_generation(b.agent_epoch, b.generation)?;
    let rx = session.enqueue_paste(
        p,
        b.connection_id,
        b.lease_epoch,
        b.input_id,
        b.input_seq,
        b.text,
        b.append_enter,
        std::time::Instant::now(),
    )?;
    let elapsed = tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .map_err(|_| ErrorCode::InputDeliveryUnknown)?
        .map_err(|_| ErrorCode::InputDeliveryUnknown)??;
    Ok(Json(
        serde_json::json!({"type":"written","input_id":b.input_id,"agent_receive_to_write_us":elapsed}),
    ))
}
