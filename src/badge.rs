//! The tray number, drawn live: a dark tile filling the whole slot, the number in pure white
//! filling 80 % of its height. When the desktop changes the number rolls: up towards the next
//! desktop, down towards the previous one, under a soft flash of EzDesk's light on the tile's rim.
//!
//! Drawn with Direct2D into a small bitmap, then handed to the tray as an icon: once when the
//! number changes, then 33 times a second while it rolls. The digits are the font's own
//! outlines, centred on their ink rather than on the font's line box.

use crate::{glow, ui};
use std::collections::HashMap;
use windows::core::{w, Interface};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{FALSE, RECT, TRUE};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON};

/// How long the number takes to roll, and the rim's flash.
pub const ROLL_MS: u64 = 380;
const FLASH_MS: u64 = 600;
/// Time between frames while the number rolls.
pub const BUSY_MS: u32 = 30;

const TILE: u32 = 0x09090B; // ui::BG
const INK: u32 = 0xFFFFFF; // pure white
const HEIGHT: f32 = 0.80; // of the tile, for the number's ink
const WIDTH: f32 = 0.88; // at most (two digits)

/// Where the roll is, 0 → 1, at `k` = elapsed / ROLL_MS (0 → 1). It may overshoot 1 a little:
/// the number then settles back into place.
fn roll_ease(k: f64) -> f64 {
    // TODO(human)
    k
}

struct Roll {
    from: i32,
    dir: i32,
    at: u64,
}

pub struct Badge {
    size: u32,
    d2d: ID2D1Factory,
    face: IDWriteFontFace,
    rt: ID2D1DCRenderTarget,
    dc: HDC,
    dib: HBITMAP,
    bits: *mut u32,
    shapes: HashMap<i32, (ID2D1PathGeometry, D2D_RECT_F)>,
    num: i32,
    roll: Option<Roll>,
}

/// What a number reads as on the tile: 1 to 99, else "+".
fn label(n: i32) -> String {
    if (1..=99).contains(&n) {
        n.to_string()
    } else {
        "+".into()
    }
}

impl Badge {
    pub fn new() -> Option<Self> {
        Self::sized(unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32)
    }

    fn sized(size: u32) -> Option<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let mut fonts = None;
            dw.GetSystemFontCollection(&mut fonts, FALSE).ok()?;
            let fonts = fonts?;
            let (mut index, mut found) = (0u32, FALSE);
            for name in [w!("Segoe UI Variable Display"), w!("Segoe UI")] {
                if fonts.FindFamilyName(name, &mut index, &mut found).is_ok() && found.as_bool() {
                    break;
                }
            }
            if !found.as_bool() {
                return None;
            }
            let font = fonts.GetFontFamily(index).ok()?.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL).ok()?;
            let face = font.CreateFontFace().ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let rt = d2d.CreateDCRenderTarget(&props).ok()?;
            let dc = CreateCompatibleDC(None);
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    biHeight: -(size as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            SelectObject(dc, dib);
            Some(Badge { size, d2d, face, rt, dc, dib, bits: bits as *mut u32, shapes: HashMap::new(), num: 0, roll: None })
        }
    }

    /// The number to show. A change from one number to another rolls (up when it grows).
    pub fn set(&mut self, num: i32, now: u64) {
        if num == self.num {
            return;
        }
        if self.num > 0 {
            self.roll = Some(Roll { from: self.num, dir: if num > self.num { 1 } else { -1 }, at: now });
        }
        self.num = num;
    }

    /// Rolling or flashing: frames come quicker.
    pub fn busy(&self, now: u64) -> bool {
        self.roll.as_ref().is_some_and(|r| now.saturating_sub(r.at) < FLASH_MS.max(ROLL_MS))
    }

    /// The number's outline, at 100 DIPs, and the box of its ink. Made once per number.
    fn shape(&mut self, n: i32) -> Option<(ID2D1PathGeometry, D2D_RECT_F)> {
        if let Some(s) = self.shapes.get(&n) {
            return Some(s.clone());
        }
        let cps: Vec<u32> = label(n).chars().map(|c| c as u32).collect();
        let mut glyphs = vec![0u16; cps.len()];
        unsafe {
            self.face.GetGlyphIndices(cps.as_ptr(), cps.len() as u32, glyphs.as_mut_ptr()).ok()?;
            let mut fm = DWRITE_FONT_METRICS::default();
            self.face.GetMetrics(&mut fm);
            let mut gm = vec![DWRITE_GLYPH_METRICS::default(); glyphs.len()];
            self.face.GetDesignGlyphMetrics(glyphs.as_ptr(), glyphs.len() as u32, gm.as_mut_ptr(), FALSE).ok()?;
            let em = 100.0f32;
            let advances: Vec<f32> = gm.iter().map(|g| g.advanceWidth as f32 * em / fm.designUnitsPerEm.max(1) as f32).collect();
            let path = self.d2d.CreatePathGeometry().ok()?;
            let sink = path.Open().ok()?;
            self.face
                .GetGlyphRunOutline(em, glyphs.as_ptr(), Some(advances.as_ptr()), None, glyphs.len() as u32, FALSE, FALSE, &sink)
                .ok()?;
            sink.Close().ok()?;
            let ink = path.GetBounds(None).ok()?;
            self.shapes.insert(n, (path.clone(), ink));
            Some((path, ink))
        }
    }

    /// The number, centred on the tile, `dy` lower.
    fn draw_number(&mut self, n: i32, dy: f32, brush: &ID2D1Brush) {
        let Some((path, ink)) = self.shape(n) else { return };
        let s = self.size as f32;
        let (iw, ih) = ((ink.right - ink.left).max(0.1), (ink.bottom - ink.top).max(0.1));
        let k = (HEIGHT * s / ih).min(WIDTH * s / iw);
        let (cx, cy) = ((ink.left + ink.right) / 2.0, (ink.top + ink.bottom) / 2.0);
        let m = Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: s / 2.0 - k * cx, M32: s / 2.0 + dy - k * cy };
        unsafe {
            if let Ok(g) = self.d2d.CreateTransformedGeometry(&path, &m) {
                self.rt.FillGeometry(&g, brush, None);
            }
        }
    }

    /// This moment's icon (the caller destroys it once the tray has it).
    pub fn frame(&mut self, now: u64) -> Option<HICON> {
        if self.num == 0 {
            return None;
        }
        if self.roll.as_ref().is_some_and(|r| now.saturating_sub(r.at) >= FLASH_MS.max(ROLL_MS)) {
            self.roll = None;
        }
        let (size, s) = (self.size, self.size as f32);
        let tile = ui::R { x: 0.0, y: 0.0, w: s, h: s };
        unsafe {
            self.rt.BindDC(self.dc, &RECT { left: 0, top: 0, right: size as i32, bottom: size as i32 }).ok()?;
            self.rt.BeginDraw();
            self.rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            if let Ok(bg) = self.rt.CreateSolidColorBrush(&ui::color(TILE), None) {
                self.rt.FillRoundedRectangle(&ui::rounded(&tile, s * 0.24), &bg);
            }
            if let Some(ink) = self.rt.CreateSolidColorBrush(&ui::color(INK), None).ok().and_then(|b| b.cast::<ID2D1Brush>().ok()) {
                match self.roll.as_ref().map(|r| (r.from, r.dir, now.saturating_sub(r.at))) {
                    Some((from, dir, t)) if t < ROLL_MS => {
                        let k = roll_ease(t as f64 / ROLL_MS as f64) as f32;
                        let d = dir as f32;
                        self.rt.PushAxisAlignedClip(&tile.d2d(), D2D1_ANTIALIAS_MODE_ALIASED);
                        self.draw_number(from, -d * s * k, &ink); // leaving
                        self.draw_number(self.num, d * s * (1.0 - k), &ink); // arriving
                        self.rt.PopAxisAlignedClip();
                    }
                    _ => self.draw_number(self.num, 0.0, &ink),
                }
            }
            // a soft flash of the rim while it rolls
            if let Some(t) = self.roll.as_ref().map(|r| now.saturating_sub(r.at)).filter(|&t| t < FLASH_MS) {
                let k = 0.6 * (std::f64::consts::PI * t as f64 / FLASH_MS as f64).sin();
                if let Ok(rim) = self.rt.CreateSolidColorBrush(&ui::color(glow::color_at(k * 1.2, 6.0, 0.0)), None) {
                    rim.SetOpacity(k as f32);
                    let w = (s / 9.0).max(1.5);
                    let r = ui::R { x: w / 2.0, y: w / 2.0, w: s - w, h: s - w };
                    self.rt.DrawRoundedRectangle(&ui::rounded(&r, s * 0.24 - w / 2.0), &rim, w, None);
                }
            }
            self.rt.EndDraw(None, None).ok()?;
            // icons want straight alpha
            let px = std::slice::from_raw_parts(self.bits, (size * size) as usize);
            let straight: Vec<u32> = px
                .iter()
                .map(|&p| {
                    let a = p >> 24;
                    if a == 0 || a == 255 {
                        return p;
                    }
                    let un = |sh: u32| ((((p >> sh) & 0xFF) * 255 + a / 2) / a).min(255) << sh;
                    (a << 24) | un(16) | un(8) | un(0)
                })
                .collect();
            let color = CreateBitmap(size as i32, size as i32, 1, 32, Some(straight.as_ptr() as *const _));
            let mask = CreateBitmap(size as i32, size as i32, 1, 1, None); // all zero: the alpha decides
            let ii = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let icon = CreateIconIndirect(&ii).ok();
            let _ = DeleteObject(mask);
            let _ = DeleteObject(color);
            icon
        }
    }
}

impl Drop for Badge {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.dib);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(label(1), "1");
        assert_eq!(label(12), "12");
        assert_eq!(label(0), "+");
        assert_eq!(label(100), "+");
    }

    /// Writes a few frames as .bmp files into EZDESK_RENDER_DIR, to look at them.
    #[test]
    #[ignore]
    fn render_samples() {
        let Ok(dir) = std::env::var("EZDESK_RENDER_DIR") else { return };
        for size in [24u32, 96] {
            let mut b = Badge::sized(size).expect("badge");
            b.set(2, 0);
            for (name, at) in [("calm", 0u64), ("calm2", 1300)] {
                let _ = b.frame(at);
                save(&b, &format!("{dir}/{size}-{name}.bmp"));
            }
            b.set(3, 10_000);
            for t in [60u64, 150, 250] {
                let _ = b.frame(10_000 + t);
                save(&b, &format!("{dir}/{size}-roll{t}.bmp"));
            }
            b.set(12, 20_000);
            let _ = b.frame(21_000);
            save(&b, &format!("{dir}/{size}-twelve.bmp"));
        }
    }

    /// The bitmap as it was drawn (premultiplied), over the dark taskbar.
    fn save(b: &Badge, path: &str) {
        let n = b.size as usize;
        let px = unsafe { std::slice::from_raw_parts(b.bits, n * n) };
        let mut out = Vec::new();
        let row = n * 3 + (4 - n * 3 % 4) % 4;
        let len = 54 + row * n;
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        out.extend_from_slice(&(n as i32).to_le_bytes());
        out.extend_from_slice(&(n as i32).to_le_bytes());
        out.extend_from_slice(&[1, 0, 24, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&((row * n) as u32).to_le_bytes());
        out.extend_from_slice(&[0; 16]);
        for y in (0..n).rev() {
            for x in 0..n {
                let p = px[y * n + x];
                let a = (p >> 24) as u32;
                let bg = 0x1Cu32;
                let c = |s: u32| (((p >> s) & 0xFF) + bg * (255 - a) / 255).min(255) as u8;
                out.extend_from_slice(&[c(0), c(8), c(16)]);
            }
            out.extend(std::iter::repeat(0u8).take(row - n * 3));
        }
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn the_roll_starts_and_ends_in_place() {
        assert!(roll_ease(0.0).abs() < 1e-9);
        assert!((roll_ease(1.0) - 1.0).abs() < 1e-6);
    }
}
