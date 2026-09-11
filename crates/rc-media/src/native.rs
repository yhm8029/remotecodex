//! Real WGC -> D3D11 -> H.264 -> DTLS-SRTP WebRTC pipeline.
//! The source and encoder are bridged through an appsink/appsrc pump so a
//! cached frame can be displayed without being reported as a fresh capture.
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Sender};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_sdp as sdp;
use gstreamer_webrtc as webrtc;
use rc_core::media_wire::*;
use std::{
    collections::BTreeMap,
    io::{self, BufReader, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use super::{adaptation::AdaptivePolicy, encoder::EncoderChoice, pump::frame_interval};

const REQUIRED_FACTORIES: &[&str] = &[
    "d3d11screencapturesrc",
    "d3d11convert",
    "h264parse",
    "rtph264pay",
    "webrtcbin",
    "nicesrc",
    "nicesink",
];
const SOFTWARE_FACTORIES: &[&str] = &["d3d11download", "openh264enc"];

pub fn probe() -> super::Capabilities {
    let init = gst::init();
    let missing = if let Err(e) = init {
        vec![e.to_string()]
    } else {
        let mut missing: Vec<String> = REQUIRED_FACTORIES
            .iter()
            .filter(|n| gst::ElementFactory::find(n).is_none())
            .map(|n| n.to_string())
            .collect();
        let hardware = gst::ElementFactory::find("mfh264enc").is_some();
        let software = SOFTWARE_FACTORIES
            .iter()
            .all(|n| gst::ElementFactory::find(n).is_some());
        if !hardware && !software {
            missing.push("mfh264enc or d3d11download+openh264enc".into());
        }
        missing
    };
    super::Capabilities {
        protocol: 1,
        native_backend: true,
        available: missing.is_empty(),
        sdk_version: Some(gst::version_string().to_string()),
        missing,
        windows_runtime_verified: false,
    }
}

fn source_arg(source: &NativeSource, handle: u64) -> String {
    match source {
        NativeSource::Window { .. } => {
            format!("window-handle={handle} window-capture-mode=client")
        }
        NativeSource::Monitor { .. } => format!("monitor-handle={handle}"),
    }
}

fn encoder_description(p: VideoProfile, choice: EncoderChoice) -> String {
    let encoder = match choice {
        EncoderChoice::Hardware => format!(
            "d3d11convert ! video/x-raw(memory:D3D11Memory),format=NV12,width={},height={} \
             ! mfh264enc name=encoder low-latency=true cabac=false bitrate={} gop-size={}",
            p.width,
            p.height,
            p.bitrate_kbps,
            p.fps * 2
        ),
        EncoderChoice::SoftwareViewOnlySlow => format!(
            "d3d11download ! video/x-raw,format=NV12,width={},height={} \
             ! videoconvert ! openh264enc name=encoder bitrate={}",
            p.width,
            p.height,
            p.bitrate_kbps.saturating_mul(1000)
        ),
    };
    encoder
}

fn parse_pipeline(
    source_arg: &str,
    p: VideoProfile,
    choice: EncoderChoice,
) -> Result<gst::Pipeline> {
    parse_pipeline_with_output(source_arg, p, choice, OutputMode::WebRtc)
}

#[derive(Clone, Copy)]
enum OutputMode {
    WebRtc,
    Fakesink,
}

fn parse_pipeline_with_output(
    source_arg: &str,
    p: VideoProfile,
    choice: EncoderChoice,
    output_mode: OutputMode,
) -> Result<gst::Pipeline> {
    // gst_parse_launch does not accept independent chains separated by ';'.
    // Build one Pipeline and attach two parsed bins, then request the
    // webrtcbin sink pad explicitly.
    let pipeline = gst::Pipeline::new();
    let rtc = match output_mode {
        OutputMode::WebRtc => Some(gst::ElementFactory::make("webrtcbin").name("rtc").build()?),
        OutputMode::Fakesink => None,
    };
    let source = gst::parse::bin_from_description(
        &format!(
            "d3d11screencapturesrc name=capture capture-api=wgc {source_arg} \
             show-border=true show-cursor=true \
             ! appsink name=source_sink emit-signals=true max-buffers=1 drop=true sync=false async=false",
        ),
        false,
    )?;
    let output_tail = match output_mode {
        OutputMode::WebRtc => {
            "! rtph264pay name=pay pt=96 config-interval=-1 aggregate-mode=zero-latency"
        }
        OutputMode::Fakesink => "! fakesink name=probe_sink sync=false",
    };
    let output = gst::parse::bin_from_description(
        &format!(
            "appsrc name=source_pump is-live=false do-timestamp=false format=time block=false \
             ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream \
             ! {} \
             ! video/x-h264,profile=constrained-baseline \
             ! identity name=timestamp_bridge single-segment=true \
             ! h264parse name=encoded config-interval=-1 {output_tail}",
            encoder_description(p, choice),
        ),
        true,
    )?;
    if let Some(rtc) = &rtc {
        pipeline.add(rtc)?;
    }
    pipeline.add(&source)?;
    pipeline.add(&output)?;
    if let Some(rtc) = rtc {
        let sink = rtc
            .request_pad_simple("sink_%u")
            .context("WebRTC video sink pad missing")?;
        output
            .static_pad("src")
            .context("RTP payloader source pad missing")?
            .link(&sink)
            .map_err(|e| anyhow::anyhow!("WebRTC pad link failed: {e:?}"))?;
    }
    Ok(pipeline)
}

fn build_capture_encode_pipeline(
    init: &MediaInit,
    handle: u64,
) -> Result<(gst::Pipeline, EncoderChoice, VideoProfile)> {
    let source = source_arg(&init.source.native, handle);
    let hardware_profile = EncoderChoice::Hardware
        .profile(init.profile, init.source.rect)
        .map_err(anyhow::Error::msg)?;
    if gst::ElementFactory::find("mfh264enc").is_some() {
        if let Ok(pipeline) = parse_pipeline_with_output(
            &source,
            hardware_profile,
            EncoderChoice::Hardware,
            OutputMode::Fakesink,
        ) {
            let encoder = pipeline
                .by_name("encoder")
                .context("Hardware encoder missing")?;
            let accepts_d3d11 = encoder
                .find_property("d3d11-aware")
                .is_some_and(|_| encoder.property::<bool>("d3d11-aware"));
            if accepts_d3d11 {
                return Ok((pipeline, EncoderChoice::Hardware, hardware_profile));
            }
        }
    }
    if SOFTWARE_FACTORIES
        .iter()
        .all(|n| gst::ElementFactory::find(n).is_some())
    {
        let profile = EncoderChoice::SoftwareViewOnlySlow
            .profile(init.profile, init.source.rect)
            .map_err(anyhow::Error::msg)?;
        let pipeline = parse_pipeline_with_output(
            &source,
            profile,
            EncoderChoice::SoftwareViewOnlySlow,
            OutputMode::Fakesink,
        )?;
        return Ok((pipeline, EncoderChoice::SoftwareViewOnlySlow, profile));
    }
    bail!("No compatible D3D11 hardware or software view-only encoder")
}

fn build_pipeline(
    init: &MediaInit,
    handle: u64,
) -> Result<(gst::Pipeline, EncoderChoice, VideoProfile)> {
    let source = source_arg(&init.source.native, handle);
    let hardware_profile = EncoderChoice::Hardware
        .profile(init.profile, init.source.rect)
        .map_err(anyhow::Error::msg)?;
    if gst::ElementFactory::find("mfh264enc").is_some() {
        if let Ok(pipeline) = parse_pipeline(&source, hardware_profile, EncoderChoice::Hardware) {
            let encoder = pipeline
                .by_name("encoder")
                .context("Hardware encoder missing")?;
            let accepts_d3d11 = encoder
                .find_property("d3d11-aware")
                .is_some_and(|_| encoder.property::<bool>("d3d11-aware"));
            if accepts_d3d11 {
                return Ok((pipeline, EncoderChoice::Hardware, hardware_profile));
            }
        }
    }
    if SOFTWARE_FACTORIES
        .iter()
        .all(|n| gst::ElementFactory::find(n).is_some())
    {
        let profile = EncoderChoice::SoftwareViewOnlySlow
            .profile(init.profile, init.source.rect)
            .map_err(anyhow::Error::msg)?;
        let pipeline = parse_pipeline(&source, profile, EncoderChoice::SoftwareViewOnlySlow)?;
        return Ok((pipeline, EncoderChoice::SoftwareViewOnlySlow, profile));
    }
    bail!("No compatible D3D11 hardware or software view-only encoder")
}

fn event(tx: &Sender<HelperOut>, stop: &AtomicBool, e: HelperOut) {
    if tx.try_send(e).is_err() {
        stop.store(true, Ordering::Release);
    }
}

fn write_event(w: &mut impl Write, e: &HelperOut) -> Result<()> {
    let bytes = serde_json::to_vec(e)?;
    if bytes.len() > MAX_SIGNAL - 1 {
        bail!("Oversized signaling output");
    }
    w.write_all(&bytes)?;
    w.write_all(b"\n")?;
    w.flush()?;
    Ok(())
}

struct Playing(gst::Pipeline);
impl Drop for Playing {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

const MAX_PENDING_FRESH_PROOFS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FreshProof {
    serial: u64,
    captured_at_ms: u64,
}

#[derive(Debug, Default)]
struct FreshProofMap {
    entries: BTreeMap<u64, FreshProof>,
}

impl FreshProofMap {
    fn insert(&mut self, pts_ns: u64, proof: FreshProof) {
        while self.entries.len() >= MAX_PENDING_FRESH_PROOFS {
            let _ = self.entries.pop_first();
        }
        self.entries.insert(pts_ns, proof);
    }

    fn take(&mut self, pts_ns: u64) -> Option<FreshProof> {
        self.entries.remove(&pts_ns)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

pub fn run() -> Result<()> {
    gst::init()?;
    let mut input = BufReader::new(io::stdin());
    let init = match read_json_line::<_, HelperIn>(&mut input)? {
        Some(HelperIn::Init { config }) => config,
        _ => bail!("First message must be Init"),
    };
    init.validate().map_err(anyhow::Error::msg)?;
    if rc_platform_windows::validate_source(&init.source)? != init.source.rect {
        bail!("Approved geometry changed before capture");
    }
    let caps = probe();
    if !caps.available {
        bail!("Missing native factories: {:?}", caps.missing);
    }
    let handle = init.source.native.handle().map_err(anyhow::Error::msg)?;
    let (pipeline, encoder_choice, profile) = build_pipeline(&init, handle)?;
    let _guard = Playing(pipeline.clone());
    let rtc = pipeline.by_name("rtc").context("WebRTC element missing")?;
    let encoder = pipeline.by_name("encoder").context("Encoder missing")?;
    let ice = rtc.property::<webrtc::WebRTCICE>("ice-agent");
    if ice.find_property("min-rtp-port").is_none() || ice.find_property("max-rtp-port").is_none() {
        bail!("ICE port bounds unavailable");
    }
    ice.set_property("min-rtp-port", init.min_port as u32);
    ice.set_property("max-rtp-port", init.max_port as u32);
    // This official ICE action disables automatic local-interface discovery.
    if !ice.emit_by_name::<bool>("add-local-ip-address", &[&init.tailnet_ip]) {
        bail!("Could not bind WebRTC to the approved Tailscale IP");
    }

    let source_pump = super::pump::attach(&pipeline)?;
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, events) = bounded::<HelperOut>(64);
    let pending_source = Arc::new(Mutex::new(FreshProofMap::default()));
    let (commands, input_rx) = bounded::<HelperIn>(32);
    let s = stop.clone();
    std::thread::Builder::new()
        .name("rc-media-input".into())
        .spawn(move || {
            loop {
                match read_json_line::<_, HelperIn>(&mut input) {
                    Ok(Some(m)) => {
                        let end = matches!(m, HelperIn::Stop);
                        if commands.send(m).is_err() || end {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            s.store(true, Ordering::Release);
        })?;

    let negotiating = Arc::new(AtomicBool::new(false));
    let first = negotiating.clone();
    let t = tx.clone();
    let s = stop.clone();
    rtc.connect("on-negotiation-needed", false, move |values| {
        if first.swap(true, Ordering::AcqRel) {
            return None;
        }
        let Ok(rtc) = values[0].get::<gst::Element>() else {
            s.store(true, Ordering::Release);
            return None;
        };
        let w = rtc.downgrade();
        let t = t.clone();
        let s = s.clone();
        let promise = gst::Promise::with_change_func(move |reply| {
            let result = (|| -> Result<()> {
                let r = reply
                    .map_err(|e| anyhow::anyhow!("Offer promise: {e:?}"))?
                    .context("No SDP offer")?;
                let offer = r.get::<webrtc::WebRTCSessionDescription>("offer")?;
                let rtc = w.upgrade().context("WebRTC element gone")?;
                rtc.emit_by_name::<()>("set-local-description", &[&offer, &None::<gst::Promise>]);
                let text = sanitize_sdp(&offer.sdp().as_text()?).map_err(anyhow::Error::msg)?;
                event(&t, &s, HelperOut::Offer { sdp: text });
                Ok(())
            })();
            if result.is_err() {
                event(
                    &t,
                    &s,
                    HelperOut::Error {
                        code: "OFFER_FAILED".into(),
                    },
                );
                s.store(true, Ordering::Release);
            }
        });
        rtc.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &promise]);
        None
    });
    let ip = init.tailnet_ip.clone();
    let t = tx.clone();
    let s = stop.clone();
    rtc.connect("on-ice-candidate", false, move |values| {
        if let (Ok(mline), Ok(candidate)) = (values[1].get::<u32>(), values[2].get::<String>()) {
            if mline == 0 && candidate_allowed(&candidate, Some(&ip)) {
                event(&t, &s, HelperOut::Ice { candidate, mline });
            }
        }
        None
    });

    // Correlate exact buffer PTS values. FIFO order is unsafe because encoder
    // queues can drop or reorder cached repeats.
    let pending_for_probe = pending_source.clone();
    let t = tx.clone();
    let s = stop.clone();
    pipeline
        .by_name("encoded")
        .context("Parser missing")?
        .static_pad("src")
        .context("Encoder pad missing")?
        .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            let proof = info
                .buffer()
                .and_then(|buffer| buffer.pts())
                .and_then(|pts| {
                    pending_for_probe
                        .lock()
                        .ok()
                        .and_then(|mut pending| pending.take(pts.nseconds()))
                });
            if let Some(proof) = proof {
                event(
                    &t,
                    &s,
                    HelperOut::Frame {
                        sequence: proof.serial.to_string(),
                        captured_at_ms: proof.captured_at_ms,
                    },
                );
            }
            gst::PadProbeReturn::Ok
        });
    let bus = pipeline.bus().context("Pipeline has no bus")?;
    let t = tx.clone();
    let s = stop.clone();
    bus.set_sync_handler(move |_, m| {
        match m.view() {
            gst::MessageView::Error(_) => {
                event(
                    &t,
                    &s,
                    HelperOut::Error {
                        code: "NATIVE_PIPELINE_FAILED".into(),
                    },
                );
                s.store(true, Ordering::Release);
            }
            gst::MessageView::Eos(_) => {
                s.store(true, Ordering::Release);
            }
            _ => {}
        }
        gst::BusSyncReply::Drop
    });

    let mode = encoder_choice.mode();
    let encoder_control_allowed = encoder_choice.control_allowed();
    let started = Instant::now();
    let started_tick = rc_platform_windows::monotonic_millis();
    let mut policy = AdaptivePolicy::new(profile.fps, profile.bitrate_kbps, 0);
    let mut target = policy.target(0);
    let mut last_source_serial = 0;
    let mut last_enqueued_serial = 0;
    let mut last_source_tick: Option<u64> = None;
    let mut next_push = Instant::now();
    let mut last_status = Instant::now() - Duration::from_secs(1);
    let mut answer_seen = false;
    let mut output = io::stdout().lock();
    write_event(&mut output, &HelperOut::Ready { protocol: 1 })?;
    write_event(
        &mut output,
        &HelperOut::Status {
            mode,
            capture_state: CaptureState::Starting,
            fps: target.fps,
            bitrate_kbps: target.bitrate_kbps,
            adaptive_idle: target.adaptive_idle,
            control_allowed: false,
            awaiting_fresh_frame: true,
        },
    )?;
    pipeline.set_state(gst::State::Playing)?;

    while !stop.load(Ordering::Acquire) {
        crossbeam_channel::select! {
            recv(events) -> m => {
                if let Ok(m) = m { write_event(&mut output, &m)?; } else { break; }
            },
            recv(input_rx) -> m => {
                match m {
                    Ok(HelperIn::Answer { sdp: answer }) => {
                        if answer_seen { bail!("Renegotiation is not supported in this stream"); }
                        let clean = sanitize_sdp(&answer).map_err(anyhow::Error::msg)?;
                        let parsed = sdp::SDPMessage::parse_buffer(clean.as_bytes())?;
                        let desc = webrtc::WebRTCSessionDescription::new(
                            webrtc::WebRTCSDPType::Answer,
                            parsed,
                        );
                        rtc.emit_by_name::<()>("set-remote-description", &[&desc, &None::<gst::Promise>]);
                        answer_seen = true;
                    }
                    Ok(HelperIn::Ice { candidate, mline }) => {
                        if mline != 0 || !candidate_allowed(&candidate, None) {
                            bail!("ICE candidate rejected");
                        }
                        rtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
                    }
                    Ok(HelperIn::Stop) | Err(_) => break,
                    Ok(HelperIn::Init { .. }) => bail!("Duplicate init"),
                }
            },
            default(Duration::from_millis(10)) => {}
        }

        let now = Instant::now();
        let now_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let next_target = policy.target(now_ms);
        if next_target != target {
            target = next_target;
            if encoder.find_property("bitrate").is_some() {
                let value = match encoder_choice {
                    EncoderChoice::Hardware => target.bitrate_kbps,
                    EncoderChoice::SoftwareViewOnlySlow => target.bitrate_kbps.saturating_mul(1000),
                };
                encoder.set_property("bitrate", value);
            }
        }
        if now >= next_push {
            let pending = pending_source.clone();
            let stop_for_push = stop.clone();
            let last_enqueued = last_enqueued_serial;
            let pushed = source_pump.push_latest_with(target.fps, move |frame| {
                if let Ok(mut queue) = pending.lock() {
                    if frame.serial > last_enqueued {
                        queue.insert(
                            frame.pts_ns,
                            FreshProof {
                                serial: frame.serial,
                                captured_at_ms: frame.captured_at_ms,
                            },
                        );
                    }
                } else {
                    stop_for_push.store(true, Ordering::Release);
                }
            })?;
            if let Some(frame) = pushed {
                if frame.serial > last_source_serial {
                    last_source_serial = frame.serial;
                    last_source_tick = Some(frame.captured_at_ms);
                    let captured_ms = frame.captured_at_ms.saturating_sub(started_tick);
                    policy.observe_source(captured_ms);
                    last_enqueued_serial = frame.serial;
                }
            }
            next_push = now + frame_interval(target.fps);
        }
        if last_status.elapsed() >= Duration::from_millis(100) {
            let now_tick = rc_platform_windows::monotonic_millis();
            let capture_state = match last_source_tick {
                None => CaptureState::Starting,
                Some(at) if at <= now_tick && now_tick - at <= 500 => CaptureState::Live,
                Some(_) => CaptureState::MinimizedOrStalled,
            };
            let awaiting_fresh_frame = capture_state != CaptureState::Live;
            write_event(
                &mut output,
                &HelperOut::Status {
                    mode,
                    capture_state,
                    fps: target.fps,
                    bitrate_kbps: target.bitrate_kbps,
                    adaptive_idle: target.adaptive_idle,
                    control_allowed: encoder_control_allowed && !awaiting_fresh_frame,
                    awaiting_fresh_frame,
                },
            )?;
            last_status = now;
        }
    }
    pipeline.set_state(gst::State::Null)?;
    write_event(&mut output, &HelperOut::Stopped)?;
    Ok(())
}

/// Run the bounded capture/encode proof against an explicitly owned fixture
/// window. The harness deliberately terminates at `fakesink`; it proves the
/// WGC appsink -> appsrc -> encoder -> h264parse bridge and timestamp proof,
/// without claiming a WebRTC or two-machine result.
#[derive(Debug, serde::Serialize)]
pub struct CaptureEncodeFixtureReport {
    pub encoder: EncoderChoiceName,
    pub source_serial: u64,
    pub fresh_proofs: u32,
    pub encoded_buffers: u32,
    pub cached_repeat_proofs: u32,
    pub stale_capture_age_ms: u64,
    pub stopped_cleanly: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncoderChoiceName {
    Hardware,
    SoftwareViewOnlySlow,
}

#[derive(Clone, Copy)]
struct EncodedObservation {
    proof: Option<FreshProof>,
}

pub fn run_owned_capture_encode_fixture(handle: u64) -> Result<CaptureEncodeFixtureReport> {
    gst::init()?;
    if handle == 0 {
        bail!("Fixture window handle is required");
    }
    let init = MediaInit {
        protocol: 1,
        source: LocalSource {
            native: NativeSource::Window {
                handle: handle.to_string(),
                pid: 0,
                created: "owned-fixture".into(),
            },
            label: "RemoteCodex owned capture fixture".into(),
            rect: rc_core::geometry::Rect {
                left: 0,
                top: 0,
                width: 760,
                height: 520,
            },
        },
        profile: VideoProfile {
            width: 640,
            height: 360,
            fps: 5,
            bitrate_kbps: 512,
        },
        // The capture/encode fixture never constructs WebRTC or uses ICE.
        tailnet_ip: "100.64.0.1".into(),
        min_port: 40_000,
        max_port: 40_031,
    };
    let (pipeline, choice, profile) = build_capture_encode_pipeline(&init, handle)?;
    let pump = super::pump::attach(&pipeline)?;
    let pending = Arc::new(Mutex::new(FreshProofMap::default()));
    let observations = Arc::new(Mutex::new(Vec::<EncodedObservation>::new()));
    let pending_probe = pending.clone();
    let observations_probe = observations.clone();
    pipeline
        .by_name("encoded")
        .context("Fixture parser missing")?
        .static_pad("src")
        .context("Fixture encoded pad missing")?
        .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            let proof = info
                .buffer()
                .and_then(|buffer| buffer.pts())
                .and_then(|pts| {
                    pending_probe
                        .lock()
                        .ok()
                        .and_then(|mut pending| pending.take(pts.nseconds()))
                });
            if let Ok(mut observations) = observations_probe.lock() {
                observations.push(EncodedObservation { proof });
            }
            gst::PadProbeReturn::Ok
        });
    let bus = pipeline.bus().context("Fixture pipeline has no bus")?;
    pipeline.set_state(gst::State::Playing)?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while !pump.has_latest()? {
        drain_fixture_bus(&bus)?;
        if Instant::now() >= deadline {
            let _ = pipeline.set_state(gst::State::Null);
            bail!("Owned fixture produced no WGC sample before timeout");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // Freeze immediately after the first source sample so a later callback
    // cannot turn the intended cache repeat into a second capture.
    pump.pause_capture();
    std::thread::sleep(Duration::from_millis(100));
    let first = pump
        .push_latest_with(profile.fps, |frame| {
            if let Ok(mut pending) = pending.lock() {
                pending.insert(
                    frame.pts_ns,
                    FreshProof {
                        serial: frame.serial,
                        captured_at_ms: frame.captured_at_ms,
                    },
                );
            }
        })?
        .context("First cached fixture frame was unavailable")?;
    wait_for_observations(&bus, &observations, 1, deadline)?;

    // Freeze the appsink callback after the first real WGC sample. Subsequent
    // pushes therefore exercise the cached-repeat path with a new PTS but no
    // new source serial/proof entry.
    let cached = pump
        .push_latest_with(profile.fps, |_| {})?
        .context("Cached fixture repeat was unavailable")?;
    if cached.serial != first.serial {
        let _ = pipeline.set_state(gst::State::Null);
        bail!("Fixture repeat did not use the cached source serial");
    }
    wait_for_observations(&bus, &observations, 2, deadline)?;
    std::thread::sleep(Duration::from_millis(600));
    let _ = pump.push_latest_with(profile.fps, |_| {})?;
    wait_for_observations(&bus, &observations, 3, deadline)?;

    let observations = observations
        .lock()
        .map_err(|_| anyhow::anyhow!("Fixture observation lock poisoned"))?;
    let fresh_proofs = observations
        .iter()
        .filter(|item| item.proof.is_some())
        .count() as u32;
    let cached_repeat_proofs = observations
        .iter()
        .skip(1)
        .filter(|item| item.proof.is_some())
        .count() as u32;
    let stale_capture_age_ms =
        rc_platform_windows::monotonic_millis().saturating_sub(first.captured_at_ms);
    let stopped_cleanly = pipeline.set_state(gst::State::Null).is_ok();
    if fresh_proofs != 1 || cached_repeat_proofs != 0 || stale_capture_age_ms < 500 {
        bail!(
            "Fixture freshness proof failed: fresh={fresh_proofs}, repeat={cached_repeat_proofs}, age={stale_capture_age_ms}ms"
        );
    }
    Ok(CaptureEncodeFixtureReport {
        encoder: match choice {
            EncoderChoice::Hardware => EncoderChoiceName::Hardware,
            EncoderChoice::SoftwareViewOnlySlow => EncoderChoiceName::SoftwareViewOnlySlow,
        },
        source_serial: first.serial,
        fresh_proofs,
        encoded_buffers: observations.len() as u32,
        cached_repeat_proofs,
        stale_capture_age_ms,
        stopped_cleanly,
    })
}

fn drain_fixture_bus(bus: &gst::Bus) -> Result<()> {
    while let Some(message) = bus.timed_pop(gst::ClockTime::from_nseconds(0)) {
        if let gst::MessageView::Error(error) = message.view() {
            bail!("Owned fixture pipeline error: {}", error.error());
        }
    }
    Ok(())
}

fn wait_for_observations(
    bus: &gst::Bus,
    observations: &Arc<Mutex<Vec<EncodedObservation>>>,
    count: usize,
    deadline: Instant,
) -> Result<()> {
    loop {
        drain_fixture_bus(bus)?;
        if observations
            .lock()
            .map_err(|_| anyhow::anyhow!("Fixture observation lock poisoned"))?
            .len()
            >= count
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("Owned fixture encoder produced fewer than {count} buffers");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_proof_map_requires_exact_pts_and_preserves_serial() {
        let captured_at_ms = 1000;
        let mut pending = FreshProofMap::default();

        // The encoder may repeat an old cached buffer without a corresponding
        // source push. That PTS is intentionally absent from the map.
        pending.insert(
            100,
            FreshProof {
                serial: 7,
                captured_at_ms,
            },
        );
        assert!(pending.take(90).is_none());
        assert!(pending.take(110).is_none());
        assert_eq!(pending.take(100).unwrap().serial, 7);
        assert!(pending.take(100).is_none());

        // A later capture gets a distinct PTS and cannot be confused with the
        // old repeat, even if the encoder delivers buffers out of order.
        pending.insert(
            200,
            FreshProof {
                serial: 8,
                captured_at_ms,
            },
        );
        pending.insert(
            300,
            FreshProof {
                serial: 9,
                captured_at_ms,
            },
        );
        assert_eq!(pending.take(300).unwrap().serial, 9);
        assert_eq!(pending.take(200).unwrap().serial, 8);
    }

    #[test]
    fn fresh_proof_map_is_bounded_and_prunes_oldest_pts() {
        let captured_at_ms = 1000;
        let mut pending = FreshProofMap::default();
        for serial in 0..=(MAX_PENDING_FRESH_PROOFS as u64) {
            pending.insert(
                serial,
                FreshProof {
                    serial,
                    captured_at_ms,
                },
            );
        }

        assert_eq!(pending.len(), MAX_PENDING_FRESH_PROOFS);
        assert!(pending.take(0).is_none());
        assert_eq!(
            pending
                .take(MAX_PENDING_FRESH_PROOFS as u64)
                .unwrap()
                .serial,
            MAX_PENDING_FRESH_PROOFS as u64
        );
    }

    #[test]
    fn appsink_appsrc_pipeline_contains_real_capture_bridge() {
        gst::init().unwrap();
        if gst::ElementFactory::find("mfh264enc").is_none() {
            return;
        }
        let profile = VideoProfile {
            width: 640,
            height: 360,
            fps: 5,
            bitrate_kbps: 512,
        };
        let pipeline = parse_pipeline(
            "window-handle=1 window-capture-mode=client",
            profile,
            EncoderChoice::Hardware,
        )
        .expect("native media pipeline should parse");
        assert!(pipeline.by_name("source_sink").is_some());
        assert!(pipeline.by_name("source_pump").is_some());
        assert!(pipeline.by_name("encoder").is_some());
        assert!(pipeline.by_name("rtc").is_some());
    }

    #[test]
    fn software_pipeline_is_view_only_and_does_not_gate_hardware_probe() {
        gst::init().unwrap();
        if !SOFTWARE_FACTORIES
            .iter()
            .all(|name| gst::ElementFactory::find(name).is_some())
        {
            return;
        }
        let profile = VideoProfile {
            width: 576,
            height: 360,
            fps: 5,
            bitrate_kbps: 512,
        };
        let pipeline = parse_pipeline(
            "window-handle=1 window-capture-mode=client",
            profile,
            EncoderChoice::SoftwareViewOnlySlow,
        )
        .expect("software view-only pipeline should parse");
        assert!(pipeline.by_name("source_pump").is_some());
        assert!(pipeline.by_name("encoder").is_some());
        assert!(!EncoderChoice::SoftwareViewOnlySlow.control_allowed());
    }
}

pub fn run_owned_capture_encode_benchmark(
    handle: u64,
    width: u32,
    height: u32,
    fps: u32,
    bitrate_kbps: u32,
    seconds: u64,
) -> Result<serde_json::Value> {
    gst::init()?;
    if handle == 0 {
        bail!("Benchmark window handle is required");
    }
    if !(2..=1920).contains(&width) || !(2..=1080).contains(&height) {
        bail!("Benchmark dimensions out of range");
    }
    if !(1..=30).contains(&fps) {
        bail!("Benchmark fps out of range");
    }
    if !(1..=20000).contains(&bitrate_kbps) {
        bail!("Benchmark bitrate out of range");
    }
    if !(1..=610).contains(&seconds) {
        bail!("Benchmark duration out of range");
    }
    let init = MediaInit {
        protocol: 1,
        source: LocalSource {
            native: NativeSource::Window {
                handle: handle.to_string(),
                pid: 0,
                created: "owned-benchmark".into(),
            },
            label: "RemoteCodex owned capture benchmark".into(),
            rect: rc_core::geometry::Rect {
                left: 0,
                top: 0,
                width,
                height,
            },
        },
        profile: VideoProfile {
            width,
            height,
            fps,
            bitrate_kbps,
        },
        tailnet_ip: "100.64.0.1".into(),
        min_port: 40_000,
        max_port: 40_031,
    };
    let (pipeline, choice, effective) = build_capture_encode_pipeline(&init, handle)?;
    let outcome: Result<serde_json::Value> = (|| -> Result<serde_json::Value> {
        let pump = super::pump::attach(&pipeline)?;
        let encoded_buffers = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let encoded_probe = encoded_buffers.clone();
        pipeline
            .by_name("encoded")
            .context("Benchmark parser missing")?
            .static_pad("src")
            .context("Benchmark encoded pad missing")?
            .add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                encoded_probe.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                gst::PadProbeReturn::Ok
            });
        let bus = pipeline.bus().context("Benchmark pipeline has no bus")?;
        pipeline.set_state(gst::State::Playing)?;
        let first_deadline = Instant::now() + Duration::from_secs(10);
        while !pump.has_latest()? {
            while let Some(message) = bus.timed_pop(gst::ClockTime::from_nseconds(0)) {
                match message.view() {
                    gst::MessageView::Error(err) => {
                        bail!("Benchmark pipeline error: {}", err.error());
                    }
                    gst::MessageView::Eos(_) => {
                        bail!("Benchmark pipeline reached EOS before first frame");
                    }
                    _ => {}
                }
            }
            if Instant::now() >= first_deadline {
                bail!("Benchmark produced no source sample before timeout");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let interval = frame_interval(effective.fps);
        let start = Instant::now();
        let deadline = start + Duration::from_secs(seconds);
        let mut next = Instant::now();
        let mut source_pushes: u64 = 0;
        let mut unique_pushed_source_frames: u64 = 0;
        let mut last_serial: Option<u64> = None;
        while Instant::now() < deadline {
            let now = Instant::now();
            if now < next {
                std::thread::sleep(next - now);
            }
            if Instant::now() >= deadline {
                break;
            }
            if let Some(pushed) = pump.push_latest_with(effective.fps, |_| {})? {
                source_pushes += 1;
                if last_serial.map(|s| s != pushed.serial).unwrap_or(true) {
                    unique_pushed_source_frames += 1;
                    last_serial = Some(pushed.serial);
                }
            }
            let mut bail_with = None;
            while let Some(message) = bus.timed_pop(gst::ClockTime::from_nseconds(0)) {
                match message.view() {
                    gst::MessageView::Error(err) => {
                        bail_with = Some(format!("Benchmark pipeline error: {}", err.error()));
                    }
                    gst::MessageView::Eos(_) => {
                        bail_with = Some("Benchmark pipeline reached EOS during run".to_string());
                    }
                    _ => {}
                }
            }
            if let Some(msg) = bail_with {
                bail!("{}", msg);
            }
            next = (next + interval).max(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(100));
        while let Some(message) = bus.timed_pop(gst::ClockTime::from_nseconds(0)) {
            match message.view() {
                gst::MessageView::Error(err) => {
                    bail!("Benchmark pipeline error: {}", err.error());
                }
                gst::MessageView::Eos(_) => bail!("Benchmark reached EOS during final drain"),
                _ => {}
            }
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let encoded_buffers_val = encoded_buffers.load(std::sync::atomic::Ordering::Relaxed);
        let actual_encoded_fps = if elapsed_ms > 0 {
            encoded_buffers_val as f64 / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };
        let unique_pushed_fps = if elapsed_ms > 0 {
            unique_pushed_source_frames as f64 / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };
        Ok(serde_json::json!({
            "scope": "owned_window_capture_encode_fakesink",
            "encoder": match choice {
                EncoderChoice::Hardware => EncoderChoiceName::Hardware,
                EncoderChoice::SoftwareViewOnlySlow => EncoderChoiceName::SoftwareViewOnlySlow,
            },
            "requested_profile": { "width": width, "height": height, "fps": fps, "bitrate_kbps": bitrate_kbps },
            "effective_profile": { "width": effective.width, "height": effective.height, "fps": effective.fps, "bitrate_kbps": effective.bitrate_kbps },
            "width": effective.width,
            "height": effective.height,
            "fps": effective.fps,
            "bitrate_kbps": effective.bitrate_kbps,
            "source_pushes": source_pushes,
            "unique_pushed_source_frames": unique_pushed_source_frames,
            "encoded_buffers": encoded_buffers_val,
            "actual_encoded_fps": actual_encoded_fps,
            "unique_pushed_fps": unique_pushed_fps,
            "elapsed_ms": elapsed_ms,
            "stopped_cleanly": false,
        }))
    })();
    let stopped = pipeline.set_state(gst::State::Null);
    let mut value = outcome?;
    stopped.context("Benchmark stop failed")?;
    value["stopped_cleanly"] = serde_json::json!(true);
    Ok(value)
}
