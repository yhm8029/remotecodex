//! Authenticated, explicitly registered loopback HTTP gateways, on distinct origins.
//! No arbitrary URLs, management-origin iframe, blind port scan, or automatic Serve mutation.
use crate::{
    auth::secret,
    http::{principal, Failure, Shared},
};
use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{Message, WebSocketUpgrade},
        Path, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use rc_core::{error::ErrorCode, protocol::Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct PreviewInfo {
    pub id: Uuid,
    pub project_id: Uuid,
    pub port: u16,
    pub gateway_port: u16,
    pub origin: String,
    pub pid: u32,
    pub process_created: String,
    pub approved_by: Uuid,
}
struct Entry {
    info: PreviewInfo,
    created: u64,
    stop: CancellationToken,
}
struct LaunchGrant {
    preview: Uuid,
    client: Uuid,
    until: Instant,
}
struct BrowserGrant {
    preview: Uuid,
    client: Uuid,
    until: Instant,
}
#[derive(Default)]
struct Inner {
    entries: HashMap<Uuid, Arc<Entry>>,
    launches: HashMap<[u8; 32], LaunchGrant>,
    browsers: HashMap<[u8; 32], BrowserGrant>,
}
#[derive(Default)]
pub struct PreviewManager {
    inner: Mutex<Inner>,
}
fn hash(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}
impl PreviewManager {
    pub fn revoke_client(&self, id: Uuid) {
        let mut i = self.inner.lock();
        i.launches.retain(|_, g| g.client != id);
        i.browsers.retain(|_, g| g.client != id);
    }
    pub fn block_all(&self) {
        let mut i = self.inner.lock();
        i.launches.clear();
        i.browsers.clear();
    }
    pub fn shutdown(&self) {
        let mut i = self.inner.lock();
        for e in i.entries.values() {
            e.stop.cancel();
        }
        i.entries.clear();
        i.launches.clear();
        i.browsers.clear();
    }
    fn prune(i: &mut Inner) {
        let n = Instant::now();
        i.launches.retain(|_, x| x.until > n);
        i.browsers.retain(|_, x| x.until > n);
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    project_id: Uuid,
    port: u16,
    public_port: u16,
}
pub async fn list(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<axum::Json<Vec<PreviewInfo>>, Failure> {
    principal(&s, &h, Scope::PreviewRead)?;
    Ok(axum::Json(
        s.previews
            .inner
            .lock()
            .entries
            .values()
            .map(|e| e.info.clone())
            .collect(),
    ))
}
pub async fn register(
    State(s): State<Shared>,
    h: HeaderMap,
    axum::Json(body): axum::Json<Register>,
) -> Result<axum::Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::AdminSettings)?;
    if body.port < 1024
        || body.port == s.config.bind.port()
        || (13844..=13851).contains(&body.port)
        || !(8444..=8451).contains(&body.public_port)
    {
        return Err(ErrorCode::InvalidRequest.into());
    }
    let base = s
        .config
        .public_origin
        .as_ref()
        .ok_or(ErrorCode::PreviewNotApproved)?;
    let mut origin = url::Url::parse(base).map_err(|_| ErrorCode::InvalidRequest)?;
    origin
        .set_port(Some(body.public_port))
        .map_err(|_| ErrorCode::InvalidRequest)?;
    let origin = origin.origin().ascii_serialization();
    let port = body.port;
    let (pid, created) =
        tokio::task::spawn_blocking(move || rc_platform_windows::loopback_listener_identity(port))
            .await
            .map_err(|_| ErrorCode::Internal)?
            .map_err(|_| ErrorCode::UpstreamChanged)?;
    let gateway_port = body.public_port + 5400;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, gateway_port)))
            .await
            .map_err(|_| ErrorCode::PreviewNotApproved)?;
    let id = Uuid::new_v4();
    let entry = Arc::new(Entry {
        info: PreviewInfo {
            id,
            project_id: body.project_id,
            port: body.port,
            gateway_port,
            origin: origin.clone(),
            pid,
            process_created: created.to_string(),
            approved_by: p.client_id,
        },
        created,
        stop: CancellationToken::new(),
    });
    {
        let mut inner = s.previews.inner.lock();
        if inner.entries.len() >= 8
            || inner
                .entries
                .values()
                .any(|e| e.info.gateway_port == gateway_port)
        {
            return Err(ErrorCode::RateLimited.into());
        }
        inner.entries.insert(id, entry.clone());
    }
    let store = s.store.clone();
    let project = body.project_id;
    let origin_copy = origin.clone();
    let reserved =
        tokio::task::spawn_blocking(move || store.reserve_preview_slot(&origin_copy, project))
            .await
            .map_err(|_| ErrorCode::Internal)?;
    if reserved.is_err() {
        s.previews.inner.lock().entries.remove(&id);
        return Err(ErrorCode::PreviewNotApproved.into());
    }
    let gateway = Gateway {
        state: s.clone(),
        entry: entry.clone(),
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| ErrorCode::Internal)?,
    };
    tokio::spawn(async move {
        let stop = entry.stop.clone();
        let app = Router::new().fallback(proxy).with_state(gateway);
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await;
    });
    Ok(axum::Json(
        serde_json::json!({"id":id,"origin":origin,"gateway_port":gateway_port,"network_configuration_required":true,
        "serve_plan":format!("Inspect existing Serve config, then explicitly approve HTTPS {} -> http://127.0.0.1:{}",body.public_port,gateway_port),
        "limitations":["Trusted development apps only in shared-host/port mode","No service workers","document.cookie name mapping is not transparent","Bearer Authorization is not forwarded in this initial safety profile"]}),
    ))
}
pub async fn remove(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Failure> {
    principal(&s, &h, Scope::AdminSettings)?;
    let mut i = s.previews.inner.lock();
    if let Some(e) = i.entries.remove(&id) {
        e.stop.cancel();
    }
    i.launches.retain(|_, g| g.preview != id);
    i.browsers.retain(|_, g| g.preview != id);
    Ok(StatusCode::NO_CONTENT)
}
pub async fn launch(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<serde_json::Value>, Failure> {
    let p = principal(&s, &h, Scope::PreviewRead)?;
    let mut i = s.previews.inner.lock();
    PreviewManager::prune(&mut i);
    let origin = i
        .entries
        .get(&id)
        .ok_or(ErrorCode::PreviewNotApproved)?
        .info
        .origin
        .clone();
    if i.launches.len() >= 64 {
        return Err(ErrorCode::RateLimited.into());
    }
    let ticket = secret();
    i.launches.insert(
        hash(&ticket),
        LaunchGrant {
            preview: id,
            client: p.client_id,
            until: Instant::now() + Duration::from_secs(30),
        },
    );
    Ok(axum::Json(
        serde_json::json!({"url":format!("{origin}/_rc/bootstrap#{ticket}"),"expires_in":30}),
    ))
}
#[derive(Clone)]
struct Gateway {
    state: Shared,
    entry: Arc<Entry>,
    http: reqwest::Client,
}
fn cookie_name(id: Uuid) -> String {
    format!("__Host-rcpv-{}", id.simple())
}
fn cookie_prefix(id: Uuid) -> String {
    format!("rcapp_{}_", id.simple())
}
const SCRIPT: &str = r#"const ticket=location.hash.slice(1);history.replaceState(null,'','/_rc/bootstrap');fetch('/_rc/consume',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({ticket})}).then(r=>{if(!r.ok)throw Error('Preview approval expired');location.replace('/')}).catch(()=>{document.body.textContent='Preview authorization failed. Reopen it from RemoteCodex.'});"#;
async fn proxy(
    State(g): State<Gateway>,
    upgrade: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    request: Request,
) -> Response {
    match proxy_inner(g, upgrade.ok(), request).await {
        Ok(response) => response,
        Err(e) => Failure(e).into_response(),
    }
}
async fn proxy_inner(
    g: Gateway,
    upgrade: Option<WebSocketUpgrade>,
    request: Request,
) -> Result<Response, ErrorCode> {
    if g.entry.stop.is_cancelled() {
        return Err(ErrorCode::PreviewNotApproved);
    }
    let origin = &g.entry.info.origin;
    let expected = url::Url::parse(origin).map_err(|_| ErrorCode::Internal)?;
    let authority = format!(
        "{}:{}",
        expected.host_str().ok_or(ErrorCode::Internal)?,
        expected.port_or_known_default().unwrap_or(443)
    );
    if request.headers().get("host").and_then(|v| v.to_str().ok()) != Some(authority.as_str()) {
        return Err(ErrorCode::OriginRejected);
    }
    if let Some(o) = request.headers().get("origin") {
        if o.to_str().ok() != Some(origin.as_str()) {
            return Err(ErrorCode::OriginRejected);
        }
    }
    let path = request.uri().path();
    if path == "/_rc/bootstrap" && request.method() == axum::http::Method::GET {
        let digest =
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(SCRIPT.as_bytes()));
        return Ok(Response::builder().status(200).header("content-type","text/html; charset=utf-8").header("cache-control","no-store").header("referrer-policy","no-referrer").header("content-security-policy",format!("default-src 'none'; script-src 'sha256-{digest}'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'"))
            .body(Body::from(format!("<!doctype html><meta charset=utf-8><title>RemoteCodex preview</title><p>Authenticating this preview…</p><script>{SCRIPT}</script>"))).map_err(|_|ErrorCode::Internal)?);
    }
    if path == "/_rc/consume" && request.method() == axum::http::Method::POST {
        if request
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            != Some(origin.as_str())
        {
            return Err(ErrorCode::OriginRejected);
        }
        let bytes = to_bytes(request.into_body(), 2048)
            .await
            .map_err(|_| ErrorCode::InvalidRequest)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| ErrorCode::InvalidRequest)?;
        let token = value["ticket"]
            .as_str()
            .filter(|t| t.len() <= 128)
            .ok_or(ErrorCode::Unauthenticated)?;
        let mut inner = g.state.previews.inner.lock();
        PreviewManager::prune(&mut inner);
        let grant = inner
            .launches
            .remove(&hash(token))
            .filter(|v| v.preview == g.entry.info.id)
            .ok_or(ErrorCode::Unauthenticated)?;
        g.state
            .auth
            .device_principal(grant.client)?
            .require(Scope::PreviewRead)?;
        if inner.browsers.len() >= 128 {
            return Err(ErrorCode::RateLimited);
        }
        let credential = secret();
        inner.browsers.insert(
            hash(&credential),
            BrowserGrant {
                client: grant.client,
                preview: grant.preview,
                until: Instant::now() + Duration::from_secs(600),
            },
        );
        return Response::builder()
            .status(204)
            .header("cache-control", "no-store")
            .header(
                "set-cookie",
                format!(
                    "{}={}; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=600",
                    cookie_name(grant.preview),
                    credential
                ),
            )
            .body(Body::empty())
            .map_err(|_| ErrorCode::Internal);
    }
    if path.starts_with("/_rc/") {
        return Err(ErrorCode::InvalidRequest);
    }
    let cookies = request
        .headers()
        .get(header::COOKIE)
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    let name = cookie_name(g.entry.info.id);
    let credential = cookies
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .find_map(|(n, v)| (n == name).then_some(v))
        .ok_or(ErrorCode::Unauthenticated)?;
    let client = {
        let mut i = g.state.previews.inner.lock();
        PreviewManager::prune(&mut i);
        i.browsers
            .get(&hash(credential))
            .filter(|b| b.preview == g.entry.info.id)
            .map(|b| b.client)
            .ok_or(ErrorCode::Unauthenticated)?
    };
    let principal = g.state.auth.device_principal(client)?;
    principal.require(Scope::PreviewRead)?;
    // Fresh process-creation timestamp check prevents trusting a reused PID/port indefinitely.
    let port = g.entry.info.port;
    let identity =
        tokio::task::spawn_blocking(move || rc_platform_windows::loopback_listener_identity(port))
            .await
            .map_err(|_| ErrorCode::Internal)?
            .map_err(|_| ErrorCode::UpstreamChanged)?;
    if identity != (g.entry.info.pid, g.entry.created) {
        g.entry.stop.cancel();
        return Err(ErrorCode::UpstreamChanged);
    }
    let suffix = request
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let url = format!("http://127.0.0.1:{port}{suffix}");
    let mut headers = filtered_request_headers(request.headers(), g.entry.info.id);
    headers.insert(
        header::HOST,
        HeaderValue::from_str(&format!("127.0.0.1:{port}")).map_err(|_| ErrorCode::Internal)?,
    );
    if request.headers().contains_key(header::ORIGIN) {
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_str(&format!("http://127.0.0.1:{port}"))
                .map_err(|_| ErrorCode::Internal)?,
        );
    }
    if let Some(upgrade) = upgrade {
        return websocket_proxy(g, principal, upgrade, request.headers(), suffix.to_owned()).await;
    }
    let method = request.method().clone();
    let body = request.into_body().into_data_stream();
    let upstream = g
        .http
        .request(method, url)
        .headers(headers)
        .body(reqwest::Body::wrap_stream(body))
        .send();
    let response = tokio::select! {_ = principal.revoked.cancelled()=>return Err(ErrorCode::DeviceRevoked),_ = g.entry.stop.cancelled()=>return Err(ErrorCode::PreviewNotApproved),r=upstream=>r.map_err(|_|ErrorCode::UpstreamChanged)?};
    let status = response.status();
    let mut output = response.headers().clone();
    strip_hop_by_hop(&mut output);
    output.remove("set-cookie");
    for raw in response.headers().get_all("set-cookie") {
        if let Ok(v) = raw.to_str() {
            if let Ok(mut cookie) = cookie::Cookie::parse(v.to_owned()) {
                let old = cookie.name().to_owned();
                if old.starts_with("__Host-rcpv-") || old.starts_with("rcapp_") {
                    continue;
                }
                cookie.set_name(format!(
                    "{}{}",
                    cookie_prefix(g.entry.info.id),
                    URL_SAFE_NO_PAD.encode(old.as_bytes())
                ));
                cookie.unset_domain();
                cookie.set_secure(true);
                if let Ok(v) = HeaderValue::from_str(&cookie.to_string()) {
                    output.append(header::SET_COOKIE, v);
                }
            }
        }
    }
    // Add, do not remove, application CSP / X-Frame-Options. The most restrictive policy wins.
    output.append(
        "content-security-policy",
        HeaderValue::from_static("worker-src 'none'"),
    );
    output.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    output.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    let stop = g.entry.stop.clone();
    let revoked = principal.revoked.clone();
    let mut stream = response.bytes_stream();
    let body = async_stream::stream! {
        loop{tokio::select!{
            _=stop.cancelled()=>break,_=revoked.cancelled()=>break,
            next=stream.next()=>match next{Some(Ok(bytes))=>yield Ok::<_,std::io::Error>(bytes),Some(Err(e))=>{yield Err(std::io::Error::other(e));break;},None=>break},
        }}
    };
    let mut result = Response::new(Body::from_stream(body));
    *result.status_mut() = status;
    *result.headers_mut() = output;
    Ok(result)
}
fn strip_hop_by_hop(h: &mut HeaderMap) {
    let listed = h
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect::<Vec<_>>();
    for k in listed {
        h.remove(k);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        h.remove(name);
    }
}
fn filtered_request_headers(input: &HeaderMap, id: Uuid) -> HeaderMap {
    let mut h = input.clone();
    strip_hop_by_hop(&mut h);
    for name in [
        "authorization",
        "cookie",
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "tailscale-user-login",
        "tailscale-user-name",
        "sec-websocket-key",
        "sec-websocket-version",
        "sec-websocket-protocol",
    ] {
        h.remove(name);
    }
    let prefix = cookie_prefix(id);
    let cookies = input
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .filter_map(|(name, value)| {
            let encoded = name.strip_prefix(&prefix)?;
            let decoded = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded).ok()?).ok()?;
            if decoded
                .bytes()
                .any(|b| b <= 32 || b >= 127 || b == b';' || b == b'=')
            {
                return None;
            }
            Some(format!("{decoded}={value}"))
        })
        .collect::<Vec<_>>()
        .join("; ");
    if !cookies.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&cookies) {
            h.insert(header::COOKIE, v);
        }
    }
    h
}
async fn websocket_proxy(
    g: Gateway,
    p: crate::auth::Principal,
    upgrade: WebSocketUpgrade,
    original: &HeaderMap,
    suffix: String,
) -> Result<Response, ErrorCode> {
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message as UpMessage};
    let port = g.entry.info.port;
    let mut request = format!("ws://127.0.0.1:{port}{suffix}")
        .into_client_request()
        .map_err(|_| ErrorCode::InvalidRequest)?;
    for (k, v) in filtered_request_headers(original, g.entry.info.id).iter() {
        if k != header::HOST {
            request.headers_mut().insert(k.clone(), v.clone());
        }
    }
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_str(&format!("http://127.0.0.1:{port}"))
            .map_err(|_| ErrorCode::Internal)?,
    );
    if let Some(v) = original.get("sec-websocket-protocol") {
        request
            .headers_mut()
            .insert("sec-websocket-protocol", v.clone());
    }
    let mut ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    ws_config.max_message_size = Some(1024 * 1024);
    ws_config.max_frame_size = Some(1024 * 1024);
    let (upstream, response) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async_with_config(request, Some(ws_config), false),
    )
    .await
    .map_err(|_| ErrorCode::UpstreamChanged)?
    .map_err(|_| ErrorCode::UpstreamChanged)?;
    let protocol = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let upgrade = if let Some(protocol) = protocol {
        upgrade.protocols([protocol])
    } else {
        upgrade
    };
    Ok(upgrade.max_message_size(1024*1024).on_upgrade(move|mut browser|async move{
        let(mut out,mut input)=upstream.split();let expires=tokio::time::Instant::now()+Duration::from_secs(600);
        loop{tokio::select!{
            _=g.entry.stop.cancelled()=>break,_=p.revoked.cancelled()=>break,_=tokio::time::sleep_until(expires)=>break,
            from=browser.recv()=>match from{
                Some(Ok(Message::Text(t)))=>{if out.send(UpMessage::Text(t.to_string().into())).await.is_err(){break;}},
                Some(Ok(Message::Binary(b)))=>{if out.send(UpMessage::Binary(b)).await.is_err(){break;}},
                Some(Ok(Message::Ping(_)|Message::Pong(_)))=>{},_=>break,
            },
            from=input.next()=>match from{
                Some(Ok(UpMessage::Text(t)))=>{if browser.send(Message::Text(t.to_string().into())).await.is_err(){break;}},
                Some(Ok(UpMessage::Binary(b)))=>{if b.len()>1024*1024||browser.send(Message::Binary(b)).await.is_err(){break;}},
                Some(Ok(UpMessage::Ping(_)|UpMessage::Pong(_)))=>{},_=>break,
            },
        }}
        let _=out.close().await;let _=browser.send(Message::Close(None)).await;
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn do_not_forward_control_credentials() {
        let id = Uuid::new_v4();
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("Bearer SECRET"));
        h.insert(
            "cookie",
            HeaderValue::from_str(&format!(
                "{}=SECRET; {}{}=value",
                cookie_name(id),
                cookie_prefix(id),
                URL_SAFE_NO_PAD.encode("app")
            ))
            .unwrap(),
        );
        let f = filtered_request_headers(&h, id);
        assert!(!f.contains_key("authorization"));
        assert_eq!(f.get("cookie").unwrap(), "app=value");
    }
    #[test]
    fn connection_named_headers_removed() {
        let mut h = HeaderMap::new();
        h.insert("connection", HeaderValue::from_static("X-Remove"));
        h.insert("x-remove", HeaderValue::from_static("secret"));
        strip_hop_by_hop(&mut h);
        assert!(h.is_empty());
    }
}
