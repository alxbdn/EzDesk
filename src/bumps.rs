//! The side bumps: where you take a held window to the previous / next desktop.
//!
//! While a window is held, a drop of light sits against each side of its screen, half-way down,
//! glued to the edge: thickest in its middle, running out along the edge into long, thin points,
//! like water clinging to the side of a glass. Bring the window towards a side and that drop grows
//! longer and thicker, smoothly with the distance, from 200 px away, and a small tile slides out
//! beside it with the number of the desktop you'd land on ("+" past the last one: a new desktop).
//! Into the edge (about 20 px from it) and hold: the drop fills with white from its middle for
//! 0.75 s, then you slide to that desktop, window in hand, and it gives a little bounce. Windows'
//! Snap still works all along: let go against the edge before the 0.75 s and the window snaps
//! there. On desktop 1 there's no left drop.
//!
//! Drawn with Direct2D into two layered windows that clicks go through.

use crate::layered::Layered;
use crate::ui::{self, R};
use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

const QMAX: f32 = 1.4; // closeness plus the bounce's swell
const REACH: f64 = 200.0; // from how far it starts to grow
const CONTACT: f64 = 20.0; // this close to the edge, the window is "in"
const RAISE_MS: u64 = 100; // back on top this often: Snap's preview comes up over everything
// The window round each drop, in pixels at 100 %: the drop, the tile beside it, their light.
const WIN_W: f64 = 64.0;
const WIN_H: f64 = 520.0;
const HAZE: f32 = 9.0; // how far the haze reaches past the drop
const POINTS: f32 = 2.4; // how sharply the ends run out (higher: longer, thinner points)

struct Gfx {
    d2d: ID2D1Factory,
    rt: ID2D1DCRenderTarget,
    stops: ID2D1GradientStopCollection,
    round: Option<ID2D1StrokeStyle>,
    digits: Option<IDWriteTextFormat>,
}

pub struct Bumps {
    win: [Layered; 2], // [left, right]
    s: f64,
    w: usize,
    h: usize,
    gfx: Option<Gfx>,
    pub shown: bool,
    open_at: u64,
    squash: [f64; 2],
    avail: [f64; 2],
    work: RECT,
}

fn spring(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (-6.0 * t).exp() * (9.0 * t).cos() // 0 → 1, overshooting a little
}

fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// The drop at closeness qq (0 → QMAX), in pixels at 100 %: half its length along the edge, and
/// how thick it is in its middle.
fn drop_size(qq: f32) -> (f32, f32) {
    (90.0 + 110.0 * qq, 4.0 + 7.0 * qq)
}

/// How thick the drop is `d` from its middle, along the edge: `thick` there, running out to 0 at
/// ±half in concave points.
fn thickness(d: f32, half: f32, thick: f32) -> f32 {
    let u = (d.abs() / half).min(1.0);
    thick * (1.0 - u * u).powf(POINTS)
}

/// The drop's outline, glued to the edge: down the edge, back up along its thickness. `extra`
/// grows it all round (the haze), its points included.
fn outline(d2d: &ID2D1Factory, edge: f32, side: f32, yc: f32, half: f32, thick: f32, extra: f32) -> Option<ID2D1PathGeometry> {
    const N: usize = 64;
    let reach = half + extra;
    let pts: Vec<D2D_POINT_2F> = (0..=N)
        .map(|i| {
            let d = -reach + 2.0 * reach * i as f32 / N as f32;
            let t = thickness(d, half, thick);
            let x = edge - side * (t + extra * (0.35 + 0.65 * t / thick.max(1e-3)) * (1.0 - ((d.abs() - half).max(0.0) / extra.max(1e-3))));
            D2D_POINT_2F { x, y: yc + d }
        })
        .collect();
    unsafe {
        let g = d2d.CreatePathGeometry().ok()?;
        let sink = g.Open().ok()?;
        sink.BeginFigure(D2D_POINT_2F { x: edge, y: yc - reach }, D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLines(&pts);
        sink.AddLine(D2D_POINT_2F { x: edge, y: yc + reach });
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close().ok()?;
        Some(g)
    }
}

/// The charge's white along the edge, for charge 0 → 1: (position 0 → 1 from top to bottom,
/// opacity). Full in the middle part reached so far, fading softly over a third of the drop on
/// each side; it brightens as it spreads.
fn charge_stops(charge: f32) -> [(f32, f32); 7] {
    let peak = 0.5 + 0.4 * charge;
    let (hc, soft) = (charge, 0.35); // in halves of the drop
    let total = 2.0 * (hc + soft);
    let at = |y: f32| y / total; // from the top of the glow
    [
        (0.0, 0.0),
        (at(soft * 0.5), peak * 0.3),
        (at(soft), peak * 0.8),
        (0.5, peak),
        (1.0 - at(soft), peak * 0.8),
        (1.0 - at(soft * 0.5), peak * 0.3),
        (1.0, 0.0),
    ]
}

fn charge_brush(rt: &ID2D1RenderTarget, yc: f32, half: f32, charge: f32) -> Option<ID2D1LinearGradientBrush> {
    let reach = half * (charge + 0.35);
    let stops: Vec<D2D1_GRADIENT_STOP> = charge_stops(charge)
        .iter()
        .map(|&(position, a)| D2D1_GRADIENT_STOP { position, color: D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a } })
        .collect();
    unsafe {
        let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok()?;
        let line = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: D2D_POINT_2F { x: 0.0, y: yc - reach }, endPoint: D2D_POINT_2F { x: 0.0, y: yc + reach } };
        rt.CreateLinearGradientBrush(&line, None, &coll).ok()
    }
}

/// How far the tile has come out: none until the window is half-way there, all of it by 85 %.
fn tile_in(q: f32) -> f32 {
    ease((q - 0.5) / 0.35)
}

impl Bumps {
    pub fn new(scale: f64) -> Self {
        let (w, h) = ((WIN_W * scale).round() as usize, (WIN_H * scale).round() as usize);
        Bumps {
            win: [Layered::new(w, h), Layered::new(w, h)],
            s: scale,
            w,
            h,
            gfx: None,
            shown: false,
            open_at: 0,
            squash: [0.0; 2],
            avail: [0.0, 1.0],
            work: RECT::default(),
        }
    }

    pub fn hwnds(&self) -> [isize; 2] {
        [self.win[0].hwnd.0 as isize, self.win[1].hwnd.0 as isize]
    }

    /// On the screen whose work area is `work`. first: on desktop 1 (no left drop).
    pub fn open(&mut self, work: RECT, now: u64, first: bool) {
        self.work = work;
        self.squash = [0.0; 2];
        self.avail = [if first { 0.0 } else { 1.0 }, 1.0];
        self.open_at = now;
        self.shown = true;
    }

    pub fn hide(&mut self) {
        for w in &mut self.win {
            w.hide();
        }
        self.shown = false;
    }

    /// The little bounce after a switch that way.
    pub fn bounce(&mut self, side: i32) {
        self.squash[if side < 0 { 0 } else { 1 }] = 1.0;
    }

    /// Sets up the drawing ahead of time, so the first frame is quick.
    pub fn warm(&mut self) {
        if self.gfx.is_some() {
            return;
        }
        unsafe {
            let Ok(d2d) = D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) else { return };
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let Ok(rt) = d2d.CreateDCRenderTarget(&props) else { return };
            let Some(stops) = rt.cast::<ID2D1RenderTarget>().ok().and_then(|b| ui::glow_stops(&b)) else { return };
            let caps = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                ..Default::default()
            };
            let round = d2d.CreateStrokeStyle(&caps, None).ok();
            let digits = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)
                .ok()
                .and_then(|dw| ui::text_format(&dw, (13.0 * self.s) as f32, DWRITE_FONT_WEIGHT_SEMI_BOLD))
                .inspect(|f| {
                    let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                });
            self.gfx = Some(Gfx { d2d, rt, stops, round, digits });
        }
    }

    /// One frame of both drops, cursor at (x, y). cur / count: the desktop you're on (from 0), and
    /// how many. fill(side): how full of light that one is. Returns the side the cursor is in (-1
    /// left, +1 right, 0 neither).
    pub fn frame(&mut self, x: i32, now: u64, cur: i32, count: i32, fill: impl Fn(i32) -> f64) -> i32 {
        if !self.shown {
            return 0;
        }
        self.warm();
        let RECT { left: wl, top: wt, right: wr, bottom: wb } = self.work;
        let yc = (wt + wb) as f64 / 2.0; // fixed, half-way down the screen
        let mut contact = 0;
        for i in 0..2 {
            let side = if i == 0 { -1 } else { 1 };
            let want = if side < 0 { (cur > 0) as i32 as f64 } else { 1.0 };
            self.avail[i] += (want - self.avail[i]) * 0.12; // desktop 1: the left one tucks away
            let dist = if side < 0 { (x - wl) as f64 } else { (wr - 1 - x) as f64 };
            let near = (1.0 - dist / (REACH * self.s)).clamp(0.0, 1.0);
            let p = 1.0 - (1.0 - near).powi(3);
            let grow = spring((now.saturating_sub(self.open_at) as f64 - 120.0 - i as f64 * 60.0) / 600.0) * self.avail[i];
            self.squash[i] *= 0.88;
            if self.avail[i] > 0.5 && dist >= -1.0 && dist <= CONTACT * self.s {
                contact = side;
            }
            let alpha = (255.0 * (0.5 + 0.5 * p) * (grow.max(0.0) * 1.5).min(1.0)).round();
            if alpha < 3.0 {
                if self.win[i].visible {
                    self.win[i].hide();
                }
                continue;
            }
            let q = (p * grow.max(0.0)) as f32;
            let swell = (self.squash[i] * grow.max(0.0)) as f32;
            let label = if side > 0 && cur + 1 >= count { None } else { Some(cur + 1 + side) }; // None: a new desktop
            self.draw(i, side, q, swell, fill(side) as f32, label);
            let (w, h) = (self.w as i32, self.h as i32);
            let win = &mut self.win[i];
            win.put(if side < 0 { wl } else { wr - w }, (yc - h as f64 / 2.0).round() as i32, alpha.min(255.0) as u8);
            if !win.visible || now.saturating_sub(win.raised_at) > RAISE_MS {
                win.raise(now);
            }
        }
        contact
    }

    /// One drop into its window. q: closeness 0 → 1; swell: the bounce; charge: the white, 0 → 1;
    /// label: the desktop that way (None: "+").
    fn draw(&self, i: usize, side: i32, q: f32, swell: f32, charge: f32, label: Option<i32>) {
        let Some(g) = self.gfx.as_ref() else { return };
        let (s, w, h) = (self.s as f32, self.w as f32, self.h as f32);
        let (edge, sd, yc) = (if side > 0 { w } else { 0.0 }, side as f32, h / 2.0);
        let qq = (q + 0.5 * swell).clamp(0.0, QMAX);
        let (half, thick) = drop_size(qq);
        let (half, thick) = (half * s, thick * s);
        let round = g.round.as_ref();
        unsafe {
            let rt = &g.rt;
            if rt.BindDC(self.win[i].dc(), &RECT { left: 0, top: 0, right: self.w as i32, bottom: self.h as i32 }).is_err() {
                return;
            }
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let Ok(base) = rt.cast::<ID2D1RenderTarget>() else {
                let _ = rt.EndDraw(None, None);
                return;
            };
            // the drop: the light flowing up it, in a haze of larger, fainter drops
            if let Some(light) = ui::flow_brush(&base, &g.stops, edge, yc + 2.0 * half, 0.0, -4.0 * half) {
                for j in (0..=6).rev() {
                    let f = 1.0 - j as f32 / 7.0;
                    light.SetOpacity(if j == 0 { 1.0 } else { 0.22 * f * f });
                    if let Some(shape) = outline(&g.d2d, edge, sd, yc, half, thick, j as f32 / 6.0 * HAZE * s) {
                        rt.FillGeometry(&shape, &light, None);
                    }
                }
            }
            // the charge: a soft white glow spreading from the middle out, in the drop's own shape
            if charge > 0.0 {
                if let Some(white) = charge_brush(&base, yc, half, charge.min(1.0)) {
                    for j in (0..=4).rev() {
                        let f = 1.0 - j as f32 / 5.0;
                        white.SetOpacity(if j == 0 { 1.0 } else { 0.3 * f * f });
                        if let Some(shape) = outline(&g.d2d, edge, sd, yc, half, thick, j as f32 / 4.0 * HAZE * s) {
                            rt.FillGeometry(&shape, &white, None);
                        }
                    }
                }
            }
            // the tile with the desktop you'd land on
            let k = tile_in(q);
            if k > 0.0 {
                let cs = 26.0 * s;
                let cx = edge - sd * (thick + 14.0 * s + cs / 2.0 - (1.0 - k) * 10.0 * s);
                let r = R { x: cx - cs / 2.0, y: yc - cs / 2.0, w: cs, h: cs };
                ui::aura_faded(&base, &g.stops, &r, 7.0 * s, 5, s, k);
                if let (Ok(bg), Ok(fg)) = (rt.CreateSolidColorBrush(&ui::color(ui::BG), None), rt.CreateSolidColorBrush(&ui::color(ui::FG), None)) {
                    bg.SetOpacity(k);
                    fg.SetOpacity(k);
                    rt.FillRoundedRectangle(&ui::rounded(&r, 7.0 * s), &bg);
                    match (label, g.digits.as_ref()) {
                        (Some(n), Some(f)) => ui::text(&base, &n.to_string(), f, &R { y: r.y - 0.5 * s, ..r }, &fg),
                        _ => {
                            let a = 6.0 * s;
                            rt.DrawLine(D2D_POINT_2F { x: cx - a, y: yc }, D2D_POINT_2F { x: cx + a, y: yc }, &fg, 2.0 * s, round);
                            rt.DrawLine(D2D_POINT_2F { x: cx, y: yc - a }, D2D_POINT_2F { x: cx, y: yc + a }, &fg, 2.0 * s, round);
                        }
                    }
                }
            }
            let _ = rt.EndDraw(None, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drop_and_its_tile_fit_their_window() {
        let (half, thick) = drop_size(QMAX);
        assert!((2.0 * (half + HAZE)) as f64 <= WIN_H);
        // the drop + space + tile + its aura, from the edge
        assert!((thick + 14.0 + 26.0 + 6.0) as f64 <= WIN_W);
        assert!((thick + HAZE) as f64 <= WIN_W);
    }

    #[test]
    fn the_drop_is_thickest_in_its_middle_and_runs_out_to_points() {
        let (half, thick) = drop_size(1.0);
        assert_eq!(thickness(0.0, half, thick), thick);
        assert_eq!(thickness(half, half, thick), 0.0);
        assert_eq!(thickness(-half, half, thick), 0.0);
        let along: Vec<f32> = (0..=20).map(|i| thickness(half * i as f32 / 20.0, half, thick)).collect();
        assert!(along.windows(2).all(|p| p[1] <= p[0]), "thinner and thinner out to the end");
        assert!(thickness(0.8 * half, half, thick) < 0.1 * thick, "long, thin points");
    }

    #[test]
    fn the_charge_glows_softly_from_the_middle() {
        for charge in [0.05, 0.5, 1.0] {
            let st = charge_stops(charge);
            assert!(st.windows(2).all(|p| p[1].0 >= p[0].0), "stops in order");
            assert_eq!((st[0].1, st[6].1), (0.0, 0.0), "no hard edge: it fades to nothing");
            assert!(st.iter().all(|&(_, a)| a <= st[3].1), "brightest in the middle");
        }
        assert!(charge_stops(1.0)[3].1 > charge_stops(0.1)[3].1, "brighter as it spreads");
    }

    #[test]
    fn it_grows_but_not_too_far() {
        assert_eq!(drop_size(0.0).0, 90.0);
        assert_eq!(drop_size(1.0).0, 200.0);
    }

    #[test]
    fn the_tile_comes_out_from_half_way() {
        assert_eq!(tile_in(0.4), 0.0);
        assert_eq!(tile_in(0.9), 1.0);
        assert!(tile_in(0.65) > 0.0 && tile_in(0.65) < 1.0);
    }
}

#[cfg(test)]
mod look {
    use super::*;

    /// Writes the right drop at a few moments as one .bmp into EZDESK_RENDER_DIR, to look at it.
    #[test]
    #[ignore]
    fn render_samples() {
        let Ok(dir) = std::env::var("EZDESK_RENDER_DIR") else { return };
        let mut b = Bumps::new(1.5);
        b.warm();
        let states: [(f32, f32, Option<i32>); 5] = [(0.0, 0.0, Some(2)), (0.45, 0.0, Some(2)), (0.8, 0.0, Some(2)), (1.0, 0.25, Some(2)), (1.0, 0.75, Some(2))];
        let (w, h) = (b.w, b.h);
        let mut sheet = vec![0u32; w * states.len() * h];
        for (k, (q, charge, label)) in states.iter().enumerate() {
            b.draw(1, 1, *q, 0.0, *charge, *label);
            let px = b.win[1].snapshot();
            for y in 0..h {
                for x in 0..w {
                    sheet[y * w * states.len() + k * w + x] = px[y * w + x];
                }
            }
        }
        let (sw, sh) = (w * states.len(), h);
        let row = sw * 3 + (4 - sw * 3 % 4) % 4;
        let mut out = Vec::new();
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&((54 + row * sh) as u32).to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        out.extend_from_slice(&(sw as i32).to_le_bytes());
        out.extend_from_slice(&(sh as i32).to_le_bytes());
        out.extend_from_slice(&[1, 0, 24, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&((row * sh) as u32).to_le_bytes());
        out.extend_from_slice(&[0; 16]);
        for y in (0..sh).rev() {
            for x in 0..sw {
                let p = sheet[y * sw + x];
                let a = p >> 24;
                let bg = 0x22u32; // a dark wallpaper
                let c = |s: u32| (((p >> s) & 0xFF) + bg * (255 - a) / 255).min(255) as u8;
                out.extend_from_slice(&[c(0), c(8), c(16)]);
            }
            out.extend(std::iter::repeat(0u8).take(row - sw * 3));
        }
        std::fs::write(format!("{dir}/rail.bmp"), out).unwrap();
    }
}
