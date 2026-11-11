//! What EzDesk reads and changes in Windows besides desktops: its settings file, "Start with
//! Windows", the taskbar's "show apps from all desktops", the Store package, and whether its tray
//! icon is in view.

use crate::win::{pcw, wide};
use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Com::CLSIDFromString;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::WindowsProgramming::{GetPrivateProfileStringW, WritePrivateProfileStringW};
use windows::Win32::UI::Shell::{
    IsUserAnAdmin, SHGetKnownFolderPath, ShellExecuteW, Shell_NotifyIconGetRect, KF_FLAG_DEFAULT, NOTIFYICONIDENTIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowRect, SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, SW_SHOWNORMAL, WM_SETTINGCHANGE,
};

// ---- Settings: %APPDATA%\EzDesk\settings.ini, as earlier versions wrote it ----

pub fn app_data() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("EzDesk") // per-package private copy when packaged
}

pub struct Settings {
    path: Vec<u16>,
}

impl Settings {
    pub fn new() -> Self {
        Settings { path: wide(&app_data().join("settings.ini").to_string_lossy()) }
    }

    pub fn get(&self, key: &str, default: i32) -> i32 {
        let (sec, k, d) = (wide("EzDesk"), wide(key), wide(""));
        let mut buf = [0u16; 32];
        let n = unsafe { GetPrivateProfileStringW(pcw(&sec), pcw(&k), pcw(&d), Some(&mut buf), pcw(&self.path)) } as usize;
        String::from_utf16_lossy(&buf[..n]).trim().parse().unwrap_or(default)
    }

    pub fn set(&self, key: &str, value: i32) {
        let _ = std::fs::create_dir_all(app_data());
        let (sec, k, v) = (wide("EzDesk"), wide(key), wide(&value.to_string()));
        unsafe {
            let _ = WritePrivateProfileStringW(pcw(&sec), pcw(&k), pcw(&v), pcw(&self.path));
        }
    }
}

/// The very first launch (welcomed before, under either name: false). Remembers it.
pub fn first_run() -> bool {
    let flag = app_data().join("welcome-shown");
    let old = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("DesktopSwitcher").join("welcome-shown");
    if flag.exists() || old.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(app_data());
    let _ = std::fs::write(flag, "");
    true
}

// ---- Registry ----

fn reg_dword(root: HKEY, sub: &str, name: &str) -> Option<u32> {
    let (s, n) = (wide(sub), wide(name));
    let mut v = 0u32;
    let mut size = 4u32;
    let r = unsafe { RegGetValueW(root, pcw(&s), pcw(&n), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size)) };
    r.is_ok().then_some(v)
}

fn reg_string(root: HKEY, sub: &str, name: &str) -> Option<String> {
    let (s, n) = (wide(sub), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(root, pcw(&s), pcw(&n), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
    if r.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn reg_set_dword(sub: &str, name: &str, v: u32) {
    let (s, n) = (wide(sub), wide(name));
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, pcw(&s), pcw(&n), REG_DWORD.0, Some(&v as *const u32 as *const _), 4);
    }
}

fn reg_set_string(sub: &str, name: &str, v: &str) {
    let (s, n, d) = (wide(sub), wide(name), wide(v));
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, pcw(&s), pcw(&n), REG_SZ.0, Some(d.as_ptr() as *const _), (d.len() * 2) as u32);
    }
}

fn reg_delete(sub: &str, name: &str) {
    let (s, n) = (wide(sub), wide(name));
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, pcw(&s), pcw(&n));
    }
}

/// This Windows' build number (22000+ is Windows 11).
pub fn windows_build() -> u32 {
    reg_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

// ---- How it runs ----

/// Running from an MSIX package (the Store app).
pub fn is_packaged() -> bool {
    let mut len = 0u32;
    unsafe { GetCurrentPackageFullName(&mut len, PWSTR::null()).0 != 15700 } // APPMODEL_ERROR_NO_PACKAGE
}

/// Running as admin (admin mode's scheduled task).
pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

/// Opens a web page or a Settings page.
pub fn open(target: &str) {
    let (op, t) = (wide("open"), wide(target));
    unsafe {
        ShellExecuteW(None, pcw(&op), pcw(&t), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

// ---- "Start with Windows" (simple mode): HKCU\...\Run ----
// Admin mode starts from a scheduled task; the Store app from a startup task in its package.

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

pub fn startup_on() -> bool {
    reg_string(HKEY_CURRENT_USER, RUN, "EzDesk").is_some()
}

/// On: (re)written, so it follows the folder if it moves.
pub fn set_startup(on: bool) {
    if on {
        let exe = std::env::current_exe().unwrap_or_default();
        reg_set_string(RUN, "EzDesk", &format!("\"{}\" --startup", exe.display()));
    } else {
        reg_delete(RUN, "EzDesk");
    }
}

/// Earlier versions started with a shortcut in the Startup folder (EzDesk.lnk, or Desktop
/// Switcher.lnk before the rename). Removed; true if there was one, so it's turned on the new way.
pub fn remove_startup_shortcuts() -> bool {
    let dir = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
    let mut found = false;
    for name in ["EzDesk.lnk", "Desktop Switcher.lnk"] {
        if std::fs::remove_file(dir.join(name)).is_ok() {
            found = true;
        }
    }
    found
}

// ---- Taskbar: show apps from all desktops (Windows' own setting) ----
// VirtualDesktopTaskbarFilter = 0 is "On all desktops". A packaged app's registry writes go to a
// private copy, so the Store app opens the Settings page instead.

const EXPLORER_ADV: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

pub fn taskbar_all_desktops() -> bool {
    reg_dword(HKEY_CURRENT_USER, EXPLORER_ADV, "VirtualDesktopTaskbarFilter").unwrap_or(1) == 0
}

pub fn set_taskbar_all_desktops(on: bool) {
    reg_set_dword(EXPLORER_ADV, "VirtualDesktopTaskbarFilter", if on { 0 } else { 1 });
    let what = wide("TraySettings");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(what.as_ptr() as isize), SMTO_ABORTIFHUNG, 1000, None); // the taskbar re-reads it
    }
}

// ---- The tray icon: shown on the taskbar, or hidden under ^ ----

/// True when (x, y) is inside the taskbar's notification area: tray icons, network/volume, clock.
pub fn over_tray_area(taskbar: HWND, x: i32, y: i32) -> bool {
    let class = wide("TrayNotifyWnd");
    let Ok(tray) = (unsafe { FindWindowExW(taskbar, None, pcw(&class), PCWSTR::null()) }) else { return false };
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(tray, &mut r);
    }
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

/// Some(true): our icon is on the taskbar; Some(false): hidden under ^; None: can't tell.
pub fn tray_icon_shown(hwnd: HWND, id: u32, build: u32) -> Option<bool> {
    if let Some(p) = tray_promoted() {
        return Some(p);
    }
    if build >= 22000 {
        return None; // Windows 11 reports a hidden icon at the ^ button, inside the tray too
    }
    // Windows 10: a hidden icon sits in the (closed) overflow window, outside the taskbar's tray area.
    let nid = NOTIFYICONIDENTIFIER { cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32, hWnd: hwnd, uID: id, ..Default::default() };
    let r = unsafe { Shell_NotifyIconGetRect(&nid) }.ok()?;
    let class = wide("Shell_TrayWnd");
    let taskbar = unsafe { FindWindowW(pcw(&class), PCWSTR::null()) }.ok()?;
    let p = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
    Some(over_tray_area(taskbar, p.x, p.y))
}

/// Windows 11 (22H2+) keeps one key per tray icon under NotifyIconSettings, found by exe path.
/// IsPromoted = 1 means shown on the taskbar. Paths under known folders are stored as {GUID}\rest.
fn tray_promoted() -> Option<bool> {
    let me = std::env::current_exe().ok()?.to_string_lossy().to_lowercase();
    let base = r"Control Panel\NotifyIconSettings";
    let b = wide(base);
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&b), 0, KEY_READ, &mut key) }.is_err() {
        return None;
    }
    let mut found = None;
    for i in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        if unsafe { RegEnumKeyExW(key, i, PWSTR(name.as_mut_ptr()), &mut len, None, PWSTR::null(), None, None) }.is_err() {
            break;
        }
        let sub = format!(r"{base}\{}", String::from_utf16_lossy(&name[..len as usize]));
        let Some(path) = reg_string(HKEY_CURRENT_USER, &sub, "ExecutablePath") else { continue };
        if expand_known_folder(&path).to_lowercase() == me {
            found = Some(reg_dword(HKEY_CURRENT_USER, &sub, "IsPromoted").unwrap_or(0) == 1);
            break;
        }
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

fn expand_known_folder(path: &str) -> String {
    if !path.starts_with('{') || path.len() < 38 {
        return path.to_string();
    }
    let (guid, rest) = path.split_at(38);
    let g = wide(guid);
    let Ok(id) = (unsafe { CLSIDFromString(pcw(&g)) }) else { return path.to_string() };
    let Ok(p) = (unsafe { SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) }) else { return path.to_string() };
    let folder = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _)) };
    folder + rest
}

/// The Settings page where a hidden tray icon is turned on.
pub fn open_tray_icon_settings(build: u32) {
    if build >= 22000 {
        open("ms-settings:taskbar"); // Windows 11: Taskbar > Other system tray icons
    } else {
        open("shell:::{05d7b0f4-2121-4eff-bf6b-ed3f69b894d9}"); // Windows 10: Select which icons appear on the taskbar
    }
}
