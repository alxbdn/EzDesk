//! A layered window (per-pixel alpha) that clicks go through, never activated, not in the
//! taskbar, always on top: what the rails and the Settings guide are drawn into (with Direct2D).

use crate::win;
use std::sync::Once;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowPos, ShowWindow, UpdateLayeredWindow, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const CLASS: &str = "EzDeskGlow";

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(h, msg, wp, lp)
}

pub struct Layered {
    pub hwnd: HWND,
    dc: HDC,
    bits: *mut u32,
    pub w: usize,
    pub h: usize,
    pub visible: bool,
    pub raised_at: u64,
}

impl Layered {
    pub fn new(w: usize, h: usize) -> Self {
        static REGISTER: Once = Once::new();
        let class = win::wide(CLASS);
        unsafe {
            let inst = GetModuleHandleW(None).unwrap_or_default();
            REGISTER.call_once(|| {
                let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: win::pcw(&class), ..Default::default() };
                RegisterClassW(&wc);
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                win::pcw(&class),
                None,
                WS_POPUP,
                0,
                0,
                w as i32,
                h as i32,
                None,
                None,
                inst,
                None,
            )
            .unwrap_or_default();
            let dc = CreateCompatibleDC(None);
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
            SelectObject(dc, bmp);
            Layered { hwnd, dc, bits: bits as *mut u32, w, h, visible: false, raised_at: 0 }
        }
    }

    /// Shows its pixels at (x, y), × alpha.
    pub fn put(&self, x: i32, y: i32, alpha: u8) {
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: AC_SRC_ALPHA as u8 };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&POINT { x, y }),
                Some(&SIZE { cx: self.w as i32, cy: self.h as i32 }),
                self.dc,
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    /// On top of everything (Snap's preview included), without activating it.
    pub fn raise(&mut self, now: u64) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        self.visible = true;
        self.raised_at = now;
    }

    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
    }

    /// A copy of its pixels (premultiplied 0xAARRGGBB, row by row), to look at them in tests.
    #[cfg(test)]
    pub fn snapshot(&self) -> Vec<u32> {
        if self.bits.is_null() {
            return vec![];
        }
        unsafe { std::slice::from_raw_parts(self.bits, self.w * self.h) }.to_vec()
    }

    /// Its memory DC (premultiplied 32-bit), to draw into with Direct2D.
    pub fn dc(&self) -> HDC {
        self.dc
    }

    /// Its window and its pixels, freed (for one made for a moment).
    pub fn destroy(&mut self) {
        use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, GetCurrentObject, OBJ_BITMAP};
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
            let bmp = GetCurrentObject(self.dc, OBJ_BITMAP);
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(bmp);
        }
        self.bits = std::ptr::null_mut();
        self.visible = false;
    }
}
