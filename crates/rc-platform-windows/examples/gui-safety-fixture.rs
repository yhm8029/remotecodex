#[cfg(windows)]
mod windows_fixture {

    use anyhow::{bail, Result};
    use rc_core::{
        geometry::Rect,
        media_wire::{GuiAction, LocalSource, NativeSource},
    };
    use rc_platform_windows::{gui_session::GuiSession, process_created, validate_source};
    use std::{
        mem::zeroed,
        ptr::null_mut,
        sync::atomic::{AtomicU32, Ordering},
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::{ClientToScreen, UpdateWindow},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };

    static BUTTON_COUNT: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_COMMAND if (lparam as HWND) == button_handle() => {
                if (wparam as u32 >> 16) == BN_CLICKED {
                    BUTTON_COUNT.fetch_add(1, Ordering::AcqRel);
                }
                0
            }
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

    static mut BUTTON: HWND = null_mut();
    fn button_handle() -> HWND {
        unsafe { BUTTON }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    struct FixtureWindows {
        primary: HWND,
        secondary: HWND,
        edit: HWND,
        secondary_edit: HWND,
    }
    impl Drop for FixtureWindows {
        fn drop(&mut self) {
            unsafe {
                if !self.secondary.is_null() {
                    DestroyWindow(self.secondary);
                }
                if !self.primary.is_null() {
                    DestroyWindow(self.primary);
                }
            }
        }
    }

    fn create_windows() -> Result<FixtureWindows> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_name = wide("RemoteCodexOwnedGuiFixture");
            let mut class: WNDCLASSW = zeroed();
            class.hInstance = instance;
            class.lpfnWndProc = Some(window_proc);
            class.lpszClassName = class_name.as_ptr();
            RegisterClassW(&class);
            let title = wide("RemoteCodex owned input fixture");
            let second_title = wide("RemoteCodex owned foreground rejection fixture");
            let primary = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                120,
                120,
                640,
                360,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            let secondary = CreateWindowExW(
                0,
                class_name.as_ptr(),
                second_title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                820,
                120,
                360,
                240,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if primary.is_null() || secondary.is_null() {
                bail!("owned fixture window creation failed");
            }
            let edit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_BORDER | ES_LEFT as u32,
                20,
                30,
                560,
                80,
                primary,
                null_mut(),
                instance,
                null_mut(),
            );
            let button = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("Owned fixture button").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
                20,
                140,
                220,
                50,
                primary,
                null_mut(),
                instance,
                null_mut(),
            );
            let secondary_edit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_BORDER | ES_LEFT as u32,
                20,
                30,
                280,
                80,
                secondary,
                null_mut(),
                instance,
                null_mut(),
            );
            if secondary_edit.is_null() {
                bail!("owned secondary text control failed");
            }
            BUTTON = button;
            if edit.is_null() || button.is_null() {
                bail!("owned fixture controls failed");
            }
            ShowWindow(primary, SW_SHOW);
            ShowWindow(secondary, SW_SHOW);
            UpdateWindow(primary);
            UpdateWindow(secondary);
            Ok(FixtureWindows {
                primary,
                secondary,
                edit,
                secondary_edit,
            })
        }
    }

    fn client_rect(hwnd: HWND) -> Result<Rect> {
        unsafe {
            let mut rect: RECT = zeroed();
            let mut point = POINT { x: 0, y: 0 };
            if GetClientRect(hwnd, &mut rect) == 0 || ClientToScreen(hwnd, &mut point) == 0 {
                bail!("owned fixture geometry unavailable");
            }
            Ok(Rect {
                left: point.x,
                top: point.y,
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            })
        }
    }

    fn pump_messages(ms: u64) {
        let deadline = std::time::Instant::now() + Duration::from_millis(ms);
        unsafe {
            let mut message: MSG = zeroed();
            while std::time::Instant::now() < deadline {
                while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn text(hwnd: HWND) -> String {
        unsafe {
            let mut buffer = vec![0u16; 2048];
            let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            String::from_utf16_lossy(&buffer[..len.max(0) as usize])
        }
    }

    fn source(hwnd: HWND, label: &str) -> Result<LocalSource> {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let created = process_created(pid)?;
            Ok(LocalSource {
                native: NativeSource::Window {
                    handle: (hwnd as usize as u64).to_string(),
                    pid,
                    created: created.to_string(),
                },
                label: label.into(),
                rect: client_rect(hwnd)?,
            })
        }
    }

    fn focus_owned(windows: &FixtureWindows, target: HWND, focus: HWND) -> Result<()> {
        let fg = unsafe { GetForegroundWindow() };
        if fg != windows.primary && fg != windows.secondary {
            anyhow::bail!("User moved focus away from owned fixture");
        }
        unsafe { SetForegroundWindow(target) };
        unsafe { SetFocus(focus) };
        pump_messages(10);
        let new_fg = unsafe { GetForegroundWindow() };
        if new_fg != target {
            anyhow::bail!("Owned fixture did not gain foreground");
        }
        Ok(())
    }

    pub fn run() -> anyhow::Result<()> {
        let windows = create_windows()?;
        focus_owned(&windows, windows.primary, windows.edit)?;
        pump_messages(100);
        let primary_source = source(windows.primary, "owned primary fixture")?;
        let approved_rect = validate_source(&primary_source)?;
        if approved_rect != primary_source.rect {
            bail!("owned fixture geometry changed before session");
        }
        let session = GuiSession::start(primary_source.clone())?;
        session.frame()?;
        let epoch = session.arm()?;
        session.action(
            epoch,
            GuiAction::Text {
                text: "owned fixture text".into(),
            },
        )?;
        pump_messages(100);
        let received_text = text(windows.edit);
        let button_point = (
            (20.0 + 110.0) / primary_source.rect.width as f64,
            (140.0 + 25.0) / primary_source.rect.height as f64,
        );
        session.action(
            epoch,
            GuiAction::Button {
                x: button_point.0,
                y: button_point.1,
                button: 0,
                down: true,
            },
        )?;
        session.action(
            epoch,
            GuiAction::Button {
                x: button_point.0,
                y: button_point.1,
                button: 0,
                down: false,
            },
        )?;
        pump_messages(100);
        let button_count = BUTTON_COUNT.load(Ordering::Acquire);

        let key_was_up = unsafe { (GetAsyncKeyState(VK_F1 as i32) as i32) & 0x8000 == 0 };
        if !key_was_up {
            bail!("F1 is already held; refusing key-release fixture");
        }
        use std::time::{Duration, Instant};

        let key_down_observed = {
            session.frame()?;
            session.action(
                epoch,
                GuiAction::Key {
                    scan: 0x3b,
                    extended: false,
                    down: true,
                },
            )?;
            let deadline = Instant::now() + Duration::from_millis(200);
            let mut observed = false;
            while Instant::now() < deadline {
                pump_messages(5);
                if unsafe { (GetAsyncKeyState(VK_F1 as i32) as i32) & 0x8000 != 0 } {
                    observed = true;
                    break;
                }
            }
            session.disarm();
            observed
        };
        let key_released = {
            let deadline = Instant::now() + Duration::from_millis(200);
            let mut released = false;
            while Instant::now() < deadline {
                pump_messages(5);
                if unsafe { (GetAsyncKeyState(VK_F1 as i32) as i32) & 0x8000 == 0 } {
                    released = true;
                    break;
                }
            }
            released
        };
        if !key_down_observed {
            bail!("Owned keydown was not observed before release");
        }

        drop(session);
        focus_owned(&windows, windows.primary, windows.edit)?;
        let foreground_session = GuiSession::start(primary_source.clone())?;
        foreground_session.frame()?;
        let foreground_epoch = foreground_session.arm()?;
        let saved_edit_text = text(windows.edit);
        let saved_secondary_edit_text = text(windows.secondary_edit);
        let saved_button_count = BUTTON_COUNT.load(Ordering::Acquire);
        focus_owned(&windows, windows.secondary, windows.secondary_edit)?;
        let observed_foreground = unsafe { GetForegroundWindow() };
        if observed_foreground != windows.secondary {
            bail!("Wrong owned foreground");
        }
        let posted_while_wrong_foreground = foreground_session
            .action(
                foreground_epoch,
                GuiAction::Text {
                    text: "must be rejected".into(),
                },
            )
            .is_ok();
        pump_messages(150);
        let foreground_still_armed = foreground_session.armed();
        let foreground_rejected = !foreground_still_armed
            && text(windows.edit) == saved_edit_text
            && text(windows.secondary_edit) == saved_secondary_edit_text
            && BUTTON_COUNT.load(Ordering::Acquire) == saved_button_count;
        foreground_session.disarm();
        drop(foreground_session);
        focus_owned(&windows, windows.primary, windows.edit)?;

        let good_session = GuiSession::start(primary_source.clone())?;
        good_session.frame()?;
        good_session.arm()?;
        good_session.disarm();
        drop(good_session);

        let mut stale = primary_source.clone();
        stale.rect.width = stale.rect.width.saturating_add(1);
        let stale_session = GuiSession::start(stale.clone())?;
        stale_session.frame()?;
        let geometry_error = stale_session.arm().err().map(|error| error.to_string());

        let stale_geometry_rejected = validate_source(&stale).ok() == Some(approved_rect)
            && geometry_error.as_deref() == Some("Geometry changed");

        stale_session.disarm();
        drop(stale_session);
        let mut wrong_identity = primary_source.clone();
        if let NativeSource::Window { pid, .. } = &mut wrong_identity.native {
            *pid = pid.saturating_add(1);
        }
        let source_identity_rejected = validate_source(&wrong_identity).is_err();
        println!(
            "{}",
            serde_json::json!({
                "scope": "owned fixture GUI input only",
                "received_text": received_text,
                "button_count": button_count,
                "key_down_observed": key_down_observed,
                "key_released": key_released,
                "foreground_rejected": foreground_rejected,
                "foreground_handle": observed_foreground as usize as u64,
                "secondary_handle": windows.secondary as usize as u64,
                "foreground_action_posted": posted_while_wrong_foreground,
                "foreground_still_armed": foreground_still_armed,
                "stale_geometry_rejected": stale_geometry_rejected,
                "source_identity_rejected": source_identity_rejected
            })
        );
        if received_text != "owned fixture text"
            || button_count != 1
            || !key_released
            || !foreground_rejected
            || !stale_geometry_rejected
            || !source_identity_rejected
        {
            bail!("owned GUI safety fixture failed");
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
