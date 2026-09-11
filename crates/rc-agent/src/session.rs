use crate::{
    auth::Principal,
    config::Config,
    profiles,
    store::Store,
    terminal::{Size, TerminalModel},
};
use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use rc_core::{
    error::ErrorCode,
    frame::{self, Frame},
    input::InputLedger,
    lease::Lease,
    protocol::*,
    ring::EventRing,
    vt_fence::VtFence,
};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

pub struct SessionManager {
    pub epoch: Uuid,
    sessions: Mutex<HashMap<Uuid, Arc<Session>>>,
    config: Config,
    store: Arc<Store>,
    pub events: broadcast::Sender<serde_json::Value>,
}
struct OutputState {
    model: TerminalModel,
    fence: VtFence,
    ring: EventRing,
    seq: u64,
}
struct InputState {
    lease: Lease,
    ledger: InputLedger,
}
struct InputJob {
    p: Principal,
    connection: Uuid,
    epoch: u64,
    id: Uuid,
    data: Vec<u8>,
    reply: oneshot::Sender<Result<u64, ErrorCode>>,
    received: Instant,
    bulk: bool,
    bracketed: bool,
    append_enter: bool,
}
pub struct Session {
    meta: Mutex<SessionInfo>,
    output: Mutex<OutputState>,
    input: Mutex<InputState>,
    resize_guard: Mutex<()>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    writer: Sender<InputJob>,
    interrupt: Sender<InputJob>,
    stop: Sender<()>,
    running: AtomicBool,
    pub output_events: broadcast::Sender<Arc<Frame>>,
    events: broadcast::Sender<serde_json::Value>,
    store: Arc<Store>,
    managed_codex: bool,
}
impl SessionManager {
    pub fn new(config: Config, store: Arc<Store>, epoch: Uuid) -> Self {
        let (events, _) = broadcast::channel(128);
        Self {
            epoch,
            sessions: Mutex::new(HashMap::new()),
            config,
            store,
            events,
        }
    }
    pub fn get(&self, id: Uuid) -> Result<Arc<Session>, ErrorCode> {
        self.sessions
            .lock()
            .get(&id)
            .cloned()
            .ok_or(ErrorCode::SessionLost)
    }
    pub fn rename(&self, p: &Principal, id: Uuid, label: String) -> Result<SessionInfo, ErrorCode> {
        p.require(Scope::TerminalCreate)?;
        let label = label.trim();
        if label.is_empty() || label.len() > 160 {
            return Err(ErrorCode::InvalidRequest);
        }
        self.get(id)?.rename_label(label)
    }
    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions.lock().values().map(|s| s.info()).collect()
    }
    pub fn create(&self, req: CreateSession) -> Result<SessionInfo, ErrorCode> {
        if req.label.trim().is_empty()
            || req.label.len() > 160
            || req.cols < 2
            || req.cols > MAX_COLS
            || req.rows < 1
            || req.rows > MAX_ROWS
        {
            return Err(ErrorCode::InvalidRequest);
        }
        let canonical_cwd = PathBuf::from(&req.cwd)
            .canonicalize()
            .map_err(|_| ErrorCode::InvalidRequest)?;
        let cwd = normalize_pty_cwd(&canonical_cwd)?;
        if !cwd.is_dir() {
            return Err(ErrorCode::InvalidRequest);
        }
        let mut sessions = self.sessions.lock();
        if sessions
            .values()
            .filter(|s| s.running.load(Ordering::Acquire))
            .count()
            >= self.config.max_sessions
        {
            return Err(ErrorCode::RateLimited);
        }
        // Archive completed sessions in SQLite instead of retaining unbounded live models.
        sessions.retain(|_, s| s.running.load(Ordering::Acquire));
        let project_id = self
            .store
            .project_for_cwd(&cwd.to_string_lossy(), req.project_id)
            .map_err(|_| ErrorCode::InvalidRequest)?;
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: req.rows,
                cols: req.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| ErrorCode::Internal)?;
        let launch = profiles::resolve(&req.profile)?;
        let mut cmd = CommandBuilder::new(launch.program.clone());
        cmd.args(launch.args.clone());
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("REMOTECODEX_") {
                cmd.env_remove(k);
            }
        }
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| ErrorCode::Internal)?;
        let writer = pair.master.take_writer().map_err(|_| ErrorCode::Internal)?;
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|_| ErrorCode::InvalidRequest)?;
        let pid = child.process_id();
        let created = pid.and_then(|p| rc_platform_windows::process_created(p).ok());
        if launch.managed_codex && (pid.is_none() || created.is_none()) {
            let _ = child.kill();
            return Err(ErrorCode::Internal);
        }
        let killer = child.clone_killer();
        drop(pair.slave);
        let (tx, rx) = crossbeam_channel::bounded(64);
        let (itx, irx) = crossbeam_channel::bounded(8);
        let (stx, srx) = crossbeam_channel::bounded(1);
        let (qtx, qrx) = crossbeam_channel::bounded(128);
        let (output_events, _) = broadcast::channel(64);
        let id = Uuid::new_v4();
        let meta = SessionInfo {
            session_id: id,
            project_id: Some(project_id),
            label: req.label.trim().into(),
            initial_cwd: cwd.to_string_lossy().into(),
            profile: req.profile,
            agent_epoch: self.epoch,
            generation: 1,
            state: Lifecycle::Running,
            cols: req.cols,
            rows: req.rows,
            output_seq: "0".into(),
            pid,
            process_created: created.map(|v| v.to_string()),
            program_path: Some(launch.program.to_string_lossy().into_owned()),
            last_activity_at: Some(activity_timestamp()),
            composer_allowed: false,
            lease: None,
        };
        let s = Arc::new(Session {
            meta: Mutex::new(meta),
            output: Mutex::new(OutputState {
                model: TerminalModel::new(req.cols, req.rows, qtx),
                fence: VtFence::default(),
                ring: EventRing::new(self.config.ring_bytes),
                seq: 0,
            }),
            input: Mutex::new(InputState {
                lease: Lease::default(),
                ledger: InputLedger::default(),
            }),
            resize_guard: Mutex::new(()),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(killer),
            writer: tx,
            interrupt: itx,
            stop: stx,
            running: AtomicBool::new(true),
            output_events,
            events: self.events.clone(),
            store: self.store.clone(),
            managed_codex: launch.managed_codex,
        });
        let weak = Arc::downgrade(&s);
        std::thread::Builder::new()
            .name(format!("pty-write-{id}"))
            .stack_size(512 * 1024)
            .spawn(move || writer_loop(weak, writer, rx, irx, qrx, srx))
            .map_err(|_| ErrorCode::Internal)?;
        let weak = Arc::downgrade(&s);
        std::thread::Builder::new()
            .name(format!("pty-read-{id}"))
            .stack_size(1024 * 1024)
            .spawn(move || reader_loop(weak, reader))
            .map_err(|_| ErrorCode::Internal)?;
        let weak = Arc::downgrade(&s);
        std::thread::Builder::new()
            .name(format!("pty-wait-{id}"))
            .stack_size(256 * 1024)
            .spawn(move || {
                let exit = child.wait();
                if let Some(s) = weak.upgrade() {
                    s.running.store(false, Ordering::Release);
                    s.input.lock().lease.revoke_all();
                    s.meta.lock().state = if exit.is_ok() {
                        Lifecycle::Exited
                    } else {
                        Lifecycle::Lost
                    };
                    let _ = s.stop.try_send(());
                    s.changed("exited");
                    let _ = s.store.save_session(&s.info());
                }
            })
            .map_err(|_| ErrorCode::Internal)?;
        let info = s.info();
        if self.store.save_session(&info).is_err() {
            s.close();
            return Err(ErrorCode::Internal);
        }
        sessions.insert(id, s.clone());
        s.changed("created");
        Ok(info)
    }
    pub fn revoke_connection(&self, id: Uuid) {
        for s in self.sessions.lock().values() {
            s.input.lock().lease.revoke_connection(id);
            s.changed("lease");
        }
    }
    pub fn revoke_client(&self, id: Uuid) {
        for s in self.sessions.lock().values() {
            s.input.lock().lease.revoke_client(id);
            s.changed("lease");
        }
    }
    pub fn block_input(&self) {
        for s in self.sessions.lock().values() {
            s.input.lock().lease.revoke_all();
            s.changed("lease");
        }
    }
    pub fn shutdown(&self) {
        for s in self.sessions.lock().values() {
            s.close();
        }
    }
}
impl Session {
    pub fn readable_projection(&self) -> serde_json::Value {
        let o = self.output.lock();
        let m = self.meta.lock();
        serde_json::json!({
            "session_id": m.session_id,
            "agent_epoch": m.agent_epoch,
            "generation": m.generation,
            "sequence": o.seq.to_string(),
            "projection": o.model.readable_projection()
        })
    }
    pub fn info(&self) -> SessionInfo {
        let o = self.output.lock();
        let mut m = self.meta.lock().clone();
        m.output_seq = o.seq.to_string();
        m.lease = self.input.lock().lease.view(Instant::now());
        m.composer_allowed = self.managed_codex && self.identity_matches(&m);
        m
    }
    fn identity_matches(&self, meta: &SessionInfo) -> bool {
        let current_created = meta
            .pid
            .and_then(|pid| rc_platform_windows::process_created(pid).ok());
        let current_program = meta
            .pid
            .and_then(|pid| rc_platform_windows::process_image_path(pid).ok());
        identity_matches_values(
            self.running.load(Ordering::Acquire),
            meta.pid,
            meta.process_created.as_deref(),
            meta.program_path.as_deref(),
            current_created,
            current_program.as_deref(),
        )
    }
    fn changed(&self, reason: &str) {
        let _=self.events.send(serde_json::json!({"type":"session_changed","session_id":self.meta.lock().session_id,"reason":reason}));
    }
    fn rename_label(&self, label: &str) -> Result<SessionInfo, ErrorCode> {
        let mut meta = self.meta.lock();
        if !self.running.load(Ordering::Acquire) {
            return Err(ErrorCode::SessionLost);
        }
        self.store
            .rename_session_label(meta.session_id, label)
            .map_err(|_| ErrorCode::Internal)?;
        meta.label = label.to_owned();
        drop(meta);
        self.changed("renamed");
        Ok(self.info())
    }
    pub fn acquire(&self, p: &Principal, c: Uuid, takeover: bool) -> Result<LeaseView, ErrorCode> {
        p.require(Scope::TerminalWrite)?;
        if !self.running.load(Ordering::Acquire) {
            return Err(ErrorCode::SessionLost);
        }
        let v = self
            .input
            .lock()
            .lease
            .acquire(p.client_id, c, takeover, Instant::now())?;
        self.changed("lease");
        Ok(v)
    }
    pub fn renew(&self, p: &Principal, c: Uuid, epoch: u64) -> Result<LeaseView, ErrorCode> {
        p.require(Scope::TerminalWrite)?;
        self.input
            .lock()
            .lease
            .renew(p.client_id, c, epoch, Instant::now())
    }
    pub fn release(&self, p: &Principal, c: Uuid, epoch: u64) -> Result<(), ErrorCode> {
        p.require(Scope::TerminalWrite)?;
        self.input
            .lock()
            .lease
            .release(p.client_id, c, epoch, Instant::now())?;
        self.changed("lease");
        Ok(())
    }
    pub fn validate_generation(&self, agent: Uuid, generation: u32) -> Result<(), ErrorCode> {
        let m = self.meta.lock();
        if m.agent_epoch != agent {
            return Err(ErrorCode::HostRestarted);
        }
        if m.generation != generation {
            return Err(ErrorCode::SessionGenerationMismatch);
        }
        if !self.running.load(Ordering::Acquire) {
            return Err(ErrorCode::SessionLost);
        }
        Ok(())
    }
    pub fn enqueue(
        &self,
        p: Principal,
        c: Uuid,
        lease: u64,
        id: Uuid,
        seq: u64,
        data: Vec<u8>,
        received: Instant,
    ) -> Result<oneshot::Receiver<Result<u64, ErrorCode>>, ErrorCode> {
        p.require(Scope::TerminalWrite)?;
        let mut state = self.input.lock();
        state.lease.verify(p.client_id, c, lease, Instant::now())?;
        state.ledger.accept(c, lease, id, seq, data.len())?;
        let priority = data.as_slice() == b"\x03";
        let (tx, rx) = oneshot::channel();
        let job = InputJob {
            p,
            connection: c,
            epoch: lease,
            id,
            data,
            reply: tx,
            received,
            bulk: false,
            bracketed: false,
            append_enter: false,
        };
        if (if priority {
            &self.interrupt
        } else {
            &self.writer
        })
        .try_send(job)
        .is_err()
        {
            state.ledger.finish(id, "rejected");
            return Err(ErrorCode::RateLimited);
        }
        Ok(rx)
    }
    pub fn enqueue_paste(
        &self,
        p: Principal,
        c: Uuid,
        lease: u64,
        id: Uuid,
        seq: u64,
        text: String,
        append_enter: bool,
        received: Instant,
    ) -> Result<oneshot::Receiver<Result<u64, ErrorCode>>, ErrorCode> {
        p.require(Scope::TerminalWrite)?;
        if text.is_empty()
            || text.len() > MAX_PASTE
            || text
                .bytes()
                .any(|b| b < 32 && !matches!(b, 9 | 10 | 13) || b == 127)
        {
            return Err(ErrorCode::InvalidRequest);
        }
        let bracketed = self.output.lock().model.bracketed_paste();
        let data = text.replace("\r\n", "\r").replace('\n', "\r").into_bytes();
        let mut state = self.input.lock();
        state.lease.verify(p.client_id, c, lease, Instant::now())?;
        state.ledger.accept_paste(c, lease, id, seq, data.len())?;
        let (tx, rx) = oneshot::channel();
        let job = InputJob {
            p,
            connection: c,
            epoch: lease,
            id,
            data,
            reply: tx,
            received,
            bulk: true,
            bracketed,
            append_enter,
        };
        if self.writer.try_send(job).is_err() {
            state.ledger.finish(id, "rejected");
            return Err(ErrorCode::RateLimited);
        }
        Ok(rx)
    }
    pub fn resize(
        &self,
        p: &Principal,
        c: Uuid,
        lease: u64,
        cols: u16,
        rows: u16,
    ) -> Result<(), ErrorCode> {
        // Serialize resizes without holding the terminal model lock during the blocking
        // ConPTY API. The PTY reader continues draining while Windows applies the size.
        let _serial = self.resize_guard.lock();
        p.require(Scope::TerminalWrite)?;
        if cols < 2 || cols > MAX_COLS || rows < 1 || rows > MAX_ROWS {
            return Err(ErrorCode::InvalidRequest);
        }
        self.input
            .lock()
            .lease
            .verify(p.client_id, c, lease, Instant::now())?;
        if !self.running.load(Ordering::Acquire) {
            return Err(ErrorCode::SessionLost);
        }
        let old = self.meta.lock().clone();
        if old.cols == cols && old.rows == rows {
            return Ok(());
        }
        let owner = self.master.lock();
        let master = owner.as_ref().ok_or(ErrorCode::SessionLost)?;
        // The host canonical stream defines the resize boundary, just as a local
        // terminal does. All clients see the ordered size event before later bytes.
        self.commit_resize(cols, rows);
        let result = master.resize(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        });
        if result.is_err() {
            self.commit_resize(old.cols, old.rows);
        }
        drop(owner);
        self.changed("resized");
        result.map_err(|_| ErrorCode::Internal)
    }
    fn commit_resize(&self, cols: u16, rows: u16) {
        let mut o = self.output.lock();
        o.model.resize(Size { cols, rows });
        o.seq += 1;
        let seq = o.seq;
        let mut m = self.meta.lock();
        m.cols = cols;
        m.rows = rows;
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&cols.to_be_bytes());
        payload.extend_from_slice(&rows.to_be_bytes());
        let event = o.ring.push(Frame {
            kind: frame::RESIZE,
            session_id: m.session_id,
            generation: m.generation,
            sequence: seq,
            payload: Bytes::from(payload),
        });
        let _ = self.output_events.send(event);
    }
    pub fn subscribe_snapshot(
        &self,
    ) -> Result<(SnapshotMeta, Vec<u8>, broadcast::Receiver<Arc<Frame>>), ErrorCode> {
        let m = self.meta.lock().clone();
        let o = self.output.lock();
        let rx = self.output_events.subscribe();
        let data = o.model.snapshot();
        if data.len() > 4 * 1024 * 1024 {
            return Err(ErrorCode::FrameTooLarge);
        }
        let size = o.model.size();
        let meta = SnapshotMeta {
            cols: size.cols,
            rows: size.rows,
            sequence: o.seq.to_string(),
            generation: m.generation,
            agent_epoch: m.agent_epoch,
            fidelity: "experimental_vt_snapshot".into(),
            warnings: o.model.warnings.clone(),
            bytes: data.len(),
        };
        Ok((meta, data, rx))
    }
    pub fn ring_after(&self, seq: u64) -> Result<Vec<Arc<Frame>>, ErrorCode> {
        self.output.lock().ring.after(seq)
    }
    pub fn close(&self) {
        if self.running.swap(false, Ordering::AcqRel) {
            self.input.lock().lease.revoke_all();
            self.meta.lock().state = Lifecycle::Closing;
            let _ = self.killer.lock().kill();
            let _ = self.stop.try_send(());
        }
        // Closing the owned pseudoconsole is explicit. Viewer/UI disconnect never calls this.
        drop(self.master.lock().take());
        self.changed("closed");
    }
}
fn reader_loop(session: Weak<Session>, mut reader: Box<dyn Read + Send>) {
    let mut buf = vec![0u8; frame::MAX_PAYLOAD];
    loop {
        let count = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let Some(s) = session.upgrade() else { break };
        let m = s.meta.lock().clone();
        let mut o = s.output.lock();
        let tokens = o.fence.push_bytes(&buf[..count]);
        let mut batch = Vec::new();
        for token in tokens {
            o.model.apply(&token);
            if batch.len() + token.len() > frame::MAX_PAYLOAD && !batch.is_empty() {
                emit(&s, &m, &mut o, std::mem::take(&mut batch));
            }
            batch.extend_from_slice(&token);
        }
        if !batch.is_empty() {
            emit(&s, &m, &mut o, batch);
        }
    }
}
fn emit(s: &Session, m: &SessionInfo, o: &mut OutputState, data: Vec<u8>) {
    o.seq += 1;
    s.meta.lock().last_activity_at = Some(activity_timestamp());
    let seq = o.seq;
    let event = o.ring.push(Frame {
        kind: frame::OUTPUT,
        session_id: m.session_id,
        generation: m.generation,
        sequence: seq,
        payload: Bytes::from(data),
    });
    let _ = s.output_events.send(event);
}
fn writer_loop(
    s: Weak<Session>,
    mut writer: Box<dyn Write + Send>,
    normal: Receiver<InputJob>,
    interrupt: Receiver<InputJob>,
    queries: Receiver<Vec<u8>>,
    stop: Receiver<()>,
) {
    loop {
        crossbeam_channel::select_biased! {
            recv(stop)->_=>break,
            recv(interrupt)->job=>{let Ok(job)=job else{break};
                // An interrupt cancels queued, not-yet-written input. Never replay it later.
                while let Ok(pending)=normal.try_recv(){if let Some(s)=s.upgrade(){s.input.lock().ledger.finish(pending.id,"rejected");}let _=pending.reply.send(Err(ErrorCode::InputCanceled));}
                write_job(&s,&mut writer,job,&normal,&interrupt);
            },
            recv(queries)->query=>{if let Ok(q)=query{let _=writer.write_all(&q);}else{break;}},
            recv(normal)->job=>{match job{Ok(j)=>write_job(&s,&mut writer,j,&normal,&interrupt),Err(_)=>break}},
        }
    }
    for pending in normal.try_iter().chain(interrupt.try_iter()) {
        let _ = pending.reply.send(Err(ErrorCode::InputDeliveryUnknown));
    }
}
fn write_job(
    weak: &Weak<Session>,
    writer: &mut Box<dyn Write + Send>,
    job: InputJob,
    normal: &Receiver<InputJob>,
    interrupt: &Receiver<InputJob>,
) {
    let Some(s) = weak.upgrade() else {
        let _ = job.reply.send(Err(ErrorCode::SessionLost));
        return;
    };
    let permitted = || {
        job.p.require(Scope::TerminalWrite).and_then(|_| {
            if !s.running.load(Ordering::Acquire) {
                return Err(ErrorCode::SessionLost);
            }
            s.input
                .lock()
                .lease
                .verify(job.p.client_id, job.connection, job.epoch, Instant::now())
        })
    };
    let mut prefix_written = false;
    let mut urgent = None;
    let result = (|| {
        permitted()?;
        if job.bracketed {
            writer
                .write_all(b"\x1b[200~")
                .map_err(|_| ErrorCode::InputDeliveryUnknown)?;
            prefix_written = true;
        }
        // A single bounded upload is validated before any PTY bytes are written.
        // Break between chunks for permission changes and priority Ctrl+C.
        for chunk in job.data.chunks(8192) {
            if job.bulk {
                if let Ok(stop) = interrupt.try_recv() {
                    urgent = Some(stop);
                    return Err(ErrorCode::InputCanceled);
                }
            }
            permitted()?;
            writer
                .write_all(chunk)
                .map_err(|_| ErrorCode::InputDeliveryUnknown)?;
        }
        Ok(())
    })();
    // Close only our own bracketed-paste delimiter, including cancellation. No user text is replayed.
    let suffix = if prefix_written {
        writer
            .write_all(b"\x1b[201~")
            .map_err(|_| ErrorCode::InputDeliveryUnknown)
    } else {
        Ok(())
    };
    let mut result = result.and(suffix);
    if result.is_ok() && job.append_enter {
        result = permitted().and_then(|_| {
            writer
                .write_all(b"\r")
                .map_err(|_| ErrorCode::InputDeliveryUnknown)
        });
    }
    let result = result.map(|_| job.received.elapsed().as_micros() as u64);
    if result.is_ok() {
        s.meta.lock().last_activity_at = Some(activity_timestamp());
    }
    s.input.lock().ledger.finish(
        job.id,
        if result.is_ok() {
            "written"
        } else {
            "delivery_unknown"
        },
    );
    let _ = job.reply.send(result);
    if let Some(stop) = urgent {
        while let Ok(pending) = normal.try_recv() {
            s.input.lock().ledger.finish(pending.id, "rejected");
            let _ = pending.reply.send(Err(ErrorCode::InputCanceled));
        }
        write_job(weak, writer, stop, normal, interrupt);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        let _ = self.killer.get_mut().kill();
        self.master.get_mut().take();
    }
}
fn activity_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .to_string()
}

fn identity_matches_values(
    running: bool,
    pid: Option<u32>,
    process_created: Option<&str>,
    program_path: Option<&str>,
    current_created: Option<u64>,
    current_program: Option<&std::path::Path>,
) -> bool {
    if !running || pid.is_none() || process_created.is_none() || current_created.is_none() {
        return false;
    }
    let Some(program_path) = program_path else {
        return false;
    };
    std::path::Path::new(program_path).is_file()
        && current_program
            .is_some_and(|current| current.to_string_lossy().eq_ignore_ascii_case(program_path))
        && current_created
            .map(|value| Some(value.to_string()) == process_created.map(str::to_owned))
            .unwrap_or(false)
}

#[cfg(windows)]
fn normalize_pty_cwd(cwd: &PathBuf) -> Result<PathBuf, ErrorCode> {
    let value = cwd.as_os_str().to_string_lossy();
    let path = if let Some(path) = value.strip_prefix("\\\\?\\") {
        if path
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
        {
            return Err(ErrorCode::InvalidRequest);
        }
        path
    } else {
        if value.starts_with("\\\\") {
            return Err(ErrorCode::InvalidRequest);
        }
        value.as_ref()
    };
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || path.encode_utf16().count() + 1 > 260
    {
        return Err(ErrorCode::InvalidRequest);
    }
    Ok(PathBuf::from(path))
}

#[cfg(not(windows))]
fn normalize_pty_cwd(cwd: &PathBuf) -> Result<PathBuf, ErrorCode> {
    Ok(cwd.clone())
}

#[cfg(all(test, windows))]
mod pty_cwd_tests {
    use super::*;

    #[test]
    fn canonical_temp_directory_is_normalized_for_cmd() {
        let canonical = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let normalized = normalize_pty_cwd(&canonical).unwrap();
        assert!(normalized.is_dir());
        assert!(!normalized.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn unc_paths_are_rejected_instead_of_falling_back() {
        assert_eq!(
            normalize_pty_cwd(&PathBuf::from(r"\\?\UNC\server\share")),
            Err(ErrorCode::InvalidRequest)
        );
        assert_eq!(
            normalize_pty_cwd(&PathBuf::from(r"\\server\share")),
            Err(ErrorCode::InvalidRequest)
        );
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn managed_identity_fails_closed_for_missing_or_stale_identity() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("codex.exe");
        std::fs::write(&program, b"fixture").unwrap();
        let path = program.to_string_lossy();
        assert!(identity_matches_values(
            true,
            Some(42),
            Some("123"),
            Some(&path),
            Some(123),
            Some(program.as_path()),
        ));
        assert!(!identity_matches_values(
            true,
            Some(42),
            Some("123"),
            Some(&path),
            Some(124),
            Some(program.as_path()),
        ));
        assert!(!identity_matches_values(
            true,
            Some(42),
            None,
            Some(&path),
            Some(123),
            Some(program.as_path()),
        ));
        assert!(!identity_matches_values(
            false,
            Some(42),
            Some("123"),
            Some(&path),
            Some(123),
            Some(program.as_path()),
        ));
        assert!(!identity_matches_values(
            true,
            Some(42),
            Some("123"),
            Some("missing-codex.exe"),
            Some(123),
            Some(program.as_path()),
        ));
    }
}
