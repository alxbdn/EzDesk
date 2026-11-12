//! The tray icon: the current desktop's number, its tooltip, notifications that do something when
//! clicked, and a short blue flash when the desktop changes.

use crate::glow;
use crate::win::{pcw, wide};
use std::collections::HashMap;
use std::path::Path;
use windows::Win32::Foundation::{HWND, TRUE};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_NOSOUND, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, DrawIconEx, GetSystemMetrics, LoadIconW, LoadImageW, DI_NORMAL, HICON, ICONINFO, IDI_APPLICATION,
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
    flashes: HashMap<String, Vec<HICON>>,
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
            flashes: HashMap::new(),
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

    /// One of the flash's frames (they're kept, not destroyed).
    pub fn set_frame(&mut self, h: HICON) {
        self.show(h, false);
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

    /// The flash for the number in this .ico: a blue rim, whitening at its peak, on and off again
    /// in about 0.6 s. Made once per file and kept.
    pub fn flash(&mut self, path: &Path) -> Vec<HICON> {
        let key = path.to_string_lossy().into_owned();
        if !self.flashes.contains_key(&key) {
            let count = 10;
            let frames = (1..=count).filter_map(|i| flash_frame(path, (std::f64::consts::PI * i as f64 / (count + 1) as f64).sin())).collect();
            self.flashes.insert(key.clone(), frames);
        }
        self.flashes[&key].clone()
    }
}

/// The number's icon with the glow's rim over it, k = 0…1 strong.
fn flash_frame(path: &Path, k: f64) -> Option<HICON> {
    let base = load(path)?;
    let sz = unsafe { GetSystemMetrics(SM_CXSMICON) } as usize;
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: sz as i32,
                biHeight: -(sz as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            let _ = DestroyIcon(base);
            return None;
        };
        let old = SelectObject(dc, bmp);
        let _ = DrawIconEx(dc, 0, 0, base, sz as i32, sz as i32, 0, None, DI_NORMAL); // premultiplied, alpha kept
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
        let _ = DestroyIcon(base);
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, sz * sz);

        // The rim: a rounded square, w wide, corners sz / 4, in the glow's blue (brightening a little at the peak, never white).
        let (szf, w) = (sz as f64, (sz as f64 / 9.0).max(1.5));
        let (r, half) = (szf / 4.0, (szf - w) / 2.0);
        let c = glow::color_at(k * 1.2, 6.0, 0.0);
        for y in 0..sz {
            for x in 0..sz {
                let qx = (x as f64 + 0.5 - szf / 2.0).abs() - (half - r);
                let qy = (y as f64 + 0.5 - szf / 2.0).abs() - (half - r);
                let d = glow::round_rect_distance(qx, qy, r, 2.0);
                let cov = (w / 2.0 + 0.5 - d.abs()).clamp(0.0, 1.0) * k;
                if cov > 0.0 {
                    px[y * sz + x] = glow::over(0xFF00_0000 | c, cov, px[y * sz + x]);
                }
            }
        }
        for p in px.iter_mut() {
            // icons want straight alpha
            let a = *p >> 24;
            if a > 0 && a < 255 {
                let un = |s: u32| ((((*p >> s) & 0xFF) * 255 + a / 2) / a).min(255) << s;
                *p = (a << 24) | un(16) | un(8) | un(0);
            }
        }
        let mask = CreateBitmap(sz as i32, sz as i32, 1, 1, None); // all-zero mask: the alpha decides
        let ii = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: bmp };
        let icon = CreateIconIndirect(&ii).ok();
        let _ = DeleteObject(mask);
        let _ = DeleteObject(bmp);
        icon
    }
}
