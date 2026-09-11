//! Scoped per-monitor-v2 coordinates for synchronous native operations only.
use anyhow::{bail, Result};
use std::{marker::PhantomData, rc::Rc};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
/// !Send/!Sync: the saved context belongs to the creating OS thread.
pub(crate) struct DpiGuard {
    previous: DPI_AWARENESS_CONTEXT,
    _thread: PhantomData<Rc<()>>,
}
impl DpiGuard {
    pub(crate) fn enter() -> Result<Self> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.is_null() {
            bail!("Per-monitor DPI context unavailable");
        }
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }
}
impl Drop for DpiGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.previous);
        }
    }
}
