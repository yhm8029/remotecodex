//! P4/P6 native media implementation. Only this optional helper links the multimedia SDK.
//! Agent/terminal-only builds never load GStreamer DLLs or start a capture thread.
#[cfg(all(windows, feature = "native-media"))]
pub mod native;
#[derive(Debug, serde::Serialize)]
pub struct Capabilities {
    pub protocol: u16,
    pub native_backend: bool,
    pub available: bool,
    pub sdk_version: Option<String>,
    pub missing: Vec<String>,
    pub windows_runtime_verified: bool,
}
#[cfg(all(windows, feature = "native-media"))]
pub fn capabilities() -> Capabilities {
    native::probe()
}
#[cfg(not(all(windows, feature = "native-media")))]
pub fn capabilities() -> Capabilities {
    Capabilities{protocol:1,native_backend:false,available:false,sdk_version:None,missing:vec!["Build rc-media on Windows with --features native-media and the documented native GStreamer SDK".into()],windows_runtime_verified:false}
}
#[cfg(test)]
mod tests {
    #[test]
    fn plain_build_has_no_video_dependencies() {
        #[cfg(not(all(windows, feature = "native-media")))]
        assert!(!super::capabilities().available);
    }
}
