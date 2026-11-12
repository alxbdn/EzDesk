//! Small helpers over the Win32 API, shared by the platform modules.

use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetCursorPos, GetWindowTextLengthW, GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE};

/// A NUL-terminated UTF-16 copy of s, for the W functions.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Keeps the buffer alive as long as the PCWSTR is used: `let w = wide(..); pcw(&w)`.
pub fn pcw(w: &[u16]) -> PCWSTR {
    PCWSTR(w.as_ptr())
}

pub fn hwnd(h: isize) -> HWND {
    HWND(h as *mut _)
}

pub fn id(h: HWND) -> isize {
    h.0 as isize
}

pub fn now() -> u64 {
    unsafe { GetTickCount64() }
}

/// The system's scale: 1.0 at 96 DPI (100 %).
pub fn scale() -> f64 {
    unsafe { GetDpiForSystem() as f64 / 96.0 }
}

pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// The work area (the screen less the taskbar) of the screen at (x, y).
pub fn work_area_at(x: i32, y: i32) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

pub fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// A window someone would hold: visible, not minimized, not a tool window, not the taskbar or the
/// desktop, with a title.
pub fn is_real_window(h: HWND) -> bool {
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        if style & 0x1000_0000 == 0 || style & 0x2000_0000 != 0 || ex & 0x80 != 0 {
            return false; // hidden, minimized, tool window
        }
        if matches!(class_name(h).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW") {
            return false;
        }
        GetWindowTextLengthW(h) > 0
    }
}

/// Every top-level window, top-most first.
pub fn top_level_windows() -> Vec<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowExW;
    let mut out = vec![];
    let mut h = HWND::default();
    while let Ok(next) = unsafe { FindWindowExW(None, h, PCWSTR::null(), PCWSTR::null()) } {
        if next.is_invalid() {
            break;
        }
        out.push(next);
        h = next;
    }
    out
}

/// Hidden by its own app (a Store app in the background), not just on another desktop.
pub fn cloaked_by_app(h: HWND) -> bool {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut c = 0u32;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut c as *mut u32 as *mut _, 4) }.is_ok() && c & 1 != 0
}

/// The whole screen at (x, y), taskbar included.
pub fn monitor_at(x: i32, y: i32) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcMonitor
    }
}

/// What went wrong, in %TEMP%\EzDesk-debug.log (kept short).
pub fn log(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("EzDesk-debug.log");
    if std::fs::metadata(&path).map(|m| m.len() > 20_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{secs}  {msg}");
    }
}

/// The folder EzDesk.exe is in.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default()
}

/// Presses keys together, then lets them go in reverse order: `press(&[VK_LCONTROL, VK_LWIN, VK_RIGHT])`.
pub fn press(keys: &[VIRTUAL_KEY]) {
    let key = |vk: VIRTUAL_KEY, up: bool| {
        let extended = matches!(vk.0, 0x25..=0x28 | 0x5B | 0x5C); // arrows, Windows keys
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: flags, ..Default::default() } } }
    };
    let inputs: Vec<INPUT> = keys.iter().map(|&k| key(k, false)).chain(keys.iter().rev().map(|&k| key(k, true))).collect();
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
