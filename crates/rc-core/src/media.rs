use crate::{error::ErrorCode, geometry::Rect};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use uuid::Uuid;
/// CPU-side ownership/state logic. Actual WGC/MF/WebRTC is isolated in rc-media.
#[derive(Debug, Default)]
pub struct MediaDemand {
    viewers: HashSet<Uuid>,
    idle_since: Option<Instant>,
}
impl MediaDemand {
    pub fn subscribe(&mut self, id: Uuid) {
        self.viewers.insert(id);
        self.idle_since = None;
    }
    pub fn unsubscribe(&mut self, id: Uuid, now: Instant) {
        self.viewers.remove(&id);
        if self.viewers.is_empty() {
            self.idle_since.get_or_insert(now);
        }
    }
    pub fn should_capture(&self) -> bool {
        !self.viewers.is_empty()
    }
    pub fn release_resources(&self, now: Instant) -> bool {
        self.viewers.is_empty()
            && self
                .idle_since
                .is_some_and(|n| now.duration_since(n) >= Duration::from_secs(2))
    }
    pub fn should_exit(&self, now: Instant) -> bool {
        self.viewers.is_empty()
            && self
                .idle_since
                .is_some_and(|n| now.duration_since(n) >= Duration::from_secs(10))
    }
}
#[derive(Debug)]
pub struct LatestFrame<T> {
    value: Option<T>,
    pub dropped: u64,
}
impl<T> Default for LatestFrame<T> {
    fn default() -> Self {
        Self {
            value: None,
            dropped: 0,
        }
    }
}
impl<T> LatestFrame<T> {
    pub fn replace(&mut self, x: T) {
        if self.value.replace(x).is_some() {
            self.dropped += 1;
        }
    }
    pub fn take(&mut self) -> Option<T> {
        self.value.take()
    }
}
#[derive(Debug, Clone)]
pub struct FrameProof {
    pub source: Uuid,
    pub generation: u32,
    pub geometry_version: u64,
    pub rect: Rect,
    pub received: Instant,
}
impl FrameProof {
    pub fn validate(
        &self,
        source: Uuid,
        generation: u32,
        geometry: u64,
        now: Instant,
    ) -> Result<(), ErrorCode> {
        if self.source != source
            || self.generation != generation
            || self.geometry_version != geometry
            || now.duration_since(self.received) > Duration::from_millis(500)
        {
            Err(ErrorCode::SourceStale)
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_viewer_has_no_capture() {
        assert!(!MediaDemand::default().should_capture());
    }
    #[test]
    fn lifecycle() {
        let mut d = MediaDemand::default();
        let id = Uuid::new_v4();
        let n = Instant::now();
        d.subscribe(id);
        assert!(d.should_capture());
        d.unsubscribe(id, n);
        assert!(d.release_resources(n + Duration::from_secs(2)));
        assert!(d.should_exit(n + Duration::from_secs(10)));
    }
    #[test]
    fn drop_old_frames_not_delayed_video() {
        let mut q = LatestFrame::default();
        q.replace(1);
        q.replace(2);
        q.replace(3);
        assert_eq!(q.take(), Some(3));
        assert_eq!(q.dropped, 2);
    }
}
