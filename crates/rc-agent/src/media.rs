//! Isolated media orchestration: approval, scope, one host-wide GUI owner, helper lifecycle.
//! No video bytes share the terminal WebSocket or enter the terminal parser.
use crate::{
    auth::Principal,
    http::{principal, Failure, Shared},
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::HeaderMap,
    response::Response,
    Json,
};
use parking_lot::Mutex;
use rc_core::{error::ErrorCode, media_wire::*, protocol::Scope};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaConfig {
    pub enabled: bool,
    pub allow_control: bool,
    pub tailnet_ip: Option<String>,
    pub allowed_peer_ips: Vec<String>,
    pub min_port: u16,
    pub max_port: u16,
    pub helper_path: Option<PathBuf>,
    pub profile: VideoProfile,
}
impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_control: false,
            tailnet_ip: None,
            allowed_peer_ips: Vec::new(),
            min_port: 50000,
            max_port: 50010,
            helper_path: None,
            profile: Default::default(),
        }
    }
}
impl MediaConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        self.profile.validate().map_err(anyhow::Error::msg)?;
        if self.min_port < 1024
            || self.max_port < self.min_port
            || self.max_port - self.min_port > 31
        {
            anyhow::bail!("Media UDP range must contain at most 32 ports above 1023");
        }
        if self.allowed_peer_ips.len() > 16 || self.allowed_peer_ips.iter().any(|x| !tailnet_ip(x))
        {
            anyhow::bail!("Media peers must be explicitly listed numeric Tailscale IPv4 addresses");
        }
        if self.enabled
            && (self.tailnet_ip.as_deref().map_or(true, |x| !tailnet_ip(x))
                || self.allowed_peer_ips.is_empty())
        {
            anyhow::bail!("Enabled media needs company tailnet_ip and allowed_peer_ips");
        }
        if self.helper_path.as_ref().is_some_and(|p| !p.is_absolute()) {
            anyhow::bail!("Explicit helper_path must be absolute");
        }
        Ok(())
    }
    pub fn helper(&self) -> anyhow::Result<PathBuf> {
        Ok(match &self.helper_path {
            Some(p) => p.clone(),
            None => {
                let exe = std::env::current_exe()?;
                let packaged = exe
                    .parent()
                    .map(|p| {
                        p.join("media-runtime")
                            .join("bin")
                            .join(helper_name(&exe))
                            .is_file()
                    })
                    .unwrap_or(false);
                default_helper_from(&exe, packaged)
            }
        })
    }
}
fn helper_name(exe: &Path) -> &'static str {
    if exe
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
    {
        "rc-media.exe"
    } else {
        "rc-media"
    }
}
fn default_helper_from(exe: &Path, packaged: bool) -> PathBuf {
    let parent = exe.parent().unwrap_or_else(|| Path::new("."));
    if packaged {
        parent
            .join("media-runtime")
            .join("bin")
            .join(helper_name(exe))
    } else {
        parent.join(helper_name(exe))
    }
}
fn bundled_runtime(helper: &Path) -> Option<PathBuf> {
    let bin = helper.parent()?;
    if !bin
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case("bin")
    {
        return None;
    }
    let runtime = bin.parent()?;
    if !runtime
        .file_name()?
        .to_string_lossy()
        .eq_ignore_ascii_case("media-runtime")
    {
        return None;
    }
    Some(runtime.to_path_buf())
}
fn configure_helper_command(command: &mut tokio::process::Command, helper: &Path, arg: &str) {
    command.arg(arg);
    let Some(runtime) = bundled_runtime(helper) else {
        return;
    };
    let bin = runtime.join("bin");
    let mut path = OsString::new();
    path.push(&bin);
    if let Some(existing) = std::env::var_os("PATH") {
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(existing);
    }
    command
        .env("PATH", path)
        .env("GST_PLUGIN_SYSTEM_PATH_1_0", "")
        .env(
            "GST_PLUGIN_PATH_1_0",
            runtime.join("lib").join("gstreamer-1.0"),
        )
        .env(
            "GST_PLUGIN_SCANNER",
            runtime
                .join("libexec")
                .join("gstreamer-1.0")
                .join("gst-plugin-scanner.exe"),
        );
}
#[derive(Clone)]
pub struct Approved {
    pub view: SourceView,
    pub local: LocalSource,
    until: Instant,
}
struct Active {
    id: Uuid,
    client: Uuid,
    cancel: CancellationToken,
}

fn helper_control_ready(
    helper_control_allowed: bool,
    capture_state: CaptureState,
    last_encoded: Instant,
    last_presented: Instant,
    now: Instant,
) -> bool {
    helper_control_allowed
        && capture_state == CaptureState::Live
        && now.duration_since(last_encoded) <= Duration::from_millis(500)
        && now.duration_since(last_presented) <= Duration::from_millis(500)
}

fn capture_age_ms(now_ms: u64, captured_at_ms: u64) -> Result<u64, &'static str> {
    if captured_at_ms > now_ms {
        return Err("Captured frame timestamp is in the future");
    }
    let age = now_ms - captured_at_ms;
    if age > 60_000 {
        return Err("Captured frame age exceeded safety bound");
    }
    Ok(age)
}
#[derive(Default)]
struct Inner {
    sources: HashMap<Uuid, Approved>,
    active: Option<Active>,
}
#[derive(Clone)]
pub struct MediaManager {
    config: MediaConfig,
    inner: Arc<Mutex<Inner>>,
    probe: Arc<tokio::sync::Mutex<Option<(Instant, serde_json::Value)>>>,
    gui_controlled: Arc<AtomicBool>,
}
impl MediaManager {
    pub fn new(config: MediaConfig) -> Self {
        Self {
            config,
            inner: Default::default(),
            probe: Default::default(),
            gui_controlled: Default::default(),
        }
    }
    pub fn gui_controlled(&self) -> bool {
        self.gui_controlled.load(Ordering::Acquire)
    }
    pub fn approve(&self, kind: &str, handle: &str, control: bool) -> anyhow::Result<SourceView> {
        if !self.config.enabled {
            anyhow::bail!("Media is disabled in host configuration");
        }
        if control && !self.config.allow_control {
            anyhow::bail!("GUI control disabled in configuration");
        }
        let source = rc_platform_windows::enumerate_sources()?
            .into_iter()
            .find(|s| {
                s.native.kind() == kind && s.native.handle().is_ok_and(|h| h.to_string() == handle)
            })
            .ok_or_else(|| anyhow::anyhow!("Local source no longer exists"))?;
        let mut inner = self.inner.lock();
        inner.sources.retain(|_, s| s.until > Instant::now());
        if inner.sources.len() >= 16 {
            anyhow::bail!("At most 16 approved sources");
        }
        let view = SourceView {
            id: Uuid::new_v4(),
            generation: 1,
            geometry_version: "1".into(),
            label: source.label.clone(),
            kind: source.native.kind().into(),
            rect: source.rect,
            control_allowed: control,
        };
        inner.sources.insert(
            view.id,
            Approved {
                view: view.clone(),
                local: source,
                until: Instant::now() + Duration::from_secs(8 * 3600),
            },
        );
        Ok(view)
    }
    pub fn get(&self, id: Uuid, p: &Principal) -> Result<Approved, ErrorCode> {
        if !self.config.enabled {
            return Err(ErrorCode::MediaUnavailable);
        }
        let source = self
            .inner
            .lock()
            .sources
            .get(&id)
            .filter(|s| s.until > Instant::now())
            .cloned()
            .ok_or(ErrorCode::Forbidden)?;
        p.require(if source.view.kind == "window" {
            Scope::WindowView
        } else {
            Scope::DesktopView
        })?;
        Ok(source)
    }
    fn begin(&self, p: &Principal, id: Uuid) -> Result<(Uuid, CancellationToken), ErrorCode> {
        self.get(id, p)?;
        let mut i = self.inner.lock();
        if i.active.is_some() {
            return Err(ErrorCode::LeaseBusy);
        }
        let stream = Uuid::new_v4();
        let cancel = CancellationToken::new();
        i.active = Some(Active {
            id: stream,
            client: p.client_id,
            cancel: cancel.clone(),
        });
        Ok((stream, cancel))
    }
    fn finish(&self, id: Uuid) {
        let mut i = self.inner.lock();
        if i.active.as_ref().is_some_and(|x| x.id == id) {
            i.active = None;
        }
    }
    pub fn cancel_all(&self) {
        if let Some(a) = &self.inner.lock().active {
            a.cancel.cancel();
        }
    }
    pub fn revoke_client(&self, id: Uuid) {
        if let Some(a) = &self.inner.lock().active {
            if a.client == id {
                a.cancel.cancel();
            }
        }
    }
    pub fn clear_sources(&self) {
        let mut i = self.inner.lock();
        i.sources.clear();
        if let Some(a) = &i.active {
            a.cancel.cancel();
        }
    }
    fn public_sources(&self, p: &Principal) -> Vec<SourceView> {
        self.inner
            .lock()
            .sources
            .values()
            .filter(|s| {
                s.until > Instant::now()
                    && p.require(if s.view.kind == "window" {
                        Scope::WindowView
                    } else {
                        Scope::DesktopView
                    })
                    .is_ok()
            })
            .map(|s| s.view.clone())
            .collect()
    }
    async fn capabilities(&self) -> serde_json::Value {
        use serde_json::json;
        if !self.config.enabled {
            return json!({"available":false,"reason":"Media disabled; terminal-only mode loads no media SDK"});
        }
        let mut cache = self.probe.lock().await;
        if let Some((at, value)) = &*cache {
            if at.elapsed() < Duration::from_secs(60) {
                return value.clone();
            }
        }
        let result = async {
            let path = self.config.helper()?;
            let mut c = tokio::process::Command::new(&path);
            configure_helper_command(&mut c, &path, "--probe");
            c.kill_on_drop(true);
            let out = tokio::time::timeout(Duration::from_secs(5), c.output()).await??;
            if !out.status.success() || out.stdout.len() > 32768 {
                anyhow::bail!("Media helper probe failed");
            }
            let value: serde_json::Value = serde_json::from_slice(&out.stdout)?;
            Ok::<_, anyhow::Error>(value)
        }
        .await;
        let v = match result {
            Ok(v) => {
                json!({"available":v["available"].as_bool().unwrap_or(false),"native":v,"window_control_configured":self.config.allow_control,"single_video_viewer":true,"windows_runtime_verified":false})
            }
            Err(e) => json!({"available":false,"reason":e.to_string()}),
        };
        *cache = Some((Instant::now(), v.clone()));
        v
    }
}
pub async fn capabilities(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, Failure> {
    principal(&s, &h, Scope::TerminalRead)?;
    Ok(Json(s.media.capabilities().await))
}
pub async fn sources(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<Vec<SourceView>>, Failure> {
    let p = principal(&s, &h, Scope::TerminalRead)?;
    Ok(Json(s.media.public_sources(&p)))
}
pub async fn upgrade(
    State(s): State<Shared>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    if !h
        .get("origin")
        .and_then(|x| x.to_str().ok())
        .is_some_and(|x| s.config.origin_allowed(x))
    {
        return Err(ErrorCode::OriginRejected.into());
    }
    let (p, id) = s.auth.consume_ws(&crate::ws::ticket(&h)?, "media")?;
    let id = id.ok_or(ErrorCode::InvalidRequest)?;
    let source = s.media.get(id, &p)?;
    Ok(ws
        .protocols(["rctm.v1"])
        .max_message_size(MAX_SIGNAL)
        .max_frame_size(MAX_SIGNAL)
        .on_upgrade(move |socket| run(s, p, source, socket)))
}
#[cfg(not(windows))]
async fn run(_: Shared, _: Principal, _: Approved, mut socket: WebSocket) {
    let _ = socket
        .send(Message::Text(
            "{\"type\":\"error\",\"code\":\"WINDOWS_REQUIRED\"}".into(),
        ))
        .await;
}

#[cfg(windows)]
async fn run(state: Shared, mut p: Principal, source: Approved, socket: WebSocket) {
    use futures_util::{SinkExt, StreamExt};
    use rc_platform_windows::gui_session::GuiSession;
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        sync::mpsc,
    };
    let (mut sink, mut client) = socket.split();
    macro_rules! send {
        ($v:expr) => {{
            tokio::time::timeout(
                Duration::from_millis(500),
                sink.send(Message::Text(($v).to_string().into())),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Media client too slow"))??;
        }};
    }
    let begun = state.media.begin(&p, source.view.id);
    let (stream_id, cancel) = match begun {
        Ok(x) => x,
        Err(e) => {
            let _ = sink
                .send(Message::Text(
                    serde_json::json!({"type":"error","code":e})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };
    struct ActiveGuard(MediaManager, Uuid);
    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            self.0.finish(self.1);
        }
    }
    let _active = ActiveGuard(state.media.clone(), stream_id);
    let config = state.config.media.clone();
    let helper = config.helper();
    let startup = async {
        let local = source.local.clone();
        let gui = tokio::task::spawn_blocking(move || GuiSession::start(local)).await??;
        let helper = helper?;
        let mut command = tokio::process::Command::new(&helper);
        configure_helper_command(&mut command, &helper, "--stdio");
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Missing helper stdin"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Missing helper stdout"))?;
        Ok::<_, anyhow::Error>((Arc::new(gui), child, input, output))
    }
    .await;
    let (gui, mut child, mut input, output) = match startup {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!(error=%e,"Media startup failed");
            let _ = sink
                .send(Message::Text(
                    serde_json::json!({"type":"error","code":"MEDIA_START_FAILED"})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };
    let (tx, mut events) = mpsc::channel::<HelperOut>(64);
    let reader = tokio::spawn(async move {
        let mut r = BufReader::new(output);
        loop {
            let mut bytes = Vec::new();
            let n = (&mut r)
                .take((MAX_SIGNAL + 1) as u64)
                .read_until(b'\n', &mut bytes)
                .await?;
            if n == 0 {
                break;
            }
            if n > MAX_SIGNAL || bytes.last() != Some(&b'\n') {
                anyhow::bail!("Helper frame size exceeded");
            }
            let event: HelperOut = serde_json::from_slice(&bytes)?;
            if tx.send(event).await.is_err() {
                break;
            }
        }
        Ok::<_, anyhow::Error>(())
    });
    async fn helper_send(
        input: &mut tokio::process::ChildStdin,
        message: &HelperIn,
    ) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut b = serde_json::to_vec(message)?;
        if b.len() > MAX_SIGNAL - 1 {
            anyhow::bail!("Signaling too large");
        }
        b.push(b'\n');
        tokio::time::timeout(Duration::from_millis(500), input.write_all(&b)).await??;
        Ok(())
    }
    let outcome=async{
  helper_send(&mut input,&HelperIn::Init{config:MediaInit{protocol:1,source:source.local.clone(),profile:config.profile,tailnet_ip:config.tailnet_ip.clone().ok_or_else(||anyhow::anyhow!("Missing tailnet address"))?,min_port:config.min_port,max_port:config.max_port}}).await?;
  send!(serde_json::json!({"type":"hello","protocol":1,"stream_id":stream_id,"source":source.view,"input_mode":"view_only"}));
  let mut tick=tokio::time::interval(Duration::from_millis(100));tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let mut last_encoded=Instant::now()-Duration::from_secs(60);let mut last_presented=last_encoded;let mut lease_until=last_encoded;
  let mut lease:Option<u64>=None;let mut last_seq=0u64;let mut last_frame_seq=0u64;let mut input_window=Instant::now();let mut input_count=0;
  let mut helper_control_allowed=false;let mut helper_capture_state=CaptureState::Starting;
  let started=Instant::now();let mut negotiated=false;
  loop{tokio::select!{
   biased;
   _=state.shutdown.cancelled()=>break,_=cancel.cancelled()=>break,_=p.revoked.cancelled()=>break,
   _=tick.tick()=>{
    if p.require(Scope::TerminalRead).is_err()||source.until<=Instant::now()||gui.closed(){break;}
    if rc_platform_windows::validate_source(&source.local).map_or(true,|r|r!=source.local.rect){send!(serde_json::json!({"type":"error","code":"SOURCE_CHANGED_REOPEN_REQUIRED"}));break;}
    if !negotiated&&started.elapsed()>Duration::from_secs(20){send!(serde_json::json!({"type":"error","code":"WEBRTC_NEGOTIATION_TIMEOUT"}));break;}
    if lease.is_some()&&(!helper_control_ready(helper_control_allowed,helper_capture_state,last_encoded,last_presented,Instant::now())||lease_until<=Instant::now()||!gui.armed()||lease!=Some(gui.revision())){gui.disarm();state.media.gui_controlled.store(false,Ordering::Release);lease=None;send!(serde_json::json!({"type":"lease","lease_epoch":null,"reason":"LOCAL_INPUT_OR_STALE_FRAME_OR_EXPIRED"}));}
   },
   event=events.recv()=>match event{
    Some(HelperOut::Frame{sequence,captured_at_ms})=>{let seq=sequence.parse::<u64>()?;if seq<=last_frame_seq{anyhow::bail!("Non-monotonic encoded frame");}let age_ms=capture_age_ms(rc_platform_windows::monotonic_millis(),captured_at_ms).map_err(anyhow::Error::msg)?;last_frame_seq=seq;let now=Instant::now();last_encoded=now.checked_sub(Duration::from_millis(age_ms)).unwrap_or(now);},
    Some(HelperOut::Offer{sdp})=>send!(serde_json::json!({"type":"offer","sdp":sanitize_sdp(&sdp).map_err(anyhow::Error::msg)?})),
    Some(HelperOut::Ice{candidate,mline})=>{if mline!=0||!candidate_allowed(&candidate,config.tailnet_ip.as_deref()){anyhow::bail!("Unexpected helper ICE");}send!(serde_json::json!({"type":"ice","candidate":candidate,"mline":mline}));},
    Some(HelperOut::Ready{protocol})=>{if protocol!=1{anyhow::bail!("Helper protocol mismatch");}},
    Some(HelperOut::Status{mode,capture_state,fps,bitrate_kbps,adaptive_idle,control_allowed,awaiting_fresh_frame})=>{
      helper_capture_state=capture_state;helper_control_allowed=control_allowed&&!awaiting_fresh_frame&&capture_state==CaptureState::Live;
      if !helper_control_allowed { gui.disarm(); state.media.gui_controlled.store(false,Ordering::Release); lease=None; }
      send!(serde_json::json!({"type":"status","mode":mode,"capture_state":capture_state,"fps":fps,"bitrate_kbps":bitrate_kbps,"adaptive_idle":adaptive_idle,"control_allowed":helper_control_allowed,"awaiting_fresh_frame":awaiting_fresh_frame}));
    },
    Some(HelperOut::Error{code})=>{send!(serde_json::json!({"type":"error","code":code}));break;},
    Some(HelperOut::Stopped)|None=>break,
   },
   frame=client.next()=>{
    let raw=match frame{Some(Ok(Message::Text(raw)))=>raw,Some(Ok(Message::Ping(v)))=>{sink.send(Message::Pong(v)).await?;continue;},_=>break};
    if input_window.elapsed()>=Duration::from_secs(1){input_window=Instant::now();input_count=0;}input_count+=1;if input_count>500{anyhow::bail!("Media input rate exceeded");}
    let message:MediaClientMessage=serde_json::from_str(&raw)?;
    match message{
     MediaClientMessage::Answer{sdp}=>{if negotiated{anyhow::bail!("Duplicate answer");}let sdp=sanitize_sdp(&sdp).map_err(anyhow::Error::msg)?;helper_send(&mut input,&HelperIn::Answer{sdp}).await?;negotiated=true;},
     MediaClientMessage::Ice{candidate,mline}=>{if mline!=0||!config.allowed_peer_ips.iter().any(|ip|candidate_allowed(&candidate,Some(ip))){anyhow::bail!("Candidate is outside explicitly approved tailnet peers");}helper_send(&mut input,&HelperIn::Ice{candidate,mline}).await?;},
     MediaClientMessage::Presented{generation,geometry_version}=>{if generation!=source.view.generation||geometry_version!=source.view.geometry_version{anyhow::bail!("Stale presentation geometry");}if last_encoded.elapsed()<=Duration::from_millis(500){last_presented=Instant::now();gui.frame()?;}},
     MediaClientMessage::Acquire{generation,geometry_version}=>{
      let scope=if source.view.kind=="window"{Scope::WindowControl}else{Scope::DesktopControl};
      if !helper_control_ready(helper_control_allowed,helper_capture_state,last_encoded,last_presented,Instant::now())||!config.allow_control||!source.view.control_allowed||p.require(scope).is_err()||generation!=source.view.generation||geometry_version!=source.view.geometry_version{send!(serde_json::json!({"type":"error","code":"GUI_CONTROL_NOT_READY_OR_FORBIDDEN"}));continue;}
      let g=gui.clone();match tokio::task::spawn_blocking(move||g.arm()).await?{Ok(n)=>{lease=Some(n);state.media.gui_controlled.store(true,Ordering::Release);lease_until=Instant::now()+Duration::from_secs(15);last_seq=0;send!(serde_json::json!({"type":"lease","lease_epoch":n.to_string()}));},Err(_)=>{gui.disarm();state.media.gui_controlled.store(false,Ordering::Release);lease=None;send!(serde_json::json!({"type":"error","code":"FOREGROUND_OR_NATIVE_SAFETY_DENIED"}));}}
     },
     MediaClientMessage::Renew{lease_epoch}=>{if helper_control_ready(helper_control_allowed,helper_capture_state,last_encoded,last_presented,Instant::now())&&lease.is_some()&&lease==Some(lease_epoch.parse::<u64>()?)&&gui.armed(){lease_until=Instant::now()+Duration::from_secs(15);}},
     MediaClientMessage::Release=>{gui.disarm();state.media.gui_controlled.store(false,Ordering::Release);lease=None;send!(serde_json::json!({"type":"lease","lease_epoch":null}));},
     MediaClientMessage::Input{lease_epoch,generation,geometry_version,seq,action}=>{
      let n=seq.parse::<u64>()?;let e=lease_epoch.parse::<u64>()?;
      if !helper_control_ready(helper_control_allowed,helper_capture_state,last_encoded,last_presented,Instant::now())||lease!=Some(e)||!gui.armed()||lease_until<=Instant::now()||generation!=source.view.generation||geometry_version!=source.view.geometry_version||n<=last_seq{gui.disarm();state.media.gui_controlled.store(false,Ordering::Release);lease=None;send!(serde_json::json!({"type":"lease","lease_epoch":null,"reason":"STALE_INPUT_REJECTED"}));continue;}
      action.validate().map_err(anyhow::Error::msg)?;last_seq=n;gui.action(e,action)?;
     },
     MediaClientMessage::RefreshAuth{ticket}=>{let(next,id)=state.auth.consume_ws(&ticket,"media").map_err(|e|anyhow::anyhow!("{e:?}"))?;if next.client_id!=p.client_id||id!=Some(source.view.id){anyhow::bail!("Refresh identity mismatch");}state.media.get(source.view.id,&next).map_err(|e|anyhow::anyhow!("{e:?}"))?;p=next;},
     MediaClientMessage::Ping=>send!(serde_json::json!({"type":"pong"})),MediaClientMessage::Stop=>break,
    }
   }
  }}Ok::<_,anyhow::Error>(())
 }.await;
    // Every exit, including codec failure, reader failure, revoke and slow client, releases injected keys.
    gui.disarm();
    state.media.gui_controlled.store(false, Ordering::Release);
    let _ = helper_send(&mut input, &HelperIn::Stop).await;
    drop(input);
    if tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    reader.abort();
    drop(gui);
    if let Err(e) = outcome {
        tracing::warn!(error=%e,"Media stream stopped");
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            sink.send(Message::Text(
                serde_json::json!({"type":"error","code":"MEDIA_SESSION_STOPPED"})
                    .to_string()
                    .into(),
            )),
        )
        .await;
    }
    let _ = sink.close().await;
}

#[cfg(test)]
mod tray_state_tests {
    use super::*;

    #[test]
    fn helper_control_predicate_requires_live_helper_authority() {
        let now = Instant::now();
        assert!(helper_control_ready(
            true,
            CaptureState::Live,
            now,
            now,
            now
        ));
        for state in [CaptureState::Starting, CaptureState::MinimizedOrStalled] {
            assert!(!helper_control_ready(true, state, now, now, now));
        }
        assert!(!helper_control_ready(
            false,
            CaptureState::Live,
            now,
            now,
            now
        ));
        assert!(!helper_control_ready(
            true,
            CaptureState::Live,
            now - Duration::from_millis(501),
            now,
            now
        ));
        assert!(!helper_control_ready(
            true,
            CaptureState::Live,
            now,
            now - Duration::from_millis(501),
            now
        ));
    }

    #[test]
    fn capture_timestamp_rejects_future_and_stale_frames() {
        assert_eq!(capture_age_ms(10_000, 9_500), Ok(500));
        assert_eq!(
            capture_age_ms(10_000, 10_001),
            Err("Captured frame timestamp is in the future")
        );
        assert_eq!(
            capture_age_ms(70_001, 10_000),
            Err("Captured frame age exceeded safety bound")
        );
    }

    #[test]
    fn gui_control_state_starts_clear_and_is_shared_by_clones() {
        let media = MediaManager::new(MediaConfig::default());
        let clone = media.clone();
        assert!(!clone.gui_controlled());
        media.gui_controlled.store(true, Ordering::Release);
        assert!(clone.gui_controlled());
    }

    #[test]
    fn installed_media_helper_wins_over_source_layout() {
        let exe = PathBuf::from(r"C:\Program Files\RemoteCodex\rc-agent.exe");
        assert_eq!(
            default_helper_from(&exe, true),
            PathBuf::from(r"C:\Program Files\RemoteCodex\media-runtime\bin\rc-media.exe")
        );
        assert_eq!(
            default_helper_from(&exe, false),
            PathBuf::from(r"C:\Program Files\RemoteCodex\rc-media.exe")
        );
    }

    #[test]
    fn bundled_runtime_is_derived_only_from_packaged_helper_layout() {
        let packaged = PathBuf::from(r"C:\RemoteCodex\media-runtime\bin\rc-media.exe");
        assert_eq!(
            bundled_runtime(&packaged),
            Some(PathBuf::from(r"C:\RemoteCodex\media-runtime"))
        );
        assert_eq!(
            bundled_runtime(&PathBuf::from(r"C:\dev\target\release\rc-media.exe")),
            None
        );
    }
}
