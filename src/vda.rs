//! The Desktops module's adapter for Windows before 11 24H2 (and the fallback after it): the
//! VirtualDesktopAccessor DLL (its Windows 11 or Windows 10 build, next to EzDesk.exe) and
//! Windows' own Ctrl+Win shortcuts, which slide where the DLL's switch is instant. Windows 11 24H2
//! and later use `native.rs` instead, compiled in.
//!
//! A DLL that doesn't match this Windows build crashes (access violation) instead of returning an
//! error, and a Rust program can't catch that. So before using it, EzDesk tries it in a child
//! process (`EzDesk.exe --probe`): if that one crashes, the DLL is left alone and the app says so.

use crate::desktops::Adapter;
use crate::hold::Hwnd;
use crate::win;
use std::cell::Cell;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use windows::core::PCSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_D, VK_LCONTROL, VK_LEFT, VK_LWIN, VK_RIGHT};

type FnInt = unsafe extern "system" fn() -> i32;
type FnHwnd = unsafe extern "system" fn(isize) -> i32;
type FnMove = unsafe extern "system" fn(isize, i32) -> i32;
type FnHook = unsafe extern "system" fn(isize, u32) -> i32;
type FnName = unsafe extern "system" fn(i32, *mut u8, usize) -> i32;
type FnGo = unsafe extern "system" fn(i32);
type FnRemove = unsafe extern "system" fn(i32, i32) -> i32;
type FnSetName = unsafe extern "system" fn(i32, *const u8) -> i32;

/// Optional calls: not in every build of the DLL (Windows 10's has none of these but the first).
struct More {
    window_desktop: Option<FnHwnd>,
    create: Option<FnInt>,
    remove: Option<FnRemove>,
    set_name: Option<FnSetName>,
}

pub struct VdaAdapter {
    more: More,
    count: Option<FnInt>,
    current: Option<FnInt>,
    move_window: Option<FnMove>,
    hook: Option<FnHook>,
    go_to: Option<FnGo>,
    name: Option<FnName>,
    pin: Option<FnHwnd>,
    unpin: Option<FnHwnd>,
    broken: Cell<bool>,
}

/// The DLL for this Windows: Windows 11 (build 22000+) or Windows 10.
pub fn dll_name(build: u32) -> &'static str {
    if build >= 22000 { "VirtualDesktopAccessor.dll" } else { "VirtualDesktopAccessor-win10.dll" }
}

impl VdaAdapter {
    /// Loads the DLL; Err with what the user should be told if it can't be used at all.
    pub fn load(build: u32) -> Result<Self, String> {
        let path = win::exe_dir().join(dll_name(build));
        let w = win::wide(&path.to_string_lossy());
        let h = unsafe { LoadLibraryW(win::pcw(&w)) }.map_err(|_| format!("Could not load {}", path.display()))?;
        let f = |name: &str| proc(h, name);
        let a = unsafe {
            VdaAdapter {
                more: More {
                    window_desktop: f("GetWindowDesktopNumber").map(|p| std::mem::transmute::<_, FnHwnd>(p)),
                    create: f("CreateDesktop").map(|p| std::mem::transmute::<_, FnInt>(p)),
                    remove: f("RemoveDesktop").map(|p| std::mem::transmute::<_, FnRemove>(p)),
                    set_name: f("SetDesktopName").map(|p| std::mem::transmute::<_, FnSetName>(p)),
                },
                count: f("GetDesktopCount").map(|p| std::mem::transmute::<_, FnInt>(p)),
                current: f("GetCurrentDesktopNumber").map(|p| std::mem::transmute::<_, FnInt>(p)),
                move_window: f("MoveWindowToDesktopNumber").map(|p| std::mem::transmute::<_, FnMove>(p)),
                hook: f("RegisterPostMessageHook").map(|p| std::mem::transmute::<_, FnHook>(p)),
                go_to: f("GoToDesktopNumber").map(|p| std::mem::transmute::<_, FnGo>(p)),
                name: f("GetDesktopName").map(|p| std::mem::transmute::<_, FnName>(p)), // Windows 11 build only
                pin: f("PinWindow").map(|p| std::mem::transmute::<_, FnHwnd>(p)),
                unpin: f("UnPinWindow").map(|p| std::mem::transmute::<_, FnHwnd>(p)),
                broken: Cell::new(false),
            }
        };
        if a.count.is_none() || a.current.is_none() || a.move_window.is_none() || a.hook.is_none() {
            return Err(format!("{} doesn't look like VirtualDesktopAccessor.\nIt may not match this Windows build ({build}).", dll_name(build)));
        }
        Ok(a)
    }

    /// One DLL with nothing in it: for when the probe crashed. Everything is a no-op.
    pub fn broken() -> Self {
        VdaAdapter { more: More { window_desktop: None, create: None, remove: None, set_name: None }, count: None, current: None, move_window: None, hook: None, go_to: None, name: None, pin: None, unpin: None, broken: Cell::new(true) }
    }
}

fn proc(h: HMODULE, name: &str) -> Option<unsafe extern "system" fn() -> isize> {
    let n: Vec<u8> = name.bytes().chain(Some(0)).collect();
    unsafe { GetProcAddress(h, PCSTR(n.as_ptr())) }
}

/// Runs in the child process: a few calls into the DLL. Exit code 0 = it answered (even "not
/// ready yet", -1), anything else = it crashed. what: "dll", or "native" (winvd: 0 only when it
/// sees desktops, so a build it doesn't know falls back to the DLL).
pub fn probe(build: u32, what: &str) -> i32 {
    if what == "native" {
        return if crate::native::NativeAdapter::new().count() > 0 { 0 } else { 3 };
    }
    match VdaAdapter::load(build) {
        Ok(a) => {
            let _ = (a.count(), a.current());
            0
        }
        Err(_) => 2,
    }
}

/// Asks a child process to try "dll" or "native". true = safe to use here.
pub fn probe_ok(what: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return true };
    let Ok(mut child) = Command::new(exe).args(["--probe", what]).creation_flags(0x0800_0000).spawn() else { return true }; // CREATE_NO_WINDOW
    for _ in 0..100 {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code() == Some(0);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill(); // hung for 5 s: Windows isn't ready, not a crash; the DLL is tried anyway
    what == "dll"
}

impl Adapter for VdaAdapter {
    fn broken(&self) -> bool {
        self.broken.get()
    }
    fn can_pin(&self) -> bool {
        self.pin.is_some() && self.unpin.is_some()
    }
    fn count(&self) -> i32 {
        self.count.map_or(-1, |f| unsafe { f() })
    }
    fn current(&self) -> i32 {
        self.current.map_or(-1, |f| unsafe { f() })
    }
    fn name(&self, i: i32) -> Option<String> {
        let f = self.name?;
        let mut buf = [0u8; 1024];
        if unsafe { f(i, buf.as_mut_ptr(), buf.len()) } == -1 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }
    fn move_window(&self, hwnd: Hwnd, i: i32) {
        if let Some(f) = self.move_window {
            unsafe { f(hwnd, i) };
        }
    }
    fn pin(&self, hwnd: Hwnd) -> bool {
        self.pin.is_some_and(|f| unsafe { f(hwnd) } != -1)
    }
    fn unpin(&self, hwnd: Hwnd) {
        if let Some(f) = self.unpin {
            unsafe { f(hwnd) };
        }
    }
    fn slide_left(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_LEFT]);
    }
    fn slide_right(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_RIGHT]);
    }
    fn slide_new(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_D]); // makes one at the end and slides there, on Windows 10 too
    }
    fn go_to(&self, i: i32) {
        if let Some(f) = self.go_to {
            unsafe { f(i) };
        }
    }
    fn window_desktop(&self, hwnd: Hwnd) -> i32 {
        self.more.window_desktop.map_or(-1, |f| unsafe { f(hwnd) })
    }
    fn can_manage(&self) -> bool {
        self.more.create.is_some() && self.more.remove.is_some() && self.more.set_name.is_some()
    }
    fn create(&self) {
        if let Some(f) = self.more.create {
            unsafe { f() };
        }
    }
    fn remove(&self, i: i32, fallback: i32) {
        if let Some(f) = self.more.remove {
            unsafe { f(i, fallback) };
        }
    }
    fn set_name(&self, i: i32, name: &str) {
        if let Some(f) = self.more.set_name {
            let utf8: Vec<u8> = name.bytes().chain(Some(0)).collect();
            unsafe { f(i, utf8.as_ptr()) };
        }
    }
    fn wallpaper(&self, _: i32) -> Option<String> {
        None // as Windows keeps it: Desktops looks in the registry
    }
    fn can_reorder(&self) -> bool {
        false // the DLL has no call for it
    }
    fn move_desktop(&self, _: i32, _: i32) {}
    fn listen(&self, hwnd: Hwnd, msg: u32) {
        if let Some(f) = self.hook {
            unsafe { f(hwnd, msg) };
        }
    }
}
