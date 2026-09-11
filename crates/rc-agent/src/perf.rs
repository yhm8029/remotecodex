use crate::perf_histogram::Histogram;
use anyhow::{bail, Context, Result};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const TRACE_ENV: &str = "RC_PERF_TRACE";
const QUEUE_CAPACITY: usize = 8192;
const CLOSED_BIT: usize = 1usize << (usize::BITS - 1);
const PRODUCER_MASK: usize = CLOSED_BIT - 1;
const FINISH_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_POLL_INTERVAL: Duration = Duration::from_millis(10);

static TRACE: OnceLock<Sink> = OnceLock::new();

/// Starts the opt-in PTY output trace when `RC_PERF_TRACE` names an absolute path.
///
/// The trace contains timing and size metadata only. It never receives PTY bytes.
pub fn init_from_env() -> Result<Option<Guard>> {
    init_from_path(std::env::var_os(TRACE_ENV))
}

fn init_from_path(path: Option<std::ffi::OsString>) -> Result<Option<Guard>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        bail!("{TRACE_ENV} must be an absolute path");
    }
    if TRACE.get().is_some() {
        bail!("performance trace is already initialized");
    }

    let guard = start(&path)?;
    if TRACE.set(guard.sink.clone()).is_err() {
        let _ = guard.finish();
        bail!("performance trace is already initialized");
    }
    Ok(Some(guard))
}

/// Records one metadata-only PTY output event without waiting for the writer.
pub fn record(session_id: Uuid, sequence: u64, bytes: usize, elapsed: Duration) {
    if let Some(sink) = TRACE.get() {
        sink.record(session_id, sequence, bytes, elapsed);
    }
}

/// Owns the trace writer and closes it with a summary footer.
pub struct Guard {
    sink: Sink,
    writer: Option<JoinHandle<io::Result<()>>>,
    finished: bool,
}

impl Guard {
    /// Finishes the trace after all expected PTY producers have stopped.
    pub fn finish(mut self) -> Result<()> {
        self.finish_inner()
    }

    fn record(&self, session_id: Uuid, sequence: u64, bytes: usize, elapsed: Duration) {
        self.sink.record(session_id, sequence, bytes, elapsed);
    }

    fn finish_inner(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;

        let deadline = Instant::now() + FINISH_TIMEOUT;
        let close_result = self.sink.close_producers(deadline);
        let signal_result = self.sink.shutdown.send(()).err();

        let Some(writer) = self.writer.take() else {
            return close_result;
        };
        let writer_result = wait_for_writer(writer, deadline);
        if let Err(error) = writer_result {
            return Err(error);
        }
        if let Err(error) = close_result {
            return Err(error);
        }
        if let Some(error) = signal_result {
            return Err(error).context("performance trace writer stopped before shutdown");
        }
        Ok(())
    }
}

fn wait_for_writer(writer: JoinHandle<io::Result<()>>, deadline: Instant) -> Result<()> {
    while !writer.is_finished() {
        if Instant::now() >= deadline {
            // Dropping JoinHandle detaches a blocked OS write. This is bounded
            // so both explicit finish and Guard::drop can return.
            return Err(anyhow::anyhow!(
                "performance trace writer did not stop within {:?}",
                FINISH_TIMEOUT
            ));
        }
        thread::sleep(Duration::from_millis(1));
    }
    match writer.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error).context("performance trace writer failed"),
        Err(_) => bail!("performance trace writer panicked"),
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.finish_inner();
    }
}

#[derive(Clone)]
struct Sink {
    tx: mpsc::SyncSender<Message>,
    shutdown: mpsc::Sender<()>,
    state: Arc<State>,
}

struct State {
    admission: AtomicUsize,
    dropped: AtomicU64,
}

enum Message {
    Event(Event),
}

/// Deliberately contains no payload and no owned strings. Serialization happens
/// on the writer thread, where the sequence string is formed for JSON output.
struct Event {
    session_id: Uuid,
    sequence: u64,
    bytes: usize,
    elapsed_us: u64,
}

impl Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object = serializer.serialize_struct("Event", 5)?;
        object.serialize_field("stage", "pty_output_receive_to_enqueue")?;
        object.serialize_field("session_id", &self.session_id)?;
        object.serialize_field("sequence", &self.sequence.to_string())?;
        object.serialize_field("bytes", &self.bytes)?;
        object.serialize_field("elapsed_us", &self.elapsed_us)?;
        object.end()
    }
}

#[derive(serde::Serialize)]
struct Summary<'a> {
    r#type: &'a str,
    samples: u64,
    dropped: u64,
    raw_events: u64,
    raw_bytes: u64,
    raw_truncated: bool,
    histogram_bucket_width_us: u64,
    histogram_overflow_bucket_start_us: u64,
    global: &'a Histogram,
    sessions: &'a BTreeMap<Uuid, Histogram>,
    other: &'a Histogram,
    p50_upper_us: Option<u64>,
    p95_upper_us: Option<u64>,
    p99_upper_us: Option<u64>,
}

pub struct TraceWriter {
    writer: BufWriter<std::fs::File>,
    raw_limit: usize,
    raw_bytes: usize,
    raw_events: u64,
    raw_truncated: bool,
    global: Histogram,
    sessions: BTreeMap<Uuid, Histogram>,
    other: Histogram,
}

impl TraceWriter {
    pub fn new(file: std::fs::File, raw_limit: usize) -> Self {
        let limit = raw_limit.min(49 * 1024 * 1024);
        Self {
            writer: BufWriter::new(file),
            raw_limit: limit,
            raw_bytes: 0,
            raw_events: 0,
            raw_truncated: false,
            global: Histogram::new(),
            sessions: BTreeMap::new(),
            other: Histogram::new(),
        }
    }

    pub fn observe(&mut self, event: &Event) -> io::Result<()> {
        self.global.record(event.elapsed_us);
        const MAX_KEYS: usize = 64;
        let sid = event.session_id;
        let needs_new = !self.sessions.contains_key(&sid) && self.sessions.len() >= MAX_KEYS;
        if needs_new {
            self.other.record(event.elapsed_us);
        } else {
            self.sessions
                .entry(sid)
                .or_insert_with(Histogram::new)
                .record(event.elapsed_us);
        }

        if self.raw_truncated {
            return Ok(());
        }

        let mut buf = serde_json::to_vec(event).map_err(io::Error::other)?;
        buf.push(b'\n');
        let len = buf.len();
        if self.raw_bytes + len > self.raw_limit {
            self.raw_truncated = true;
            return Ok(());
        }
        self.writer.write_all(&buf)?;
        self.raw_bytes += len;
        self.raw_events += 1;
        Ok(())
    }

    pub fn finish(mut self, dropped: u64) -> io::Result<()> {
        let p50 = self.global.upper_quantile(0.50);
        let p95 = self.global.upper_quantile(0.95);
        let p99 = self.global.upper_quantile(0.99);
        let summary = Summary {
            r#type: "summary",
            samples: self.global.count,
            dropped,
            raw_events: self.raw_events,
            raw_bytes: self.raw_bytes as u64,
            raw_truncated: self.raw_truncated,
            histogram_bucket_width_us: 100,
            histogram_overflow_bucket_start_us: 100_000,
            global: &self.global,
            sessions: &self.sessions,
            other: &self.other,
            p50_upper_us: p50,
            p95_upper_us: p95,
            p99_upper_us: p99,
        };
        let mut buf = serde_json::to_vec(&summary).map_err(io::Error::other)?;
        buf.push(b'\n');
        if buf.len() > 1024 * 1024 {
            return Err(io::Error::other("summary footer exceeds 1MiB"));
        }
        self.writer.write_all(&buf)?;
        self.writer.flush()
    }
}

fn start(path: &Path) -> Result<Guard> {
    if !path.is_absolute() {
        bail!("performance trace path must be absolute");
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create performance trace at {}", path.display()))?;

    let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
    let (shutdown, shutdown_rx) = mpsc::channel();
    let state = Arc::new(State {
        admission: AtomicUsize::new(0),
        dropped: AtomicU64::new(0),
    });
    let writer_state = Arc::clone(&state);
    let writer = thread::Builder::new()
        .name("rc-perf-trace".to_owned())
        .spawn(move || {
            let result = write_trace(file, rx, shutdown_rx, &writer_state);
            result
        })
        .context("spawn performance trace writer")?;

    Ok(Guard {
        sink: Sink {
            tx,
            shutdown,
            state,
        },
        writer: Some(writer),
        finished: false,
    })
}

impl Sink {
    fn record(&self, session_id: Uuid, sequence: u64, bytes: usize, elapsed: Duration) {
        if !self.admit() {
            // Records arriving after finish closes admission are outside the
            // trace's accounting contract and must not mutate the footer.
            return;
        }

        let event = Event {
            session_id,
            sequence,
            bytes,
            elapsed_us: elapsed.as_micros().min(u64::MAX as u128) as u64,
        };
        match self.tx.try_send(Message::Event(event)) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => {
                self.state.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.release();
    }

    fn admit(&self) -> bool {
        let mut current = self.state.admission.load(Ordering::Acquire);
        loop {
            if current & CLOSED_BIT != 0 || current & PRODUCER_MASK == PRODUCER_MASK {
                return false;
            }
            match self.state.admission.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }

    fn release(&self) {
        self.state.admission.fetch_sub(1, Ordering::Release);
    }

    fn close_producers(&self, deadline: Instant) -> Result<()> {
        self.state.admission.fetch_or(CLOSED_BIT, Ordering::AcqRel);
        while self.state.admission.load(Ordering::Acquire) & PRODUCER_MASK != 0 {
            if Instant::now() >= deadline {
                bail!(
                    "performance trace producers did not stop within {:?}",
                    FINISH_TIMEOUT
                );
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}

fn write_trace(
    file: std::fs::File,
    rx: mpsc::Receiver<Message>,
    shutdown_rx: mpsc::Receiver<()>,
    state: &State,
) -> io::Result<()> {
    let mut writer = TraceWriter::new(file, 49 * 1024 * 1024);
    loop {
        match shutdown_rx.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                drain_events(&mut writer, &rx)?;
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match rx.recv_timeout(WRITER_POLL_INTERVAL) {
            Ok(Message::Event(event)) => writer.observe(&event)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    writer.finish(state.dropped.load(Ordering::Acquire))
}

fn drain_events(writer: &mut TraceWriter, rx: &mpsc::Receiver<Message>) -> io::Result<()> {
    loop {
        match rx.try_recv() {
            Ok(Message::Event(event)) => writer.observe(&event)?,
            Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;

    fn bucket_sum(histogram: &Value) -> u64 {
        histogram["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bucket| bucket.as_u64().unwrap())
            .sum()
    }

    #[test]
    fn writes_metadata_event_and_summary_footer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.jsonl");
        let session = Uuid::from_u128(1);
        let guard = start(&path).unwrap();
        guard.record(session, 7, 12, Duration::from_micros(23));
        guard.finish().unwrap();

        let lines: Vec<Value> = fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["stage"], "pty_output_receive_to_enqueue");
        assert_eq!(lines[0]["session_id"], session.to_string());
        assert_eq!(lines[0]["sequence"], "7");
        assert_eq!(lines[0]["bytes"], 12);
        assert_eq!(lines[0]["elapsed_us"], 23);
        assert!(lines[0].get("payload").is_none());
        assert_eq!(lines[1]["type"], "summary");
        assert_eq!(lines[1]["samples"], 1);
        assert_eq!(lines[1]["dropped"], 0);
        assert_eq!(lines[1]["global"]["count"], 1);
        assert_eq!(lines[1]["p95_upper_us"], 23);
    }

    #[test]
    fn refuses_to_overwrite_an_existing_trace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.jsonl");
        fs::write(&path, "keep me\n").unwrap();
        assert!(start(&path).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "keep me\n");
    }

    #[test]
    fn concurrent_recorders_are_accounted_before_finish() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.jsonl");
        let guard = start(&path).unwrap();
        let sink = guard.sink.clone();
        let workers = 4;
        let per_worker = 256;
        let mut joins = Vec::new();
        for worker in 0..workers {
            let sink = sink.clone();
            joins.push(std::thread::spawn(move || {
                for sequence in 0..per_worker {
                    sink.record(
                        Uuid::from_u128((worker + 1) as u128),
                        sequence,
                        1,
                        Duration::from_micros(1),
                    );
                }
            }));
        }
        for join in joins {
            join.join().unwrap();
        }
        let sink_after_finish = sink.clone();
        guard.finish().unwrap();

        // Admission failures after close are deliberately ignored and cannot
        // alter the already-written footer.
        sink_after_finish.record(Uuid::from_u128(99), 0, 1, Duration::ZERO);
        let summary: Value = fs::read_to_string(path)
            .unwrap()
            .lines()
            .last()
            .map(|line| serde_json::from_str(line).unwrap())
            .unwrap();
        assert_eq!(
            summary["samples"].as_u64().unwrap(),
            (workers * per_worker) as u64
        );
        assert_eq!(summary["dropped"], 0);
    }

    #[test]
    fn disabled_environment_does_not_start_a_writer() {
        assert!(init_from_path(None).unwrap().is_none());
    }
    #[test]
    fn raw_limit_zero_keeps_histogram_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        let file = fs::File::create(&path).unwrap();
        let mut writer = TraceWriter::new(file, 0);

        let session = Uuid::new_v4();
        for us in [0u64, 100, 200_000].iter() {
            let ev = Event {
                session_id: session,
                sequence: 0,
                bytes: 0,
                elapsed_us: *us,
            };
            writer.observe(&ev).unwrap();
        }
        writer.finish(0).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 1, "expected exactly 1 line (footer)");

        let footer: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(footer["raw_events"], 0);
        assert_eq!(footer["raw_truncated"], true);
        assert_eq!(footer["samples"], 3);
        assert_eq!(footer["dropped"], 0);

        let global = &footer["global"];
        assert_eq!(global["count"], 3);
        let buckets = &global["buckets"];
        assert_eq!(buckets[0], 1);
        assert_eq!(buckets[1], 1);
        assert_eq!(buckets[1000], 1);

        let sessions = &footer["sessions"];
        let session_entry = &sessions[session.to_string()];
        assert_eq!(session_entry["count"], 3);
    }

    #[test]
    fn raw_limit_keeps_two_serialized_events_then_writes_summary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        let file = fs::File::create(&path).unwrap();
        let event = Event {
            session_id: Uuid::from_u128(0x1234),
            sequence: 42,
            bytes: 17,
            elapsed_us: 123,
        };
        let mut serialized = serde_json::to_vec(&event).unwrap();
        serialized.push(b'\n');
        let cap = serialized.len() * 2;
        let mut writer = TraceWriter::new(file, cap);
        for _ in 0..5 {
            writer.observe(&event).unwrap();
        }
        writer.finish(0).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "expected two raw events and one footer");
        let serialized_without_newline = &serialized[..serialized.len() - 1];
        assert_eq!(lines[0].as_bytes(), serialized_without_newline);
        assert_eq!(lines[1].as_bytes(), serialized_without_newline);

        let footer: Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(footer["raw_events"], 2);
        assert_eq!(footer["raw_truncated"], true);
        assert_eq!(footer["samples"], 5);
        assert_eq!(footer["global"]["count"], 5);
        assert_eq!(footer["sessions"][event.session_id.to_string()]["count"], 5);
        assert_eq!(bucket_sum(&footer["global"]), 5);
        assert_eq!(footer["raw_bytes"], cap as u64);
        assert!(fs::metadata(&path).unwrap().len() <= cap as u64 + 1024 * 1024);
    }

    #[test]
    fn raw_limit_caps_size_preserves_histogram_and_bounds_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        let file = fs::File::create(&path).unwrap();
        let mut writer = TraceWriter::new(file, 1);

        for index in 0u128..65 {
            let session = Uuid::from_u128(index + 1);
            let ev = Event {
                session_id: session,
                sequence: 0,
                bytes: 0,
                elapsed_us: 7,
            };
            writer.observe(&ev).unwrap();
        }
        writer.finish(0).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        assert!(
            metadata.len() <= 1 + 1024 * 1024,
            "file size {} exceeds cap",
            metadata.len()
        );

        let contents = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 1, "expected exactly 1 line (footer)");

        let footer: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(footer["raw_events"], 0);

        assert_eq!(footer["global"]["count"], 65);

        let sessions = &footer["sessions"];
        assert_eq!(sessions.as_object().unwrap().len(), 64);

        assert_eq!(footer["other"]["count"], 1);
        assert_eq!(footer["global"]["count"], 65);
        assert_eq!(
            sessions
                .as_object()
                .unwrap()
                .values()
                .map(|session| session["count"].as_u64().unwrap())
                .sum::<u64>(),
            64
        );
        assert_eq!(
            sessions
                .as_object()
                .unwrap()
                .values()
                .map(bucket_sum)
                .sum::<u64>(),
            64
        );
        assert_eq!(bucket_sum(&footer["global"]), 65);
        assert_eq!(bucket_sum(&footer["other"]), 1);
        assert_eq!(
            footer["global"]["count"].as_u64().unwrap(),
            footer["other"]["count"].as_u64().unwrap()
                + sessions
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|session| session["count"].as_u64().unwrap())
                    .sum::<u64>()
        );
        assert!(!sessions
            .as_object()
            .unwrap()
            .contains_key(&Uuid::from_u128(65).to_string()));
    }
}
