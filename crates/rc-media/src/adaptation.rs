//! Deterministic rate policy driven only by newly captured source buffers.

const IDLE_AFTER_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdaptiveTarget {
    pub(crate) fps: u32,
    pub(crate) bitrate_kbps: u32,
    pub(crate) adaptive_idle: bool,
}

pub(crate) struct AdaptivePolicy {
    requested_fps: u32,
    requested_bitrate_kbps: u32,
    last_source_ms: u64,
}

impl AdaptivePolicy {
    pub(crate) fn new(fps: u32, bitrate_kbps: u32, now_ms: u64) -> Self {
        Self {
            requested_fps: fps,
            requested_bitrate_kbps: bitrate_kbps,
            last_source_ms: now_ms,
        }
    }

    pub(crate) fn observe_source(&mut self, now_ms: u64) {
        self.last_source_ms = self.last_source_ms.max(now_ms);
    }

    pub(crate) fn target(&self, now_ms: u64) -> AdaptiveTarget {
        if now_ms.saturating_sub(self.last_source_ms) < IDLE_AFTER_MS {
            return AdaptiveTarget {
                fps: self.requested_fps,
                bitrate_kbps: self.requested_bitrate_kbps,
                adaptive_idle: false,
            };
        }
        AdaptiveTarget {
            fps: self.requested_fps.min(3),
            bitrate_kbps: (self.requested_bitrate_kbps / 4).max(256),
            adaptive_idle: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_target_becomes_idle_at_exact_boundary() {
        let policy = AdaptivePolicy::new(15, 2_500, 100);
        assert_eq!(
            policy.target(2_099),
            AdaptiveTarget {
                fps: 15,
                bitrate_kbps: 2_500,
                adaptive_idle: false
            }
        );
        assert_eq!(
            policy.target(2_100),
            AdaptiveTarget {
                fps: 3,
                bitrate_kbps: 625,
                adaptive_idle: true
            }
        );
    }

    #[test]
    fn idle_target_clamps_fps_and_floors_bitrate() {
        let policy = AdaptivePolicy::new(2, 128, 0);
        assert_eq!(
            policy.target(2_000),
            AdaptiveTarget {
                fps: 2,
                bitrate_kbps: 256,
                adaptive_idle: true
            }
        );
    }

    #[test]
    fn real_source_activity_recovers_immediately() {
        let mut policy = AdaptivePolicy::new(15, 2_500, 0);
        assert!(policy.target(u64::MAX).adaptive_idle);
        policy.observe_source(u64::MAX);
        assert_eq!(
            policy.target(u64::MAX),
            AdaptiveTarget {
                fps: 15,
                bitrate_kbps: 2_500,
                adaptive_idle: false
            }
        );
    }

    #[test]
    fn backward_clock_does_not_replace_newer_source_observation() {
        let mut policy = AdaptivePolicy::new(15, 2_500, u64::MAX - 1);
        policy.observe_source(10);
        assert!(!policy.target(u64::MAX).adaptive_idle);
    }
}
