use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

pub(crate) fn frame_interval(fps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / fps.max(1) as u64)
}

pub(crate) struct SourcePump {
    appsrc: gst_app::AppSrc,
    latest: Arc<Mutex<Option<CapturedFrame>>>,
    capture_enabled: Arc<std::sync::atomic::AtomicBool>,
    next_pts_ns: AtomicU64,
}

#[derive(Clone)]
struct CapturedFrame {
    sample: gst::Sample,
    serial: u64,
    captured_at_ms: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct PushedFrame {
    pub(crate) pts_ns: u64,
    pub(crate) serial: u64,
    pub(crate) captured_at_ms: u64,
}

impl SourcePump {
    pub(crate) fn has_latest(&self) -> Result<bool> {
        self.latest
            .lock()
            .map(|latest| latest.is_some())
            .map_err(|_| anyhow!("Source frame lock poisoned"))
    }

    pub(crate) fn pause_capture(&self) {
        self.capture_enabled.store(false, Ordering::Release);
    }

    pub(crate) fn push_latest(&self, fps: u32) -> Result<Option<u64>> {
        Ok(self
            .push_latest_with(fps, |_| {})?
            .map(|frame| frame.serial))
    }

    pub(crate) fn push_latest_with(
        &self,
        fps: u32,
        before_push: impl FnOnce(PushedFrame),
    ) -> Result<Option<PushedFrame>> {
        let captured = {
            let guard = self
                .latest
                .lock()
                .map_err(|_| anyhow!("Source frame lock poisoned"))?;
            let Some(captured) = guard.clone() else {
                return Ok(None);
            };
            captured
        };
        let sample = captured.sample;
        let serial = captured.serial;
        let pts_ns = self.next_pts_ns.fetch_add(
            frame_interval(fps).as_nanos().min(u64::MAX as u128) as u64,
            Ordering::AcqRel,
        );
        let pushed = PushedFrame {
            pts_ns,
            serial,
            captured_at_ms: captured.captured_at_ms,
        };

        if let Some(caps) = sample.caps() {
            let caps = caps.to_owned();
            self.appsrc.set_caps(Some(&caps));
        }

        let mut buffer = sample
            .buffer()
            .ok_or_else(|| anyhow!("Sample has no buffer"))?
            .copy();
        let duration = gst::ClockTime::from_nseconds(frame_interval(fps).as_nanos() as u64);
        let buffer_ref = buffer
            .get_mut()
            .ok_or_else(|| anyhow!("Buffer not mutable"))?;
        buffer_ref.set_pts(Some(gst::ClockTime::from_nseconds(pts_ns)));
        buffer_ref.set_dts(None);
        buffer_ref.set_duration(Some(duration));

        // The callback runs immediately before push_buffer so the caller can
        // correlate this source serial with the downstream encoded buffer.
        before_push(pushed);
        self.appsrc
            .push_buffer(buffer)
            .map_err(|e| anyhow!("appsrc push failed: {e}"))?;
        Ok(Some(pushed))
    }
}

pub(crate) fn attach(pipeline: &gst::Pipeline) -> Result<SourcePump> {
    let appsink = pipeline
        .by_name("source_sink")
        .context("source_sink element not found")?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow!("source_sink is not an AppSink"))?;
    let appsrc = pipeline
        .by_name("source_pump")
        .context("source_pump element not found")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| anyhow!("source_pump is not an AppSrc"))?;

    let latest: Arc<Mutex<Option<CapturedFrame>>> = Arc::new(Mutex::new(None));
    let capture_enabled = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let source_serial = Arc::new(AtomicU64::new(0));
    let latest_cb = latest.clone();
    let capture_enabled_cb = capture_enabled.clone();
    let serial_cb = source_serial.clone();
    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                if sample.buffer().is_none() {
                    return Err(gst::FlowError::Error);
                }
                if !capture_enabled_cb.load(Ordering::Acquire) {
                    return Ok(gst::FlowSuccess::Ok);
                }
                let mut guard = latest_cb.lock().map_err(|_| gst::FlowError::Error)?;
                if !capture_enabled_cb.load(Ordering::Acquire) {
                    return Ok(gst::FlowSuccess::Ok);
                }
                let serial = serial_cb.fetch_add(1, Ordering::AcqRel) + 1;
                *guard = Some(CapturedFrame {
                    sample,
                    serial,
                    captured_at_ms: rc_platform_windows::monotonic_millis(),
                });
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    Ok(SourcePump {
        appsrc,
        latest,
        capture_enabled,
        next_pts_ns: AtomicU64::new(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_interval_matches_effective_fps() {
        assert_eq!(
            frame_interval(15),
            std::time::Duration::from_nanos(66_666_666)
        );
        assert_eq!(
            frame_interval(3),
            std::time::Duration::from_nanos(333_333_333)
        );
    }
}
