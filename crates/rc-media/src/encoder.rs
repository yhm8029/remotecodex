use rc_core::{
    geometry::Rect,
    media_wire::{MediaMode, VideoProfile},
};
use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncoderChoice {
    Hardware,
    SoftwareViewOnlySlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncoderUnavailable;

impl fmt::Display for EncoderUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no supported media encoder is available")
    }
}

impl Error for EncoderUnavailable {}

impl EncoderChoice {
    pub(crate) fn select(hw_d3d11: bool, openh264: bool) -> Result<Self, EncoderUnavailable> {
        if hw_d3d11 {
            Ok(Self::Hardware)
        } else if openh264 {
            Ok(Self::SoftwareViewOnlySlow)
        } else {
            Err(EncoderUnavailable)
        }
    }

    pub(crate) fn profile(
        self,
        requested: VideoProfile,
        source: Rect,
    ) -> Result<VideoProfile, String> {
        match self {
            Self::Hardware => requested.fit(source),
            Self::SoftwareViewOnlySlow => VideoProfile {
                width: requested.width.min(640),
                height: requested.height.min(360),
                fps: requested.fps.min(5),
                bitrate_kbps: requested.bitrate_kbps.min(768),
            }
            .fit(source),
        }
    }

    pub(crate) fn mode(self) -> MediaMode {
        match self {
            Self::Hardware => MediaMode::Hardware,
            Self::SoftwareViewOnlySlow => MediaMode::SoftwareViewOnlySlow,
        }
    }

    pub(crate) fn control_allowed(self) -> bool {
        matches!(self, Self::Hardware)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values(p: VideoProfile) -> (u32, u32, u32, u32) {
        (p.width, p.height, p.fps, p.bitrate_kbps)
    }

    #[test]
    fn hardware_is_preferred_only_with_d3d11_interop() {
        assert_eq!(
            EncoderChoice::select(true, true).unwrap(),
            EncoderChoice::Hardware
        );
        assert_eq!(
            EncoderChoice::select(true, false).unwrap(),
            EncoderChoice::Hardware
        );
        assert_eq!(
            EncoderChoice::select(false, true).unwrap(),
            EncoderChoice::SoftwareViewOnlySlow
        );
    }

    #[test]
    fn missing_hardware_and_software_is_explicitly_unavailable() {
        assert_eq!(EncoderChoice::select(false, false), Err(EncoderUnavailable));
    }

    #[test]
    fn software_profile_is_bounded_and_preserves_aspect_ratio() {
        let source = rc_core::geometry::Rect {
            left: 0,
            top: 0,
            width: 1_920,
            height: 1_200,
        };
        let requested = rc_core::media_wire::VideoProfile {
            width: 1_280,
            height: 720,
            fps: 15,
            bitrate_kbps: 2_500,
        };
        assert_eq!(
            values(
                EncoderChoice::SoftwareViewOnlySlow
                    .profile(requested, source)
                    .unwrap()
            ),
            values(rc_core::media_wire::VideoProfile {
                width: 576,
                height: 360,
                fps: 5,
                bitrate_kbps: 768
            })
        );
    }

    #[test]
    fn software_profile_does_not_raise_a_lower_request() {
        let source = rc_core::geometry::Rect {
            left: 0,
            top: 0,
            width: 640,
            height: 360,
        };
        let requested = rc_core::media_wire::VideoProfile {
            width: 640,
            height: 360,
            fps: 2,
            bitrate_kbps: 300,
        };
        assert_eq!(
            values(
                EncoderChoice::SoftwareViewOnlySlow
                    .profile(requested, source)
                    .unwrap()
            ),
            values(requested)
        );
    }
}
