//! The Desktops module's adapter for Windows 11 24H2 and later (build 26100+): winvd, the Rust
//! library VirtualDesktopAccessor is built from, compiled into EzDesk.exe. No DLL.
//! Older Windows (Windows 10, Windows 11 before 24H2) keep the DLL adapter (`vda.rs`).

use crate::desktops::Adapter;
use crate::hold::Hwnd;
use crate::win;
use std::cell::RefCell;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_D, VK_LCONTROL, VK_LEFT, VK_LWIN, VK_RIGHT};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// The first build winvd supports.
pub const MIN_BUILD: u32 = 26100;

pub struct NativeAdapter {
    /// Kept alive: dropping it stops the listener.
    listener: RefCell<Option<winvd::DesktopEventThread>>,
}

impl NativeAdapter {
    pub fn new() -> Self {
        NativeAdapter { listener: RefCell::new(None) }
    }
}

fn h(hwnd: Hwnd) -> HWND {
    HWND(hwnd as *mut _)
}

impl Adapter for NativeAdapter {
    fn broken(&self) -> bool {
        false
    }
    fn can_pin(&self) -> bool {
        true
    }
    fn count(&self) -> i32 {
        winvd::get_desktop_count().map_or(-1, |c| c as i32)
    }
    fn current(&self) -> i32 {
        winvd::get_current_desktop().and_then(|d| d.get_index()).map_or(-1, |i| i as i32)
    }
    fn name(&self, i: i32) -> Option<String> {
        winvd::get_desktop(i as u32).get_name().ok()
    }
    fn move_window(&self, hwnd: Hwnd, i: i32) {
        let _ = winvd::move_window_to_desktop(i as u32, &h(hwnd));
    }
    fn pin(&self, hwnd: Hwnd) -> bool {
        winvd::pin_window(h(hwnd)).is_ok()
    }
    fn unpin(&self, hwnd: Hwnd) {
        let _ = winvd::unpin_window(h(hwnd));
    }
    fn slide_left(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_LEFT]);
    }
    fn slide_right(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_RIGHT]);
    }
    fn slide_new(&self) {
        win::press(&[VK_LCONTROL, VK_LWIN, VK_D]);
    }
    fn go_to(&self, i: i32) {
        let _ = winvd::switch_desktop(i as u32);
    }
    fn window_desktop(&self, hwnd: Hwnd) -> i32 {
        winvd::get_desktop_by_window(h(hwnd)).and_then(|d| d.get_index()).map_or(-1, |i| i as i32)
    }
    fn can_manage(&self) -> bool {
        true
    }
    fn create(&self) {
        let _ = winvd::create_desktop();
    }
    fn remove(&self, i: i32, fallback: i32) {
        let _ = winvd::remove_desktop(i as u32, fallback as u32);
    }
    fn set_name(&self, i: i32, name: &str) {
        let _ = winvd::get_desktop(i as u32).set_name(name);
    }
    fn wallpaper(&self, i: i32) -> Option<String> {
        winvd::get_desktop(i as u32).get_wallpaper().ok()
    }
    fn can_reorder(&self) -> bool {
        true
    }
    fn move_desktop(&self, i: i32, to: i32) {
        let _ = winvd::move_desktop(i as u32, to as u32);
    }
    /// winvd reports switches on a channel: a thread turns each one into msg posted to hwnd.
    fn listen(&self, hwnd: Hwnd, msg: u32) {
        let (tx, rx) = std::sync::mpsc::channel::<winvd::DesktopEvent>();
        match winvd::listen_desktop_events(tx) {
            Ok(thread) => *self.listener.borrow_mut() = Some(thread),
            Err(e) => {
                win::log(&format!("can't listen to desktop switches: {e:?}"));
                return; // the 1 s poll still notices them
            }
        }
        std::thread::spawn(move || {
            for ev in rx {
                if matches!(ev, winvd::DesktopEvent::DesktopChanged { .. }) {
                    unsafe {
                        let _ = PostMessageW(h(hwnd), msg, WPARAM(0), LPARAM(0));
                    }
                }
            }
        });
    }
}
