use crate::error::ErrorCode;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn valid(&self) -> bool {
        self.width > 0 && self.height > 0 && self.width <= 32768 && self.height <= 32768
    }
    pub fn map(&self, x: f64, y: f64) -> Result<(i32, i32), ErrorCode> {
        if !self.valid()
            || !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Err(ErrorCode::InvalidRequest);
        }
        let x = i64::from(self.left) + (x * f64::from(self.width - 1)).round() as i64;
        let y = i64::from(self.top) + (y * f64::from(self.height - 1)).round() as i64;
        Ok((
            i32::try_from(x).map_err(|_| ErrorCode::InvalidRequest)?,
            i32::try_from(y).map_err(|_| ErrorCode::InvalidRequest)?,
        ))
    }
    pub fn absolute(&self, x: i32, y: i32) -> Result<(i32, i32), ErrorCode> {
        if !self.valid() || self.width < 2 || self.height < 2 {
            return Err(ErrorCode::InvalidRequest);
        }
        let dx = i64::from(x) - i64::from(self.left);
        let dy = i64::from(y) - i64::from(self.top);
        if dx < 0 || dy < 0 || dx >= i64::from(self.width) || dy >= i64::from(self.height) {
            return Err(ErrorCode::InvalidRequest);
        }
        Ok((
            ((dx as f64) * 65535.0 / (self.width - 1) as f64).round() as i32,
            ((dy as f64) * 65535.0 / (self.height - 1) as f64).round() as i32,
        ))
    }
}
/// Preview pixels -> normalized source coordinate. Black letterbox never clicks.
pub fn unletterbox(
    x: f64,
    y: f64,
    view_w: f64,
    view_h: f64,
    source_w: f64,
    source_h: f64,
) -> Option<(f64, f64)> {
    if [x, y, view_w, view_h, source_w, source_h]
        .iter()
        .any(|v| !v.is_finite())
        || view_w <= 0.0
        || view_h <= 0.0
        || source_w <= 0.0
        || source_h <= 0.0
    {
        return None;
    }
    let scale = (view_w / source_w).min(view_h / source_h);
    let w = source_w * scale;
    let h = source_h * scale;
    let x = (x - (view_w - w) / 2.0) / w;
    let y = (y - (view_h - h) / 2.0) / h;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        None
    } else {
        Some((x, y))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_monitor_and_edges() {
        let r = Rect {
            left: -1920,
            top: -200,
            width: 1920,
            height: 1080,
        };
        assert_eq!(r.map(0., 0.).unwrap(), (-1920, -200));
        assert_eq!(r.map(1., 1.).unwrap(), (-1, 879));
    }
    #[test]
    fn reject_nan_and_letterbox() {
        let r = Rect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        assert!(r.map(f64::NAN, 0.).is_err());
        assert!(unletterbox(0., 0., 1000., 1000., 1920., 1080.).is_none());
        assert_eq!(
            unletterbox(500., 500., 1000., 1000., 1920., 1080.),
            Some((0.5, 0.5))
        );
    }
}
