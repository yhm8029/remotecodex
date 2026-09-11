use serde::Serialize;

const BUCKET_WIDTH: u64 = 100;
const BUCKET_COUNT: usize = 1001;

#[derive(Debug, Clone, Serialize)]
pub struct Histogram {
    pub count: u64,
    pub max_us: u64,
    pub sum_us: u128,
    pub buckets: Vec<u64>,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            count: 0,
            max_us: 0,
            sum_us: 0,
            buckets: vec![0u64; BUCKET_COUNT],
        }
    }

    pub fn record(&mut self, us: u64) {
        self.count += 1;
        self.sum_us += us as u128;
        if us > self.max_us {
            self.max_us = us;
        }
        let idx = (us / BUCKET_WIDTH).min((BUCKET_COUNT - 1) as u64) as usize;
        self.buckets[idx] += 1;
    }

    pub fn upper_quantile(&self, p: f64) -> Option<u64> {
        if self.count == 0 {
            return None;
        }
        let rank = ((p * self.count as f64).ceil() as u64)
            .max(1)
            .min(self.count);
        let mut cumulative: u64 = 0;
        for (idx, &b) in self.buckets.iter().enumerate() {
            cumulative += b;
            if cumulative >= rank {
                return Some(if idx == BUCKET_COUNT - 1 {
                    self.max_us
                } else {
                    ((idx as u64) * BUCKET_WIDTH + (BUCKET_WIDTH - 1)).min(self.max_us)
                });
            }
        }
        Some(self.max_us)
    }

    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum_us as f64 / self.count as f64)
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_quantile_is_none() {
        let h = Histogram::new();
        assert!(h.upper_quantile(0.5).is_none());
        assert!(h.mean().is_none());
        assert_eq!(h.count, 0);
        assert_eq!(h.buckets.len(), BUCKET_COUNT);
        assert!(h.buckets.iter().all(|&b| b == 0));
    }

    #[test]
    fn fixed_samples() {
        let mut h = Histogram::new();
        for s in [0u64, 99, 100, 100_000, 200_000] {
            h.record(s);
        }
        assert_eq!(h.count, 5);
        assert_eq!(h.sum_us, 300_199);
        assert_eq!(h.max_us, 200_000);
        assert_eq!(h.buckets[0], 2);
        assert_eq!(h.buckets[1], 1);
        assert_eq!(h.buckets[1000], 2);
        assert_eq!(h.upper_quantile(0.5), Some(199));
        assert_eq!(h.upper_quantile(0.95), Some(200_000));
        assert!((h.mean().unwrap() - (300_199.0 / 5.0)).abs() < 1e-9);
    }
}
