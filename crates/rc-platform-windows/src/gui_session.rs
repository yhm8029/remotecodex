//! On-demand native safety/input thread. Blocking message pump; no idle polling thread.
//! Low-level hooks observe ONLY an interruption counter, never record keys/coordinates.
use super::gui_input::{InputBackend, INPUT_TAG};
use anyhow::{bail, Result};
use rc_core::media_wire::{GuiAction, LocalSource};
use std::{
    cell::RefCell,
    mem::zeroed,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
const WAKE: u32 = WM_APP + 91;
#[derive(Default)]
struct Safety {
    closed: AtomicBool,
    armed: AtomicBool,
    revision: AtomicU64,
}
impl Safety {
    fn revoke(&self) {
        self.armed.store(false, Ordering::Release);
        self.revision.fetch_add(1, Ordering::AcqRel);
    }
}
thread_local! {static SAFETY:RefCell<Option<Arc<Safety>>>=const{RefCell::new(None)};}
enum Command {
    Frame,
    Arm {
        expected: u64,
        reply: SyncSender<Result<u64, String>>,
    },
    Action {
        epoch: u64,
        action: GuiAction,
    },
    Disarm,
    Stop,
}
pub struct GuiSession {
    tx: SyncSender<Command>,
    thread: u32,
    safety: Arc<Safety>,
}
impl GuiSession {
    pub fn start(source: LocalSource) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(64);
        let (ready, wait) = mpsc::sync_channel(1);
        let safety = Arc::new(Safety::default());
        let shared = safety.clone();
        std::thread::Builder::new()
            .name("rc-gui-safety".into())
            .spawn(move || {
                let result = run(source, rx, shared.clone(), ready);
                shared.revoke();
                shared.closed.store(true, Ordering::Release);
                if let Err(e) = result {
                    eprintln!("RemoteCodex GUI safety stopped: {e}");
                }
            })?;
        let thread = wait
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| anyhow::anyhow!("GUI safety setup timed out"))??;
        Ok(Self { tx, thread, safety })
    }
    fn post(&self, c: Command) -> Result<()> {
        if self.closed() {
            bail!("GUI safety closed");
        }
        self.tx
            .try_send(c)
            .map_err(|_| anyhow::anyhow!("GUI input queue full/disconnected"))?;
        if unsafe { PostThreadMessageW(self.thread, WAKE, 0, 0) } == 0 {
            self.safety.revoke();
            bail!("GUI safety wake failed");
        }
        Ok(())
    }
    pub fn frame(&self) -> Result<()> {
        self.post(Command::Frame)
    }
    /// Call outside the async reactor (spawn_blocking); bounded wait, no input is queued on failure.
    pub fn arm(&self) -> Result<u64> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.post(Command::Arm {
            expected: self.revision(),
            reply: tx,
        })?;
        rx.recv_timeout(Duration::from_millis(500))
            .map_err(|_| anyhow::anyhow!("GUI arm timeout"))?
            .map_err(anyhow::Error::msg)
    }
    pub fn action(&self, epoch: u64, action: GuiAction) -> Result<()> {
        if !self.armed() || epoch != self.revision() {
            bail!("Native GUI lease stale");
        }
        self.post(Command::Action { epoch, action })
    }
    pub fn disarm(&self) {
        self.safety.revoke();
        let _ = self.post(Command::Disarm);
    }
    pub fn armed(&self) -> bool {
        self.safety.armed.load(Ordering::Acquire)
    }
    pub fn revision(&self) -> u64 {
        self.safety.revision.load(Ordering::Acquire)
    }
    pub fn closed(&self) -> bool {
        self.safety.closed.load(Ordering::Acquire)
    }
}
impl Drop for GuiSession {
    fn drop(&mut self) {
        self.safety.revoke();
        let _ = self.tx.try_send(Command::Stop);
        unsafe {
            PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
        }
    }
}
fn physical() {
    SAFETY.with(|s| {
        if let Some(s) = s.borrow().as_ref() {
            s.revoke();
        }
    });
    unsafe {
        PostThreadMessageW(GetCurrentThreadId(), WAKE, 0, 0);
    }
}
unsafe extern "system" fn mouse_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(l as *const MSLLHOOKSTRUCT);
        if event.dwExtraInfo != INPUT_TAG {
            physical();
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}
unsafe extern "system" fn key_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(l as *const KBDLLHOOKSTRUCT);
        if event.dwExtraInfo != INPUT_TAG {
            physical();
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        PostQuitMessage(0);
        return 0;
    }
    DefWindowProcW(hwnd, msg, w, l)
}
struct Resources {
    window: HWND,
    key: HHOOK,
    mouse: HHOOK,
    timer: usize,
}
impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if !self.key.is_null() {
                UnhookWindowsHookEx(self.key);
            }
            if !self.mouse.is_null() {
                UnhookWindowsHookEx(self.mouse);
            }
            KillTimer(null_mut(), self.timer);
            UnregisterHotKey(null_mut(), 91);
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
        }
    }
}
fn run(
    source: LocalSource,
    rx: mpsc::Receiver<Command>,
    s: Arc<Safety>,
    ready: SyncSender<Result<u32>>,
) -> Result<()> {
    let setup = (|| -> Result<_> {
        let dpi = super::dpi::DpiGuard::enter()?;
        unsafe {
            let mut msg: MSG = zeroed();
            PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
            if !super::input_desktop_available() {
                bail!("Desktop unavailable");
            }
            let instance = GetModuleHandleW(null());
            let class: Vec<u16> = "RemoteCodexVisibleShare\0".encode_utf16().collect();
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = instance;
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let title: Vec<u16> = "RemoteCodex: sharing active - close or Ctrl+Alt+F12 to STOP\0"
                .encode_utf16()
                .collect();
            let window = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_CAPTION | WS_SYSMENU,
                20,
                20,
                590,
                58,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if window.is_null() {
                bail!("Visible sharing indicator unavailable");
            }
            let mut res = Resources {
                window,
                key: null_mut(),
                mouse: null_mut(),
                timer: 0,
            };
            if RegisterHotKey(
                null_mut(),
                91,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                VK_F12 as u32,
            ) == 0
            {
                bail!("Emergency hotkey already used; refusing GUI sharing");
            }
            res.key = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), instance, 0);
            res.mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), instance, 0);
            if res.key.is_null() || res.mouse.is_null() {
                bail!("Local-input preemption hooks unavailable");
            }
            res.timer = SetTimer(null_mut(), 91, 100, None);
            if res.timer == 0 {
                bail!("Safety timer unavailable");
            }
            ShowWindow(window, SW_SHOWNOACTIVATE);
            Ok((res, InputBackend::new(source)?, dpi))
        }
    })();
    let (resources, mut backend, _dpi) = match setup {
        Ok(v) => v,
        Err(e) => {
            let text = e.to_string();
            let _ = ready.send(Err(anyhow::anyhow!(text)));
            return Err(e);
        }
    };
    SAFETY.with(|x| *x.borrow_mut() = Some(s.clone()));
    if ready.send(Ok(unsafe { GetCurrentThreadId() })).is_err() {
        return Ok(());
    }
    let mut msg: MSG = unsafe { zeroed() };
    loop {
        let code = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if code <= 0 || msg.message == WM_HOTKEY {
            break;
        }
        if !s.armed.load(Ordering::Acquire) {
            backend.disarm();
        }
        if msg.message == WM_TIMER {
            if !super::input_desktop_available() {
                break;
            }
            if s.armed.load(Ordering::Acquire) && backend.check_live().is_err() {
                s.revoke();
            }
        }
        if msg.message == WAKE {
            for _ in 0..64 {
                let cmd = match rx.try_recv() {
                    Ok(c) => c,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        s.closed.store(true, Ordering::Release);
                        break;
                    }
                };
                match cmd {
                    Command::Frame => backend.frame(),
                    Command::Arm { expected, reply } => {
                        let result = (|| -> Result<u64, String> {
                            if s.revision.load(Ordering::Acquire) != expected {
                                return Err("Local input intervened before arm".into());
                            }
                            backend.arm().map_err(|e| e.to_string())?;
                            let next = expected
                                .checked_add(1)
                                .ok_or_else(|| "GUI revision exhausted".to_string())?;
                            s.revision
                                .compare_exchange(
                                    expected,
                                    next,
                                    Ordering::AcqRel,
                                    Ordering::Acquire,
                                )
                                .map_err(|_| "Local input intervened during arm".to_string())?;
                            s.armed.store(true, Ordering::Release);
                            if s.revision.load(Ordering::Acquire) != next {
                                s.armed.store(false, Ordering::Release);
                                return Err("Arm was revoked".into());
                            }
                            Ok(next)
                        })();
                        if result.is_err() {
                            backend.disarm();
                            s.revoke();
                        }
                        if reply.send(result).is_err() {
                            backend.disarm();
                            s.revoke();
                        }
                    }
                    Command::Action { epoch, action } => {
                        if s.armed.load(Ordering::Acquire)
                            && s.revision.load(Ordering::Acquire) == epoch
                        {
                            if backend
                                .apply(action, || {
                                    // PeekMessage dispatches pending nonqueued messages (including low-level hooks)
                                    // between bounded text injections, without consuming queued application commands.
                                    let mut peek: MSG = unsafe { zeroed() };
                                    unsafe {
                                        PeekMessageW(&mut peek, null_mut(), 0, 0, PM_NOREMOVE);
                                    }
                                    s.armed.load(Ordering::Acquire)
                                        && s.revision.load(Ordering::Acquire) == epoch
                                        && !s.closed.load(Ordering::Acquire)
                                })
                                .is_err()
                            {
                                backend.disarm();
                                s.revoke();
                            }
                        }
                    }
                    Command::Disarm => backend.disarm(),
                    Command::Stop => {
                        s.closed.store(true, Ordering::Release);
                        break;
                    }
                }
            }
        }
        if s.closed.load(Ordering::Acquire) {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    backend.disarm();
    s.revoke();
    SAFETY.with(|x| *x.borrow_mut() = None);
    drop(resources);
    Ok(())
}
