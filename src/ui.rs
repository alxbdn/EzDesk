//! EzDesk's own UI look, shared by the mini-dock and the menus: shadcn/ui's dark "zinc" theme
//! (near-black popover, hairline borders, rounded corners, muted text) drawn with Direct2D, and
//! EzDesk's light round what's under the mouse (Windows blue brightening to white and back,
//! flowing, as on the bumps and the tray number).

use crate::glow;
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;

// ---- shadcn/ui, dark "zinc" theme ----
pub const BG: u32 = 0x09090B; // popover
pub const BORDER: u32 = 0x27272A; // border, input
pub const ACCENT: u32 = 0x18181B; // hover background (cards)
pub const SECONDARY: u32 = 0x27272A; // hover background (buttons, menu items)
pub const FG: u32 = 0xFAFAFA; // foreground
pub const MUTED: u32 = 0xA1A1AA; // muted-foreground
pub const DISABLED: u32 = 0x52525B; // zinc-600
pub const RING: u32 = 0xD4D4D8; // ring
pub const DASH: u32 = 0x3F3F46; // zinc-700

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.x + self.w, bottom: self.y + self.h }
    }
    pub fn grow(&self, e: f32) -> R {
        R { x: self.x - e, y: self.y - e, w: self.w + 2.0 * e, h: self.h + 2.0 * e }
    }
}

pub fn color(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: ((rgb >> 16) & 0xFF) as f32 / 255.0, g: ((rgb >> 8) & 0xFF) as f32 / 255.0, b: (rgb & 0xFF) as f32 / 255.0, a: 1.0 }
}

pub fn rounded(r: &R, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius }
}

/// A text style: Segoe UI Variable (Windows 11's), else Segoe UI; one line, centred vertically,
/// "…" when it doesn't fit.
pub fn text_format(dw: &IDWriteFactory, size: f32, weight: DWRITE_FONT_WEIGHT) -> Option<IDWriteTextFormat> {
    unsafe {
        let f = dw
            .CreateTextFormat(w!("Segoe UI Variable Text"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
            .or_else(|_| dw.CreateTextFormat(w!("Segoe UI"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us")))
            .ok()?;
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        if let Ok(sign) = dw.CreateEllipsisTrimmingSign(&f) {
            let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            let _ = f.SetTrimming(&trim, &sign);
        }
        Some(f)
    }
}

/// Draws one line of text in r.
pub fn text(rt: &ID2D1RenderTarget, s: &str, f: &IDWriteTextFormat, r: &R, brush: &ID2D1SolidColorBrush) {
    let t: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        rt.DrawText(&t, f, &r.d2d(), brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
    }
}

/// Windows 11: the window gets Windows' own rounded corners and shadow, and a hairline border in
/// the theme's colour.
pub fn style_window(hwnd: HWND) {
    unsafe {
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
        let border: u32 = ((BORDER & 0xFF) << 16) | (BORDER & 0xFF00) | ((BORDER >> 16) & 0xFF); // COLORREF (BGR)
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const _, 4);
    }
}

// ---- The light round what's under the mouse ----

/// Where the light's flow is, 0 → 1 once per glow::FLOW_MS, from a precise clock (the system
/// tick moves in ~16 ms steps: the light would stutter).
pub fn flow_phase() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let ms = START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0;
    ((ms % glow::FLOW_MS as f64) / glow::FLOW_MS as f64) as f32
}

/// The palette sampled finely with the bumps' own easing (glow::color_at), its last stop the same
/// as its first: repeated, it has no seam. Made once per render target.
pub fn glow_stops(rt: &ID2D1RenderTarget) -> Option<ID2D1GradientStopCollection> {
    const SAMPLES: usize = 48;
    let stops: Vec<D2D1_GRADIENT_STOP> = (0..=SAMPLES)
        .map(|j| D2D1_GRADIENT_STOP { position: j as f32 / SAMPLES as f32, color: color(glow::color_at(j as f64, SAMPLES as f64, 0.0)) })
        .collect();
    unsafe { rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_WRAP).ok() }
}

/// The flowing light as a brush over r: one cycle of the palette is the vector v, sliding along
/// itself by phase × v, so phase 1 is exactly phase 0 (the loop doesn't show).
pub fn glow_brush(rt: &ID2D1RenderTarget, stops: &ID2D1GradientStopCollection, r: &R) -> Option<ID2D1LinearGradientBrush> {
    let len = (r.w + r.h).max(1.0); // the whole palette along it
    flow_brush(rt, stops, r.x, r.y, len, len * 0.35)
}

/// The flowing light starting at (x, y), one cycle of the palette along the vector (vx, vy), in
/// whatever direction (the bumps' rail: straight up).
pub fn flow_brush(rt: &ID2D1RenderTarget, stops: &ID2D1GradientStopCollection, x: f32, y: f32, vx: f32, vy: f32) -> Option<ID2D1LinearGradientBrush> {
    let phase = flow_phase();
    let (sx, sy) = (x + vx * phase, y + vy * phase);
    let line = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: D2D_POINT_2F { x: sx, y: sy }, endPoint: D2D_POINT_2F { x: sx + vx, y: sy + vy } };
    unsafe { rt.CreateLinearGradientBrush(&line, None, stops).ok() }
}

/// A check (✓) drawn in the flowing light, with a soft halo: centred on (cx, cy), `size` across.
pub fn glow_check(rt: &ID2D1RenderTarget, stops: &ID2D1GradientStopCollection, cx: f32, cy: f32, size: f32, s: f32) {
    let k = size / 10.0;
    let p = |dx: f32, dy: f32| D2D_POINT_2F { x: cx + dx * k, y: cy + dy * k };
    let (a, b, c) = (p(-4.5, 0.3), p(-1.4, 3.4), p(4.6, -3.6));
    let r = R { x: cx - size / 2.0, y: cy - size / 2.0, w: size, h: size };
    unsafe {
        let Some(g) = glow_brush(rt, stops, &r) else { return };
        let round = rt.GetFactory().ok().and_then(|f: ID2D1Factory| {
            let props = D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, ..Default::default() };
            f.CreateStrokeStyle(&props, None).ok()
        });
        for (opacity, width) in [(0.12, 6.0), (0.25, 3.6), (1.0, 1.8)] {
            g.SetOpacity(opacity); // halo, then the line itself
            rt.DrawLine(a, b, &g, width * s, round.as_ref());
            rt.DrawLine(b, c, &g, width * s, round.as_ref());
        }
    }
}

/// The aura round r (corners `radius`): a bright line on its edge, then `steps` thin rings fading
/// outward, the colours flowing along it.
pub fn aura(rt: &ID2D1RenderTarget, stops: &ID2D1GradientStopCollection, r: &R, radius: f32, steps: u32, s: f32) {
    aura_faded(rt, stops, r, radius, steps, s, 1.0);
}

/// The aura, × alpha (0 → 1: fading in).
pub fn aura_faded(rt: &ID2D1RenderTarget, stops: &ID2D1GradientStopCollection, r: &R, radius: f32, steps: u32, s: f32, alpha: f32) {
    unsafe {
        let Some(g) = glow_brush(rt, stops, r) else { return };
        for k in 0..=steps {
            let e = k as f32 * s;
            let f = 1.0 - k as f32 / (steps + 1) as f32;
            g.SetOpacity(alpha * if k == 0 { 0.95 } else { 0.42 * f * f });
            rt.DrawRoundedRectangle(&rounded(&r.grow(e), radius + e), &g, if k == 0 { 1.5 * s } else { 1.2 * s }, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glows_palette_loops_without_a_seam() {
        // the gradient's first and last stops: one colour, so repeating it shows no edge
        assert_eq!(glow::color_at(0.0, 48.0, 0.0), glow::color_at(48.0, 48.0, 0.0));
        assert!((0.0..1.0).contains(&flow_phase()));
    }

    #[test]
    fn colours_and_rects() {
        let c = color(0x0063B1);
        assert!((c.b - 0xB1 as f32 / 255.0).abs() < 1e-6 && c.r == 0.0);
        let r = R { x: 10.0, y: 10.0, w: 20.0, h: 10.0 };
        assert!(r.contains(10.0, 19.9) && !r.contains(30.0, 15.0));
        assert_eq!(r.grow(2.0), R { x: 8.0, y: 8.0, w: 24.0, h: 14.0 });
    }
}
