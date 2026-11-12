//! The tray icon: the current desktop's number (drawn live by `badge`, from an .ico until then),
//! its tooltip, and notifications that do something when clicked.

use crate::win::{pcw, wide};
use std::path::Path;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_NOSOUND, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetSystemMetrics, LoadIconW, LoadImageW, HICON, IDI_APPLICATION,
    IMAGE_ICON, LR_LOADFROMFILE, SM_CXSMICON, WM_APP,
};

/// Windows tells the main window about clicks on the icon with this message.
pub const WM_TRAY: u32 = WM_APP + 1;
pub const ID: u32 = 1;
/// lParam's low word of WM_TRAY when a notification was clicked / closed or gone without a click.
pub const BALLOON_CLICK: u32 = 0x405;
pub const BALLOON_GONE: u32 = 0x404;
/// The mouse rests on the icon (rich mode: instead of a tooltip, the app shows its own).
pub const POPUP_OPEN: u32 = 0x406;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    own_icon: bool,
    tip: String,
    /// Rich mode: no tooltip; Windows says when the mouse rests on the icon (POPUP_OPEN), and the app
    /// shows its mini-dock. Off: the plain tooltip (when the desktops can't be reached).
    pub rich: bool,
}

fn copy(dst: &mut [u16], s: &str) {
    let room = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(room).chain(Some(0))) {
        *d = c;
    }
}

/// An .ico file at the tray's icon size, or None.
fn load(path: &Path) -> Option<HICON> {
    let sz = unsafe { GetSystemMetrics(SM_CXSMICON) };
    let w = wide(&path.to_string_lossy());
    unsafe { LoadImageW(None, pcw(&w), IMAGE_ICON, sz, sz, LR_LOADFROMFILE) }.ok().map(|h| HICON(h.0))
}

impl Tray {
    pub fn new(hwnd: HWND) -> Self {
        Tray {
            hwnd,
            icon: unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default(),
            own_icon: false,
            tip: String::new(),
            rich: true,
        }
    }

    fn data(&self, flags: windows::Win32::UI::Shell::NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
        let flags = if self.rich || flags & NIF_TIP != NIF_TIP { flags } else { flags | NIF_SHOWTIP };
        let mut d = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ID,
            uFlags: flags,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        copy(&mut d.szTip, &self.tip);
        d
    }

    /// Puts it in the tray (again after Explorer restarts), speaking version 4: the callback's
    /// lParam has the event in its low word, wParam the icon's anchor point.
    pub fn add(&self) {
        let d = self.data(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &d).as_bool() {
                let _ = Shell_NotifyIconW(NIM_ADD, &d);
            }
            let mut v = self.data(NIF_MESSAGE);
            v.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &v);
        }
    }

    /// Back to the plain tooltip, or rich mode again.
    pub fn set_rich(&mut self, rich: bool) {
        if rich != self.rich {
            self.rich = rich;
            self.add();
        }
    }

    /// Where the icon is on screen (to keep the mini-dock open while the mouse is on it).
    pub fn rect(&self) -> Option<windows::Win32::Foundation::RECT> {
        let nid = NOTIFYICONIDENTIFIER { cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32, hWnd: self.hwnd, uID: ID, ..Default::default() };
        unsafe { Shell_NotifyIconGetRect(&nid) }.ok()
    }

    pub fn remove(&self) {
        let d = self.data(NIF_MESSAGE);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &d);
        }
    }

    fn show(&mut self, icon: HICON, own: bool) {
        if self.own_icon {
            unsafe {
                let _ = DestroyIcon(self.icon);
            }
        }
        self.icon = icon;
        self.own_icon = own;
        let d = self.data(NIF_ICON);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }

    /// The icon from an .ico file (kept as it is when the file can't be read).
    pub fn set_icon(&mut self, path: &Path) {
        if let Some(h) = load(path) {
            self.show(h, true);
        }
    }

    /// A drawn icon: the tray owns it, and destroys it when the next one comes.
    pub fn set_drawn(&mut self, h: HICON) {
        self.show(h, true);
    }

    pub fn set_tip(&mut self, tip: &str) {
        if tip == self.tip {
            return;
        }
        self.tip = tip.to_string();
        let d = self.data(NIF_TIP);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }

    /// A notification. Plain words: people read a toast in two seconds.
    pub fn notify(&self, title: &str, text: &str) {
        let mut d = self.data(NIF_INFO);
        copy(&mut d.szInfo, text);
        copy(&mut d.szInfoTitle, title);
        d.dwInfoFlags = NIIF_NOSOUND;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }
}
