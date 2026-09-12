# Tray menu repair - 2026-09-12

The notification icon appeared but right-click did not open its menu. The registered window procedure discarded shell callbacks through DefWindowProcW; handling only the MSG returned by GetMessageW missed sent callbacks.

The window procedure now posts a distinct private dispatch message, and the existing loop handles it. A hidden ordinary top-level window owns the popup. WM_NULL is posted after popup completion for repeated opening/dismissal. Legacy notification semantics and the live-terminal shutdown confirmation are preserved.

Validation:

- Two real Win32 regressions failed before the fix and passed afterward: synchronous SendMessageW and posted callback through DispatchMessageW, with payload preservation.
- rc-platform-windows: 11 tests passed. Agent regression suite: 49 passed (one separate ConPTY integration test ignored in this command).
- Owned native fixture: five-item popup visible, cancellation and repeated opening work, menu Open and double-click each dispatch once, Exit shows confirmation and No leaves the fixture running without shutdown callback. Fixture cleanup exit0; no Agent IPC.
- Initial automation attempts posted keyboard messages that did not select a popup item. The corrected harness used a physical click at the fixture menu-item rectangle after verifying window ownership; this was a harness correction, not another product patch.
- Focused independent review found no P1 issue.

M3 wrote the relay. Two M3 test drafts were rejected (wrong Windows API types, then a fake local WndProc shadowing production); Luna supplied actual Win32 tests and an isolated tray fixture. Parent integrated payload assertions, owner/loop changes and the native automation. Three M3 calls used2,950tokens; combined with the prior110-call ledger, recorded totals are113calls/147,220tokens. These are not all-session/account totals; GPT usage is unknown.

## Running-version boundary

The24-hour soak and its Agent were not restarted or replaced. They continue testing commit4962129 and Agent SHA551e8344dc15c7d7eb02348cc13d6a980a1fb9e2f060a437894e3971dca3f87e. The already-built unsigned installers also predate this repair. This commit verifies the repaired source and owned fixture only. Apply a newly built Agent/package after the soak, with explicit handling of live PTYs; do not claim that the user's current tray has already changed.

References: [Shell callback delivery](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar#receiving-notification-area-callback-messages), [TrackPopupMenu ownership and WM_NULL](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-trackpopupmenu).
