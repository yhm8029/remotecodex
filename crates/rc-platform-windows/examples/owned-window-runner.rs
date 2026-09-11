#[cfg(windows)]
mod windows_fixture {

    use anyhow::{bail, Context, Result};
    use std::{mem::zeroed, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::*, Graphics::Gdi::UpdateWindow, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    pub fn run() -> anyhow::Result<()> {
        let record = std::env::args()
            .nth(1)
            .context("usage: owned-window-runner <record-path>")?;
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_name = wide("RemoteCodexOwnedCaptureRunner");
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.lpfnWndProc = Some(window_proc);
            class.lpszClassName = class_name.as_ptr();
            RegisterClassW(&class);
            let title = wide("RemoteCodex owned capture runner");
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                140,
                140,
                640,
                360,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                bail!("owned capture runner window creation failed");
            }
            ShowWindow(hwnd, SW_SHOW);
            // A hidden process startup can cause the first ShowWindow command to
            // be ignored; the second call makes the owned capture target visible.
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            UpdateWindow(hwnd);
            std::fs::write(
                &record,
                format!("{}\n{}\n", std::process::id(), hwnd as usize),
            )?;
            let mut msg: MSG = zeroed();
            loop {
                let code = GetMessageW(&mut msg, null_mut(), 0, 0);
                if code <= 0 {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = std::fs::remove_file(record);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    windows_fixture::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows-only fixture");
}
