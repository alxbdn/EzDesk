//! The mini-dock: what the tray number shows on hover, in place of a tooltip. One card per desktop
//! (its name, its wallpaper and its windows, live), the current one ringed; click to go there, ×
//! to close it, + for a new one, right-click to rename it. Like the desktops flyout of Windows 11's
//! Task View button, next to the number.
//!
//! Drawn with Direct2D in shadcn/ui's look (its dark "zinc" theme: near-black popover, hairline
//! borders, 8 px corners, muted text, a light ring for the current item). The windows' miniatures
//! are DWM thumbnails, drawn by Windows itself over the cards.
//!
//! `layout`, `hit` and `place` are pure (and tested); `Dock` owns the window and draws it.

use crate::ui::{self, color, rounded, ACCENT, BG, BORDER, DASH, FG, MUTED, RING, SECONDARY};
pub use crate::ui::R;
use crate::win::{self, pcw, wide};
use std::collections::HashMap;
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoIncrementMTAUsage, CLSCTX_INPROC_SERVER};

// ---------------------------------------------------------------------
//  Layout (pure)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    /// The desktop (0-based), or -1 for "New desktop".
    pub target: i32,
    /// The whole card (its hover background).
    pub area: R,
    pub label: R,
    pub thumb: R,
    /// The × (desktops only).
    pub close: R,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pub cards: Vec<Card>,
}

/// n desktops (+ "New desktop" if with_new), miniatures with the screen's shape (sw × sh),
/// shrunk to fit max_w. s: the display scale.
pub fn layout(n: usize, with_new: bool, sw: f32, sh: f32, max_w: f32, s: f32) -> Layout {
    let (pad, gap, label_h) = (16.0 * s, 14.0 * s, 28.0 * s); // room for the glow round a card
    let k = n + with_new as usize;
    let mut tw = 176.0 * s;
    if k > 0 && 2.0 * pad + k as f32 * tw + (k - 1) as f32 * gap > max_w {
        tw = ((max_w - 2.0 * pad - (k - 1) as f32 * gap) / k as f32).max(48.0 * s);
    }
    let th = (tw * sh / sw.max(1.0)).round();
    let cards = (0..k)
        .map(|i| {
            let x = pad + i as f32 * (tw + gap);
            let y = pad;
            let b = 20.0 * s; // the × button
            Card {
                target: if i < n { i as i32 } else { -1 },
                area: R { x: x - 4.0 * s, y: y - 4.0 * s, w: tw + 8.0 * s, h: label_h + th + 8.0 * s },
                label: R { x, y, w: tw, h: label_h },
                thumb: R { x, y: y + label_h, w: tw, h: th },
                close: R { x: x + tw - b, y: y + (label_h - b) / 2.0 - 2.0 * s, w: b, h: b },
            }
        })
        .collect();
    Layout { w: (2.0 * pad + k as f32 * tw + k.saturating_sub(1) as f32 * gap).ceil(), h: (2.0 * pad + label_h + th).ceil(), cards }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    None,
    Card(i32),
    Close(i32),
    New,
}

/// What's at (x, y) in the dock. can_close: the × is offered (Windows 11, 2+ desktops).
pub fn hit(l: &Layout, x: f32, y: f32, can_close: bool) -> Hit {
    for c in &l.cards {
        if !c.area.contains(x, y) {
            continue;
        }
        if c.target < 0 {
            return Hit::New;
        }
        if can_close && c.close.contains(x, y) {
            return Hit::Close(c.target);
        }
        return Hit::Card(c.target);
    }
    Hit::None
}

/// Dragging desktop `from` (of n) dx pixels sideways, cards `slot` apart: how far each card is
/// drawn from its place (the dragged one follows the mouse, kept within the row; the others make
/// room), and the position it would be dropped at.
pub fn drag_offsets(n: usize, from: usize, dx: f32, slot: f32) -> (Vec<f32>, usize) {
    let dx = dx.clamp(-(from as f32) * slot, (n - 1 - from) as f32 * slot);
    let to = ((from as f32 + dx / slot).round() as isize).clamp(0, n as isize - 1) as usize;
    let offsets = (0..n)
        .map(|i| {
            if i == from {
                dx
            } else if from < to && i > from && i <= to {
                -slot // shifts left, into the gap
            } else if to < from && i >= to && i < from {
                slot
            } else {
                0.0
            }
        })
        .collect();
    (offsets, to)
}

/// Where the dock goes, w × h, for the tray icon at `icon` on a screen `mon` whose work area (less
/// the taskbar) is `work`: on the taskbar's side of the screen (found from where the work area is
/// cut), next to the icon, centred on it, kept on the screen. The icon may be in the taskbar or in
/// the ^ overflow panel: the dock opens past it either way.
pub fn place(w: i32, h: i32, icon: RECT, work: RECT, mon: RECT, s: f32) -> (i32, i32) {
    let m = (8.0 * s) as i32;
    let clamp = |v: i32, lo: i32, hi: i32| v.max(lo).min(hi.max(lo));
    let (cx, cy) = ((icon.left + icon.right) / 2, (icon.top + icon.bottom) / 2);
    let x_on_icon = clamp(cx - w / 2, work.left + m, work.right - w - m);
    let y_on_icon = clamp(cy - h / 2, work.top + m, work.bottom - h - m);
    if work.top > mon.top {
        (x_on_icon, clamp(icon.bottom.max(work.top) + m, work.top + m, work.bottom - h - m)) // taskbar at the top: below
    } else if work.left > mon.left {
        (clamp(icon.right.max(work.left) + m, work.left + m, work.right - w - m), y_on_icon) // on the left: right of it
    } else if work.right < mon.right {
        (clamp(icon.left.min(work.right) - w - m, work.left + m, work.right - w - m), y_on_icon) // on the right: left of it
    } else {
        (x_on_icon, clamp(icon.top.min(work.bottom) - h - m, work.top + m, work.bottom - h - m)) // at the bottom (or hidden): above
    }
}

// ---------------------------------------------------------------------
//  What it shows (gathered by the app)
// ---------------------------------------------------------------------

pub struct DockData {
    pub labels: Vec<String>,
    pub current: i32,
    pub can_close: bool,
    pub can_reorder: bool,
    pub with_new: bool,
    /// Each desktop's wallpaper file.
    pub walls: Vec<Option<String>>,
    /// The windows to show, bottom-most first: (window, its desktop).
    pub windows: Vec<(isize, i32)>,
    /// The screen the miniatures show.
    pub screen: RECT,
}

/// A card dragged sideways to reorder the desktops.
#[derive(Debug, Clone, Copy)]
pub struct Drag {
    pub from: i32,
    start_x: f32,
    x: f32,
    /// Moved far enough to be a drag, not a click.
    pub active: bool,
}

// ---------------------------------------------------------------------
//  The window
// ---------------------------------------------------------------------

pub struct Dock {
    pub hwnd: HWND,
    s: f32,
    d2d: ID2D1Factory,
    rt: Option<ID2D1HwndRenderTarget>,
    wic: Option<IWICImagingFactory>,
    text: IDWriteTextFormat,
    text_bold: IDWriteTextFormat,
    dashed: Option<ID2D1StrokeStyle>,
    walls: HashMap<String, Option<ID2D1Bitmap>>,
    /// The windows' miniatures: (thumbnail, its desktop, where it is when nothing's dragged).
    thumbs: Vec<(isize, i32, RECT)>,
    pub shown: bool,
    pub layout: Layout,
    pub pos: (i32, i32),
    pub labels: Vec<String>,
    pub current: i32,
    pub can_close: bool,
    pub can_reorder: bool,
    wall_of: Vec<Option<String>>,
    pub hover: Hit,
    /// Being renamed: the label shows an input instead.
    pub renaming: Option<i32>,
    pub drag: Option<Drag>,
}

impl Dock {
    /// hwnd: the dock's window, made by the app (it owns the window procedure).
    pub fn new(hwnd: HWND, s: f32) -> Option<Self> {
        unsafe {
            let _ = CoIncrementMTAUsage();
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let text = ui::text_format(&dw, 13.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
            let text_bold = ui::text_format(&dw, 13.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let props = D2D1_STROKE_STYLE_PROPERTIES { dashStyle: D2D1_DASH_STYLE_CUSTOM, ..Default::default() };
            let dashed = d2d.CreateStrokeStyle(&props, Some(&[3.0, 3.0])).ok();
            let wic = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok();
            ui::style_window(hwnd);
            Some(Dock {
                hwnd,
                s,
                d2d,
                rt: None,
                wic,
                text,
                text_bold,
                dashed,
                walls: HashMap::new(),
                thumbs: vec![],
                shown: false,
                layout: Layout::default(),
                pos: (0, 0),
                labels: vec![],
                current: -1,
                can_close: false,
                can_reorder: false,
                wall_of: vec![],
                hover: Hit::None,
                renaming: None,
                drag: None,
            })
        }
    }

    /// Opens it for the tray icon at (ax, ay), or rebuilds it in place when it's open.
    /// icon: where the tray icon is (in the taskbar, or in the ^ panel).
    pub fn show(&mut self, data: DockData, icon: RECT) {
        let (ax, ay) = ((icon.left + icon.right) / 2, (icon.top + icon.bottom) / 2);
        let sc = data.screen;
        let work = win::work_area_at(ax, ay);
        let l = layout(data.labels.len(), data.with_new, (sc.right - sc.left) as f32, (sc.bottom - sc.top) as f32, (work.right - work.left) as f32 * 0.9, self.s);
        let (w, h) = (l.w as i32, l.h as i32);
        let pos = if self.shown && self.layout.w as i32 == w && self.layout.h as i32 == h { self.pos } else { place(w, h, icon, work, win::monitor_at(ax, ay), self.s) };
        self.layout = l;
        self.pos = pos;
        self.labels = data.labels;
        self.current = data.current;
        self.can_close = data.can_close;
        self.can_reorder = data.can_reorder;
        self.wall_of = data.walls;
        self.drag = None;
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, pos.0, pos.1, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        self.shown = true;
        self.ensure_target(w as u32, h as u32);
        self.thumbnails(&data.windows, sc);
        self.redraw();
    }

    pub fn hide(&mut self) {
        self.clear_thumbnails();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(self.hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
        }
        self.shown = false;
        self.hover = Hit::None;
        self.renaming = None;
        self.drag = None;
    }

    // ---- Dragging a card to reorder the desktops ----

    fn slot(&self) -> f32 {
        match self.layout.cards.get(..2) {
            Some([a, b]) => b.thumb.x - a.thumb.x,
            _ => 1.0,
        }
    }

    fn desktops(&self) -> usize {
        self.layout.cards.iter().filter(|c| c.target >= 0).count()
    }

    /// The button went down on desktop i's card (it may be a click, or the start of a drag).
    pub fn press(&mut self, i: i32, x: f32) {
        self.drag = (self.can_reorder && self.desktops() > 1).then_some(Drag { from: i, start_x: x, x, active: false });
    }

    /// The mouse moved with the button down: past a few pixels, the card follows it.
    pub fn drag_to(&mut self, x: f32) {
        let s = self.s;
        let Some(d) = self.drag.as_mut() else { return };
        d.x = x;
        if !d.active && (x - d.start_x).abs() > 6.0 * s {
            d.active = true;
        }
        if d.active {
            self.move_thumbnails();
            self.redraw();
        }
    }

    /// The button went up: Some((from, to)) if a card was dragged to another position.
    pub fn release(&mut self) -> Option<(i32, i32)> {
        let d = self.drag.take()?;
        if !d.active {
            return None;
        }
        let (_, to) = drag_offsets(self.desktops(), d.from as usize, d.x - d.start_x, self.slot());
        self.move_thumbnails();
        self.redraw();
        (to as i32 != d.from).then_some((d.from, to as i32))
    }

    /// How far each card is drawn from its place, now.
    fn offsets(&self) -> Vec<f32> {
        match self.drag.filter(|d| d.active) {
            Some(d) => drag_offsets(self.desktops(), d.from as usize, d.x - d.start_x, self.slot()).0,
            None => vec![0.0; self.desktops()],
        }
    }

    fn move_thumbnails(&self) {
        let off = self.offsets();
        for &(t, d, r) in &self.thumbs {
            let dx = off.get(d as usize).copied().unwrap_or(0.0).round() as i32;
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION,
                rcDestination: RECT { left: r.left + dx, top: r.top, right: r.right + dx, bottom: r.bottom },
                ..Default::default()
            };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(t, &props);
            }
        }
    }

    /// The dock on screen (for "is the cursor over it?").
    pub fn screen_rect(&self) -> RECT {
        RECT { left: self.pos.0, top: self.pos.1, right: self.pos.0 + self.layout.w as i32, bottom: self.pos.1 + self.layout.h as i32 }
    }

    pub fn set_hover(&mut self, h: Hit) {
        if h != self.hover {
            self.hover = h;
            self.redraw();
        }
    }

    pub fn redraw(&self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, None, false);
        }
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        let size = D2D_SIZE_U { width: w, height: h };
        if let Some(rt) = &self.rt {
            unsafe {
                let _ = rt.Resize(&size);
            }
            return;
        }
        let props = D2D1_RENDER_TARGET_PROPERTIES { dpiX: 96.0, dpiY: 96.0, ..Default::default() }; // pixels
        let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES { hwnd: self.hwnd, pixelSize: size, presentOptions: D2D1_PRESENT_OPTIONS_NONE };
        self.rt = unsafe { self.d2d.CreateHwndRenderTarget(&props, &hp) }.ok();
        self.walls.clear(); // bitmaps belong to a render target
    }

    // ---- The windows' miniatures, drawn by Windows over the cards ----

    fn clear_thumbnails(&mut self) {
        for (t, _, _) in self.thumbs.drain(..) {
            unsafe {
                let _ = DwmUnregisterThumbnail(t);
            }
        }
    }

    fn thumbnails(&mut self, windows: &[(isize, i32)], sc: RECT) {
        self.clear_thumbnails();
        let (sw, sh) = ((sc.right - sc.left) as f32, (sc.bottom - sc.top) as f32);
        for &(src, d) in windows {
            let Some(card) = self.layout.cards.iter().find(|c| c.target == d) else { continue };
            let t = card.thumb;
            let mut r = RECT::default();
            if unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(win::hwnd(src), &mut r) }.is_err() {
                continue;
            }
            let (x1, y1, x2, y2) = (r.left.max(sc.left), r.top.max(sc.top), r.right.min(sc.right), r.bottom.min(sc.bottom));
            if x2 <= x1 || y2 <= y1 {
                continue; // not on this screen
            }
            let Ok(thumb) = (unsafe { DwmRegisterThumbnail(self.hwnd, win::hwnd(src)) }) else { continue };
            let fx = |x: i32| (t.x + (x - sc.left) as f32 * t.w / sw).round() as i32;
            let fy = |y: i32| (t.y + (y - sc.top) as f32 * t.h / sh).round() as i32;
            let dest = RECT { left: fx(x1), top: fy(y1), right: fx(x2), bottom: fy(y2) };
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_RECTSOURCE | DWM_TNP_VISIBLE,
                rcDestination: dest,
                rcSource: RECT { left: x1 - r.left, top: y1 - r.top, right: x2 - r.left, bottom: y2 - r.top },
                fVisible: true.into(),
                ..Default::default()
            };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(thumb, &props);
            }
            self.thumbs.push((thumb, d, dest));
        }
    }

    // ---- Wallpapers: decoded by WIC, made small, kept ----

    fn wallpaper(&mut self, path: &str, w: f32, h: f32) -> Option<ID2D1Bitmap> {
        if let Some(b) = self.walls.get(path) {
            return b.clone();
        }
        let b = self.load(path, w, h);
        self.walls.insert(path.to_string(), b.clone());
        b
    }

    fn load(&self, path: &str, w: f32, h: f32) -> Option<ID2D1Bitmap> {
        let (wic, rt) = (self.wic.as_ref()?, self.rt.as_ref()?);
        let p = wide(path);
        unsafe {
            let dec = wic.CreateDecoderFromFilename(pcw(&p), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
            let frame = dec.GetFrame(0).ok()?;
            let (mut fw, mut fh) = (0u32, 0u32);
            frame.GetSize(&mut fw, &mut fh).ok()?;
            // scaled once to twice the card's size (sharp, and small in memory), covering it
            let k = (2.0 * w / fw as f32).max(2.0 * h / fh as f32).min(1.0);
            let scaler = wic.CreateBitmapScaler().ok()?;
            scaler.Initialize(&frame, ((fw as f32 * k) as u32).max(1), ((fh as f32 * k) as u32).max(1), WICBitmapInterpolationModeFant).ok()?;
            let conv = wic.CreateFormatConverter().ok()?;
            conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
            rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        }
    }

    // ---- Drawing ----

    pub fn paint(&mut self) {
        let Some(rt) = self.rt.clone() else { return };
        let s = self.s;
        // During a drag: each card drawn where it's going, the dragged one last (on top).
        let off = self.offsets();
        let dragged = self.drag.filter(|d| d.active).map(|d| d.from);
        let shift = |r: R, dx: f32| R { x: r.x + dx, ..r };
        let mut cards: Vec<Card> = self
            .layout
            .cards
            .iter()
            .map(|c| {
                let dx = if c.target >= 0 { off.get(c.target as usize).copied().unwrap_or(0.0) } else { 0.0 };
                Card { area: shift(c.area, dx), label: shift(c.label, dx), thumb: shift(c.thumb, dx), close: shift(c.close, dx), ..*c }
            })
            .collect();
        if let Some(f) = dragged {
            cards.sort_by_key(|c| c.target == f);
        }
        unsafe {
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.Clear(Some(&color(BG)));
            let brush = |c: u32| rt.CreateSolidColorBrush(&color(c), None).ok();
            let (bg, border, accent, secondary, fg, muted, ring, dash) =
                (brush(BG), brush(BORDER), brush(ACCENT), brush(SECONDARY), brush(FG), brush(MUTED), brush(RING), brush(DASH));
            let (Some(_bg), Some(border), Some(accent), Some(secondary), Some(fg), Some(muted), Some(ring), Some(dash)) =
                (bg, border, accent, secondary, fg, muted, ring, dash)
            else {
                let _ = rt.EndDraw(None, None);
                return;
            };
            // the light round what's under the mouse (ui::aura)
            let stops = ui::glow_stops(&rt);
            let aura = |r: &R, radius: f32, steps: u32| {
                if let Some(st) = stops.as_ref() {
                    ui::aura(&rt, st, r, radius, steps, s);
                }
            };
            for c in &cards {
                let hovered = match (dragged, self.hover) {
                    (Some(f), _) => f == c.target,
                    (None, Hit::Card(i) | Hit::Close(i)) => i == c.target,
                    (None, Hit::New) => c.target < 0,
                    (None, Hit::None) => false,
                };
                if hovered {
                    rt.FillRoundedRectangle(&rounded(&c.area, 8.0 * s), &accent);
                    match self.hover {
                        Hit::Close(_) if dragged.is_none() => aura(&c.close, 6.0 * s, 5), // the × itself
                        _ => aura(&c.thumb, 6.0 * s, 9),
                    }
                }
                let label = R { x: c.label.x + 2.0 * s, w: c.label.w - if hovered && self.can_close && c.target >= 0 { 26.0 * s } else { 4.0 * s }, ..c.label };
                if c.target < 0 {
                    // New desktop: a dashed tile with a +
                    self.text_line(&rt, "New desktop", &label, &muted, false);
                    let t = c.thumb;
                    rt.DrawRoundedRectangle(&rounded(&t, 6.0 * s), &dash, 1.0 * s, self.dashed.as_ref());
                    let (cx, cy, k) = (t.x + t.w / 2.0, t.y + t.h / 2.0, 8.0 * s);
                    let pen = if hovered { &fg } else { &muted };
                    let _ = rt.DrawLine(D2D_POINT_2F { x: cx - k, y: cy }, D2D_POINT_2F { x: cx + k, y: cy }, pen, 1.5 * s, None);
                    let _ = rt.DrawLine(D2D_POINT_2F { x: cx, y: cy - k }, D2D_POINT_2F { x: cx, y: cy + k }, pen, 1.5 * s, None);
                    continue;
                }
                let current = c.target == self.current;
                if self.renaming != Some(c.target) {
                    let name = self.labels.get(c.target as usize).cloned().unwrap_or_default();
                    self.text_line(&rt, &name, &label, if current { &fg } else { &muted }, current);
                } else {
                    // the input's box (shadcn Input, focused): the text itself is an edit control
                    let r = R { x: c.label.x, y: c.label.y + 2.0 * s, w: c.label.w, h: c.label.h - 4.0 * s };
                    rt.FillRoundedRectangle(&rounded(&r, 6.0 * s), &accent);
                    rt.DrawRoundedRectangle(&rounded(&r, 6.0 * s), &ring, 1.0 * s, None);
                }
                // the miniature: its wallpaper, covering it (the windows come over it, from Windows)
                let t = c.thumb;
                let wall = self.wall_of.get(c.target as usize).cloned().flatten();
                let bitmap = wall.and_then(|p| self.wallpaper(&p, t.w, t.h));
                match bitmap.and_then(|b| rt.CreateBitmapBrush(&b, None, None).ok().map(|br| (b, br))) {
                    Some((b, br)) => {
                        let sz = b.GetSize();
                        let k = (t.w / sz.width).max(t.h / sz.height);
                        let m = Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: t.x + (t.w - sz.width * k) / 2.0, M32: t.y + (t.h - sz.height * k) / 2.0 };
                        br.SetTransform(&m);
                        rt.FillRoundedRectangle(&rounded(&t, 6.0 * s), &br);
                    }
                    None => rt.FillRoundedRectangle(&rounded(&t, 6.0 * s), &accent),
                }
                if current {
                    let ringed = R { x: t.x - 2.0 * s, y: t.y - 2.0 * s, w: t.w + 4.0 * s, h: t.h + 4.0 * s };
                    rt.DrawRoundedRectangle(&rounded(&ringed, 8.0 * s), &ring, 1.5 * s, None);
                } else {
                    rt.DrawRoundedRectangle(&rounded(&t, 6.0 * s), &border, 1.0 * s, None);
                }
                // × (ghost icon button), on the hovered card
                if hovered && self.can_close && self.renaming.is_none() && dragged.is_none() {
                    let b = c.close;
                    let on = self.hover == Hit::Close(c.target);
                    if on {
                        rt.FillRoundedRectangle(&rounded(&b, 6.0 * s), &secondary);
                    }
                    let (cx, cy, k) = (b.x + b.w / 2.0, b.y + b.h / 2.0, 4.0 * s);
                    let pen = if on { &fg } else { &muted };
                    let _ = rt.DrawLine(D2D_POINT_2F { x: cx - k, y: cy - k }, D2D_POINT_2F { x: cx + k, y: cy + k }, pen, 1.5 * s, None);
                    let _ = rt.DrawLine(D2D_POINT_2F { x: cx + k, y: cy - k }, D2D_POINT_2F { x: cx - k, y: cy + k }, pen, 1.5 * s, None);
                }
            }
            if rt.EndDraw(None, None).is_err() {
                self.rt = None; // device lost: made again next time
                self.walls.clear();
            }
        }
    }

    fn text_line(&self, rt: &ID2D1HwndRenderTarget, s: &str, r: &R, brush: &ID2D1SolidColorBrush, bold: bool) {
        let t: Vec<u16> = s.encode_utf16().collect();
        let f = if bold { &self.text_bold } else { &self.text };
        unsafe {
            rt.DrawText(&t, f, &r.d2d(), brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> RECT {
        RECT { left: 0, top: 0, right: 2560, bottom: 1392 } // taskbar at the bottom, 48 px
    }

    #[test]
    fn cards_have_the_screens_shape_and_new_comes_last() {
        let l = layout(3, true, 2560.0, 1440.0, 2300.0, 1.0);
        assert_eq!(l.cards.len(), 4);
        assert_eq!(l.cards[3].target, -1);
        let t = l.cards[0].thumb;
        assert!((t.w / t.h - 2560.0 / 1440.0).abs() < 0.02);
        assert!(l.cards[1].thumb.x > l.cards[0].thumb.x + t.w, "side by side, with a gap");
        assert!(l.w >= l.cards[3].thumb.x + l.cards[3].thumb.w);
    }

    #[test]
    fn many_desktops_shrink_to_fit() {
        let l = layout(12, true, 2560.0, 1440.0, 1000.0, 1.0);
        assert!(l.w <= 1001.0);
        assert!(l.cards[0].thumb.w < 176.0);
    }

    #[test]
    fn hits() {
        let l = layout(2, true, 1920.0, 1080.0, 2000.0, 1.0);
        let c = l.cards[1];
        assert_eq!(hit(&l, c.thumb.x + 5.0, c.thumb.y + 5.0, true), Hit::Card(1));
        assert_eq!(hit(&l, c.close.x + 2.0, c.close.y + 2.0, true), Hit::Close(1));
        assert_eq!(hit(&l, c.close.x + 2.0, c.close.y + 2.0, false), Hit::Card(1), "no × on Windows 10 / one desktop");
        let n = l.cards[2];
        assert_eq!(hit(&l, n.thumb.x + 5.0, n.thumb.y + 5.0, true), Hit::New);
        assert_eq!(hit(&l, 1.0, 1.0, true), Hit::None);
    }

    fn dragging_a_card_makes_room_and_stays_in_the_row() {
        let slot = 100.0;
        // desktop 0 of 4 dragged 2.4 slots right: dropped at 2, cards 1 and 2 shift left
        let (off, to) = drag_offsets(4, 0, 240.0, slot);
        assert_eq!(to, 2);
        assert_eq!(off, vec![240.0, -100.0, -100.0, 0.0]);
        // dragged far left from 3: stops at the first place
        let (off, to) = drag_offsets(4, 3, -900.0, slot);
        assert_eq!(to, 0);
        assert_eq!(off, vec![100.0, 100.0, 100.0, -300.0]);
        // a little: stays where it was, nothing else moves
        let (off, to) = drag_offsets(4, 1, 30.0, slot);
        assert_eq!(to, 1);
        assert_eq!(off, vec![0.0, 30.0, 0.0, 0.0]);
    }

    fn icon(x: i32, y: i32) -> RECT {
        RECT { left: x - 12, top: y - 12, right: x + 12, bottom: y + 12 }
    }

    #[test]
    fn placed_against_the_taskbar_wherever_it_is() {
        let mon = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };
        // bottom taskbar, icon near the right: above it, kept on screen
        let (x, y) = place(600, 160, icon(2500, 1416), work(), mon, 1.0);
        assert_eq!((x, y), (2560 - 600 - 8, 1392 - 160 - 8));
        // top taskbar (work area starts at 48), icon in it: below the taskbar, centred on it
        let top = RECT { left: 0, top: 48, right: 2560, bottom: 1440 };
        let (x, y) = place(600, 160, icon(1200, 24), top, mon, 1.0);
        assert_eq!((x, y), (900, 56));
        // top taskbar, icon in the ^ panel (below the taskbar): below the icon, not in a corner
        let (x, y) = place(600, 160, icon(1700, 90), top, mon, 1.0);
        assert_eq!((x, y), (1400, 102 + 8));
        // right-side taskbar: left of it
        let side = RECT { left: 0, top: 0, right: 2500, bottom: 1440 };
        let (x, y) = place(600, 160, icon(2530, 700), side, mon, 1.0);
        assert_eq!((x, y), (2500 - 600 - 8, 700 - 80));
    }
}
