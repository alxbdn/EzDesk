//! The mouse, system-wide: a low-level hook on its own thread (so the mouse never waits for the
//! app), and Windows' announcements of window drags. Both only post a message to the main window,
//! which does the work.
//!
//! Scrolling on the taskbar is taken (the taskbar never sees it) and becomes a desktop switch;
//! scrolling over the tray (volume, clock…) is left to Windows. A left-button press is only
//! reported: it always goes through untouched.

use crate::system::over_tray_area;
use crate::win;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetAncestor, GetMessageW, PostMessageW, SetWindowsHookExW, TranslateMessage, WindowFromPoint,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GA_ROOT, HC_ACTION, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_LBUTTONDOWN, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
};

/// wParam: +1 = next desktop (wheel down), -1 = previous.
pub const WM_WHEEL: u32 = WM_APP + 2;
/// A button pressed at (wParam, lParam).
pub const WM_PRESS: u32 = WM_APP + 3;
/// A window drag started (wParam 1) or ended (0); lParam: the window.
pub const WM_MOVESIZE: u32 = WM_APP + 4;

static MAIN: AtomicIsize = AtomicIsize::new(0);
static SWAPPED: AtomicIsize = AtomicIsize::new(0);

fn post(msg: u32, w: usize, l: isize) {
    unsafe {
        let _ = PostMessageW(win::hwnd(MAIN.load(Ordering::Relaxed)), msg, WPARAM(w), LPARAM(l));
    }
}

/// Starts both. swapped: the mouse's buttons are swapped (its primary button is the right one).
pub fn start(main: HWND, swapped: bool) {
    MAIN.store(win::id(main), Ordering::Relaxed);
    SWAPPED.store(swapped as isize, Ordering::Relaxed);
    unsafe {
        SetWinEventHook(EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND, None, Some(on_move_size), 0, 0, WINEVENT_OUTOFCONTEXT);
    }
    std::thread::spawn(|| unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        if SetWindowsHookExW(WH_MOUSE_LL, Some(on_mouse), inst, 0).is_err() {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn on_move_size(_: HWINEVENTHOOK, event: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
    if id_object == 0 {
        // OBJID_WINDOW only
        post(WM_MOVESIZE, (event == EVENT_SYSTEM_MOVESIZESTART) as usize, win::id(hwnd));
    }
}

unsafe extern "system" fn on_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let primary = if SWAPPED.load(Ordering::Relaxed) != 0 { WM_RBUTTONDOWN } else { WM_LBUTTONDOWN };
        match wparam.0 as u32 {
            WM_MOUSEWHEEL => {
                let root = GetAncestor(WindowFromPoint(info.pt), GA_ROOT);
                if matches!(win::class_name(root).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
                    && !over_tray_area(root, info.pt.x, info.pt.y)
                {
                    let delta = (info.mouseData >> 16) as i16;
                    post(WM_WHEEL, if delta < 0 { 1 } else { usize::MAX }, 0); // usize::MAX: -1
                    return LRESULT(1); // taken: the taskbar doesn't scroll
                }
            }
            m if m == primary => post(WM_PRESS, info.pt.x as usize, info.pt.y as isize),
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
