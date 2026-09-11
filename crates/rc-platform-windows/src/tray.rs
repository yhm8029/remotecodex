use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    RemoteConnected,
    GuiControlled,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenDesktop,
    BlockRemote,
    ResumeRemote,
    StopGui,
    ShutdownAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraySnapshot {
    pub state: TrayState,
    pub live_ptys: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub action: TrayAction,
    pub label: String,
    pub enabled: bool,
}

impl TrayState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Waiting",
            Self::RemoteConnected => "Remote connected",
            Self::GuiControlled => "GUI control active",
            Self::Blocked => "Remote access blocked",
        }
    }
}

pub fn menu(snapshot: TraySnapshot) -> Vec<MenuItem> {
    let blocked = snapshot.state == TrayState::Blocked;
    let count = snapshot.live_ptys;
    vec![
        MenuItem {
            action: TrayAction::OpenDesktop,
            label: "Open RemoteCodex".into(),
            enabled: true,
        },
        MenuItem {
            action: TrayAction::BlockRemote,
            label: "Block remote access".into(),
            enabled: !blocked,
        },
        MenuItem {
            action: TrayAction::ResumeRemote,
            label: "Resume remote access".into(),
            enabled: blocked,
        },
        MenuItem {
            action: TrayAction::StopGui,
            label: "Stop GUI control".into(),
            enabled: snapshot.state == TrayState::GuiControlled,
        },
        MenuItem {
            action: TrayAction::ShutdownAgent,
            label: format!(
                "Exit Agent ({count} terminal{} running)",
                if count == 1 { "" } else { "s" }
            ),
            enabled: true,
        },
    ]
}

pub fn shutdown_confirmation(snapshot: TraySnapshot) -> String {
    let count = snapshot.live_ptys;
    format!(
        "Exiting the Agent will end {count} live terminal{}. Confirm shutdown?",
        if count == 1 { "" } else { "s" }
    )
}

/// Supplies the latest state for the tray icon, tooltip, and menu.
pub type SnapshotCallback = Arc<dyn Fn() -> TraySnapshot + Send + Sync + 'static>;

pub type TraySnapshotCallback = SnapshotCallback;
pub type SnapshotProvider = SnapshotCallback;

/// Handles an action selected from the notification-area menu.
pub type ActionCallback = Arc<dyn Fn(TrayAction) -> anyhow::Result<()> + Send + Sync + 'static>;

pub type TrayActionCallback = ActionCallback;
pub type ActionHandler = ActionCallback;

#[cfg(windows)]
mod native {
    use super::{menu, shutdown_confirmation, ActionCallback, SnapshotCallback, TrayAction};
    use anyhow::{anyhow, bail, Result};
    use std::{
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcessId},
        UI::{Shell::*, WindowsAndMessaging::*},
    };

    const TRAY_ID: u32 = 1;
    const TRAY_CALLBACK: u32 = WM_APP + 41;
    const MENU_OPEN_DESKTOP: usize = 1;
    const MENU_BLOCK_REMOTE: usize = 2;
    const MENU_RESUME_REMOTE: usize = 3;
    const MENU_STOP_GUI: usize = 4;
    const MENU_SHUTDOWN_AGENT: usize = 5;

    unsafe extern "system" fn tray_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn fixed_wide<const N: usize>(value: &str) -> [u16; N] {
        let mut result = [0u16; N];
        for (slot, code_unit) in result
            .iter_mut()
            .take(N.saturating_sub(1))
            .zip(value.encode_utf16())
        {
            *slot = code_unit;
        }
        result
    }

    fn menu_command(command: usize) -> Option<TrayAction> {
        match command {
            MENU_OPEN_DESKTOP => Some(TrayAction::OpenDesktop),
            MENU_BLOCK_REMOTE => Some(TrayAction::BlockRemote),
            MENU_RESUME_REMOTE => Some(TrayAction::ResumeRemote),
            MENU_STOP_GUI => Some(TrayAction::StopGui),
            MENU_SHUTDOWN_AGENT => Some(TrayAction::ShutdownAgent),
            _ => None,
        }
    }

    fn notify_data(hwnd: HWND, icon: HICON, snapshot: super::TraySnapshot) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.hIcon = icon;
        data.szTip = fixed_wide(&format!("RemoteCodex: {}", snapshot.state.label()));
        data
    }

    struct TrayResources {
        hwnd: HWND,
        icon: HICON,
        class_name: Vec<u16>,
        icon_added: bool,
    }

    impl Drop for TrayResources {
        fn drop(&mut self) {
            unsafe {
                if self.icon_added {
                    let mut data: NOTIFYICONDATAW = zeroed();
                    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                    data.hWnd = self.hwnd;
                    data.uID = TRAY_ID;
                    Shell_NotifyIconW(NIM_DELETE, &data);
                }
                if !self.hwnd.is_null() {
                    DestroyWindow(self.hwnd);
                }
                if !self.class_name.is_empty() {
                    UnregisterClassW(self.class_name.as_ptr(), GetModuleHandleW(null()));
                }
            }
        }
    }

    fn add_icon(hwnd: HWND, icon: HICON, snapshot: super::TraySnapshot) -> Result<()> {
        let mut data = notify_data(hwnd, icon, snapshot);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = TRAY_CALLBACK;
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            bail!("Shell_NotifyIconW(NIM_ADD) failed");
        }
        Ok(())
    }

    fn update_tooltip(resources: &TrayResources, snapshot: super::TraySnapshot) -> Result<()> {
        let mut data = notify_data(resources.hwnd, resources.icon, snapshot);
        data.uFlags = NIF_TIP | NIF_SHOWTIP;
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) } == 0 {
            bail!("Shell_NotifyIconW(NIM_MODIFY) failed");
        }
        Ok(())
    }

    fn show_message(hwnd: HWND, title: &str, message: &str, style: MESSAGEBOX_STYLE) {
        let title = wide(title);
        let message = wide(message);
        unsafe {
            MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), style);
        }
    }

    fn show_handler_error(hwnd: HWND, action: TrayAction, error: anyhow::Error) {
        show_message(
            hwnd,
            "RemoteCodex",
            &format!("Unable to complete {action:?}: {error}"),
            MB_OK | MB_ICONERROR,
        );
    }

    fn popup_menu(hwnd: HWND, snapshot: super::TraySnapshot) -> Result<Option<TrayAction>> {
        let popup = unsafe { CreatePopupMenu() };
        if popup.is_null() {
            bail!("CreatePopupMenu failed");
        }

        let result = (|| {
            for (index, item) in menu(snapshot).into_iter().enumerate() {
                let command = index + 1;
                let text = wide(&item.label);
                let flags = if item.enabled {
                    MF_STRING | MF_ENABLED
                } else {
                    MF_STRING | MF_DISABLED | MF_GRAYED
                };
                if unsafe { AppendMenuW(popup, flags, command, text.as_ptr()) } == 0 {
                    bail!("AppendMenuW failed");
                }
            }

            let mut point: POINT = unsafe { zeroed() };
            if unsafe { GetCursorPos(&mut point) } == 0 {
                bail!("GetCursorPos failed");
            }
            unsafe {
                SetForegroundWindow(hwnd);
            }
            let command = unsafe {
                TrackPopupMenu(
                    popup,
                    TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    0,
                    hwnd,
                    null(),
                )
            };
            Ok(menu_command(command as usize))
        })();

        unsafe {
            DestroyMenu(popup);
        }
        result
    }

    fn dispatch_selected(
        resources: &TrayResources,
        snapshot: &SnapshotCallback,
        action_handler: &ActionCallback,
        selected: TrayAction,
    ) -> bool {
        if selected == TrayAction::ShutdownAgent {
            let current = snapshot();
            let prompt = wide(&shutdown_confirmation(current));
            let title = wide("RemoteCodex");
            let answer = unsafe {
                MessageBoxW(
                    resources.hwnd,
                    prompt.as_ptr(),
                    title.as_ptr(),
                    MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                )
            };
            if answer != IDYES {
                return false;
            }
        }

        match action_handler(selected) {
            Ok(()) if selected == TrayAction::ShutdownAgent => true,
            Ok(()) => false,
            Err(error) => {
                show_handler_error(resources.hwnd, selected, error);
                false
            }
        }
    }

    pub(super) fn run_tray(snapshot: SnapshotCallback, action: ActionCallback) -> Result<()> {
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            bail!("GetModuleHandleW failed");
        }

        let class_name = wide(&format!(
            "RemoteCodexTray-{}-{}",
            unsafe { GetCurrentProcessId() },
            std::process::id()
        ));
        let class = WNDCLASSW {
            lpfnWndProc: Some(tray_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..unsafe { zeroed() }
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            let error = unsafe { GetLastError() };
            bail!("RegisterClassW failed with error {error}");
        }

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                null(),
            )
        };
        if hwnd.is_null() {
            unsafe {
                UnregisterClassW(class_name.as_ptr(), instance);
            }
            bail!("CreateWindowExW failed");
        }

        let icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
        if icon.is_null() {
            unsafe {
                DestroyWindow(hwnd);
                UnregisterClassW(class_name.as_ptr(), instance);
            }
            bail!("LoadIconW failed");
        }

        let initial = snapshot();
        let mut resources = TrayResources {
            hwnd,
            icon,
            class_name,
            icon_added: false,
        };
        add_icon(resources.hwnd, resources.icon, initial)?;
        resources.icon_added = true;

        let mut message: MSG = unsafe { zeroed() };
        loop {
            let code = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
            if code == -1 {
                return Err(anyhow!("GetMessageW failed"));
            }
            if code == 0 {
                // WM_QUIT is only expected during process teardown. The tray itself never posts it.
                break;
            }
            if message.hwnd != resources.hwnd || message.message != TRAY_CALLBACK {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                continue;
            }

            match message.lParam as u32 {
                WM_RBUTTONUP => {
                    let current = snapshot();
                    if let Err(error) = update_tooltip(&resources, current) {
                        show_message(
                            resources.hwnd,
                            "RemoteCodex",
                            &error.to_string(),
                            MB_OK | MB_ICONERROR,
                        );
                    }
                    match popup_menu(resources.hwnd, current) {
                        Ok(Some(selected)) => {
                            if dispatch_selected(&resources, &snapshot, &action, selected) {
                                break;
                            }
                            if let Err(error) = update_tooltip(&resources, snapshot()) {
                                show_message(
                                    resources.hwnd,
                                    "RemoteCodex",
                                    &error.to_string(),
                                    MB_OK | MB_ICONERROR,
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(error) => show_message(
                            resources.hwnd,
                            "RemoteCodex",
                            &error.to_string(),
                            MB_OK | MB_ICONERROR,
                        ),
                    }
                }
                WM_LBUTTONDBLCLK => {
                    if let Err(error) = action(TrayAction::OpenDesktop) {
                        show_handler_error(resources.hwnd, TrayAction::OpenDesktop, error);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn run_tray(snapshot: SnapshotCallback, action: ActionCallback) -> anyhow::Result<()> {
    native::run_tray(snapshot, action)
}

#[cfg(not(windows))]
pub fn run_tray(_: SnapshotCallback, _: ActionCallback) -> anyhow::Result<()> {
    anyhow::bail!("Windows notification-area tray is unavailable on this target")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(state: TrayState, live_ptys: usize) -> TraySnapshot {
        TraySnapshot { state, live_ptys }
    }

    #[test]
    fn exposes_all_state_labels() {
        assert_eq!(TrayState::Idle.label(), "Waiting");
        assert_eq!(TrayState::RemoteConnected.label(), "Remote connected");
        assert_eq!(TrayState::GuiControlled.label(), "GUI control active");
        assert_eq!(TrayState::Blocked.label(), "Remote access blocked");
    }

    #[test]
    fn blocked_state_switches_block_and_resume_actions() {
        let items = menu(snapshot(TrayState::Blocked, 0));
        assert!(!items[1].enabled);
        assert!(items[2].enabled);
        let items = menu(snapshot(TrayState::Idle, 0));
        assert!(items[1].enabled);
        assert!(!items[2].enabled);
    }

    #[test]
    fn stop_gui_is_enabled_only_while_gui_controlled() {
        for state in [
            TrayState::Idle,
            TrayState::RemoteConnected,
            TrayState::Blocked,
        ] {
            assert!(!menu(snapshot(state, 0))[3].enabled);
        }
        assert!(menu(snapshot(TrayState::GuiControlled, 0))[3].enabled);
    }

    #[test]
    fn menu_action_order_is_stable() {
        assert_eq!(
            menu(snapshot(TrayState::Idle, 0))
                .into_iter()
                .map(|item| item.action)
                .collect::<Vec<_>>(),
            vec![
                TrayAction::OpenDesktop,
                TrayAction::BlockRemote,
                TrayAction::ResumeRemote,
                TrayAction::StopGui,
                TrayAction::ShutdownAgent
            ]
        );
    }

    #[test]
    fn shutdown_copy_uses_live_count_and_grammar() {
        for (count, label, confirmation) in [
            (
                0,
                "Exit Agent (0 terminals running)",
                "Exiting the Agent will end 0 live terminals. Confirm shutdown?",
            ),
            (
                1,
                "Exit Agent (1 terminal running)",
                "Exiting the Agent will end 1 live terminal. Confirm shutdown?",
            ),
            (
                3,
                "Exit Agent (3 terminals running)",
                "Exiting the Agent will end 3 live terminals. Confirm shutdown?",
            ),
        ] {
            let value = snapshot(TrayState::Idle, count);
            assert_eq!(menu(value)[4].label, label);
            assert_eq!(shutdown_confirmation(value), confirmation);
        }
    }
}
