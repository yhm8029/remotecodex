#[cfg(windows)]
mod windows_fixture {
    use anyhow::{bail, Context, Result};
    use serde_json::json;
    use std::{io::Write, mem::zeroed, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
        UI::WindowsAndMessaging::*,
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        match message {
            WM_TIMER => {
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                InvalidateRect(hwnd, std::ptr::null(), 0);
                0
            }
            WM_PAINT => {
                let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                let hdc = BeginPaint(hwnd, &mut ps);
                if !hdc.is_null() {
                    let mut rc = std::mem::zeroed::<RECT>();
                    GetClientRect(hwnd, &mut rc);
                    let brush = GetStockObject(DC_BRUSH) as HBRUSH;
                    let c = COUNTER.load(std::sync::atomic::Ordering::Relaxed);
                    let max_w = (rc.right - rc.left).max(1) as u32;
                    let off = (c.wrapping_mul(20)) % max_w;
                    let color_a: u32 = 0x001A1A2E;
                    let color_b: u32 = 0x00162030;
                    SetDCBrushColor(hdc, color_a);
                    FillRect(hdc, &rc, brush);
                    SetDCBrushColor(hdc, color_b);
                    let mut inner = RECT {
                        left: rc.left + off as i32,
                        top: rc.top + (rc.bottom - rc.top) / 4,
                        right: rc.left + off as i32 + max_w as i32 / 3,
                        bottom: rc.bottom - (rc.bottom - rc.top) / 4,
                    };
                    FillRect(hdc, &inner, brush);
                    EndPaint(hwnd, &mut ps);
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, 1);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
    pub fn run() -> Result<()> {
        let args: Vec<String> = std::env::args().collect();
        if args.len() != 5 {
            bail!("usage: record-path width height fps");
        }
        let path = args[1].clone();
        let width: i32 = args[2].parse().context("width")?;
        let height: i32 = args[3].parse().context("height")?;
        let fps: u32 = args[4].parse().context("fps")?;
        if !(2..=1920).contains(&width) {
            bail!("width out of range");
        }
        if !(2..=1080).contains(&height) {
            bail!("height out of range");
        }
        if !(1..=30).contains(&fps) {
            bail!("fps out of range");
        }

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("open record")?;

        let mut hwnd: HWND = null_mut();
        let cleanup = |hwnd: HWND, file: &mut std::fs::File| {
            unsafe {
                if !hwnd.is_null() && IsWindow(hwnd) != 0 {
                    DestroyWindow(hwnd);
                }
            }
            let _ = file.flush();
        };

        let result: Result<()> = (|| unsafe {
            let hinst = GetModuleHandleW(null_mut());
            let class_name = wide("RemoteCodexAnim");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH,
                lpszMenuName: null_mut(),
                lpszClassName: class_name.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 {
                bail!("RegisterClassW failed");
            }

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            if AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, 0, 0) == 0 {
                bail!("AdjustWindowRectEx failed");
            }

            let title = wide("RemoteCodexAnim");
            hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                null_mut(),
                null_mut(),
                hinst,
                null_mut(),
            );
            if hwnd.is_null() {
                bail!("CreateWindowExW failed");
            }

            let mut client = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetClientRect(hwnd, &mut client) == 0 {
                bail!("GetClientRect failed");
            }
            let cw = client.right - client.left;
            let ch = client.bottom - client.top;
            if cw != width || ch != height {
                bail!("client size mismatch");
            }

            let mut ft_creation: FILETIME = zeroed();
            let mut ft_exit: FILETIME = zeroed();
            let mut ft_kernel: FILETIME = zeroed();
            let mut ft_user: FILETIME = zeroed();
            if GetProcessTimes(
                GetCurrentProcess(),
                &mut ft_creation,
                &mut ft_exit,
                &mut ft_kernel,
                &mut ft_user,
            ) == 0
            {
                bail!("GetProcessTimes failed");
            }
            let raw =
                ((ft_creation.dwHighDateTime as u64) << 32) | (ft_creation.dwLowDateTime as u64);

            if SetTimer(hwnd, 1, (1000 / fps) as u32, None) == 0 {
                bail!("SetTimer failed");
            }

            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            UpdateWindow(hwnd);

            let obj = json!({
                "pid": std::process::id(),
                "process_created": raw.to_string(),
                "hwnd": (hwnd as usize).to_string(),
                "client_width": cw,
                "client_height": ch,
                "fps": fps,
            });
            serde_json::to_writer(&mut file, &obj).context("write json")?;
            file.write_all(b"\n").context("write newline")?;
            file.flush().context("flush")?;

            let mut msg: MSG = zeroed();
            loop {
                let r = GetMessageW(&mut msg, null_mut(), 0, 0);
                if r == -1 {
                    bail!("GetMessageW error");
                }
                if r == 0 {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            Ok(())
        })();

        cleanup(hwnd, &mut file);
        drop(file);
        let removed = std::fs::remove_file(&path);
        result?;
        removed.context("remove owned record")?;
        Ok(())
    }
}
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    windows_fixture::run()
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}
