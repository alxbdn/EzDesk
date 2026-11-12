//! EzDesk's menus (the tray icon's right-click, a mini-dock card's), in the same look as the
//! mini-dock: shadcn/ui's dropdown menu in its dark "zinc" theme, a check for the options that are
//! on, greyed items that can't be used, and EzDesk's flowing light round the item under the mouse.
//! Keyboard: ↑ ↓ to move, Enter to choose, Esc to close; clicking anywhere else closes it.
//!
//! `layout`, `hit`, `next` and `place` are pure (and tested); `Menu` owns the window and draws it.

use crate::ui::{self, color, rounded, R, BG, BORDER, DISABLED, FG, SECONDARY};
use crate::win;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// ok: one of Windows' settings, set the right way: a check in the flowing light.
    Item { id: usize, label: String, checked: bool, enabled: bool, bold: bool, ok: bool },
    Sep,
}

impl Entry {
    pub fn item(id: usize, label: &str) -> Entry {
        Entry::Item { id, label: label.to_string(), checked: false, enabled: true, bold: false, ok: false }
    }
    pub fn checked(self, on: bool) -> Entry {
        match self {
            Entry::Item { id, label, enabled, bold, ok, .. } => Entry::Item { id, label, checked: on, enabled, bold, ok },
            e => e,
        }
    }
    pub fn enabled(self, on: bool) -> Entry {
        match self {
            Entry::Item { id, label, checked, bold, ok, .. } => Entry::Item { id, label, checked, enabled: on, bold, ok },
            e => e,
        }
    }
    /// The default action (what a click on the icon does): in semibold, as Windows shows it.
    pub fn bold(self) -> Entry {
        match self {
            Entry::Item { id, label, checked, enabled, ok, .. } => Entry::Item { id, label, checked, enabled, bold: true, ok },
            e => e,
        }
    }
    /// One of Windows' settings: when it's set the right way, a check in the flowing light.
    pub fn ok(self, on: bool) -> Entry {
        match self {
            Entry::Item { id, label, checked, enabled, bold, .. } => Entry::Item { id, label, checked, enabled, bold, ok: on },
            e => e,
        }
    }
    fn usable(&self) -> bool {
        matches!(self, Entry::Item { enabled: true, .. })
    }
}


// ---------------------------------------------------------------------
//  Layout (pure)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    /// One per entry.
    pub rows: Vec<R>,
}

/// Rows top to bottom; label_w: each entry's text width (0 for separators). s: the display scale.
pub fn layout(entries: &[Entry], label_w: &[f32], s: f32) -> Layout {
    let (pad, item_h, sep_h, check, right) = (4.0 * s, 32.0 * s, 9.0 * s, 32.0 * s, 24.0 * s);
    let widest = label_w.iter().copied().fold(0.0, f32::max);
    let w = (check + widest + right + 2.0 * pad).max(200.0 * s).ceil();
    let mut y = pad;
    let rows = entries
        .iter()
        .map(|e| {
            let h = if matches!(e, Entry::Sep) { sep_h } else { item_h };
            let r = R { x: pad, y, w: w - 2.0 * pad, h };
            y += h;
            r
        })
        .collect();
    Layout { w, h: (y + pad).ceil(), rows }
}

/// The usable item at (x, y), if any.
pub fn hit(l: &Layout, entries: &[Entry], x: f32, y: f32) -> Option<usize> {
    l.rows.iter().position(|r| r.contains(x, y)).filter(|&i| entries[i].usable())
}

/// The next usable item down (dir 1) or up (-1) from `from`, going round.
pub fn next(entries: &[Entry], from: Option<usize>, dir: i32) -> Option<usize> {
    let n = entries.len() as i32;
    let mut i = from.map_or(if dir > 0 { -1 } else { n }, |f| f as i32);
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if entries[i as usize].usable() {
            return Some(i as usize);
        }
    }
    None
}

/// Where a w × h menu opens for a click at (x, y): down and right of it, else flipped so it stays on
/// the screen's work area (above the cursor for a taskbar at the bottom, below for one at the top).
pub fn place(w: i32, h: i32, x: i32, y: i32, work: RECT) -> (i32, i32) {
    let px = if x + w <= work.right { x } else { x - w };
    let py = if y + h <= work.bottom { y } else { y - h };
    (px.max(work.left).min((work.right - w).max(work.left)), py.max(work.top).min((work.bottom - h).max(work.top)))
}

// ---------------------------------------------------------------------
//  The window
// ---------------------------------------------------------------------

pub struct Menu {
    pub hwnd: HWND,
    s: f32,
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    rt: Option<ID2D1HwndRenderTarget>,
    stops: Option<ID2D1GradientStopCollection>,
    text: IDWriteTextFormat,
    text_bold: IDWriteTextFormat,
    pub entries: Vec<Entry>,
    pub layout: Layout,
    pub hover: Option<usize>,
    pub shown: bool,
    /// What it's for: -1 the tray icon, else the mini-dock card of that desktop.
    pub tag: i32,
}

impl Menu {
    /// hwnd: the menu's window, made by the app (it owns the window procedure).
    pub fn new(hwnd: HWND, s: f32) -> Option<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let text = ui::text_format(&dw, 13.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
            let text_bold = ui::text_format(&dw, 13.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            ui::style_window(hwnd);
            Some(Menu { hwnd, s, d2d, dw, rt: None, stops: None, text, text_bold, entries: vec![], layout: Layout::default(), hover: None, shown: false, tag: -1 })
        }
    }

    fn width_of(&self, s: &str, bold: bool) -> f32 {
        let t: Vec<u16> = s.encode_utf16().collect();
        let f = if bold { &self.text_bold } else { &self.text };
        unsafe {
            self.dw.CreateTextLayout(&t, f, 10_000.0, 100.0).ok().and_then(|l| {
                let mut m = DWRITE_TEXT_METRICS::default();
                l.GetMetrics(&mut m).ok().map(|_| m.widthIncludingTrailingWhitespace)
            })
        }
        .unwrap_or(s.len() as f32 * 7.0 * self.s)
    }

    /// Opens it at the cursor (x, y), in front, with the keyboard.
    pub fn open(&mut self, entries: Vec<Entry>, x: i32, y: i32, tag: i32) {
        let widths: Vec<f32> = entries
            .iter()
            .map(|e| match e {
                Entry::Item { label, bold, .. } => self.width_of(label, *bold),
                Entry::Sep => 0.0,
            })
            .collect();
        self.layout = layout(&entries, &widths, self.s);
        self.entries = entries;
        self.hover = None;
        self.tag = tag;
        let (w, h) = (self.layout.w as i32, self.layout.h as i32);
        let (px, py) = place(w, h, x, y, win::work_area_at(x, y));
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, px, py, w, h, SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(self.hwnd); // keys come here; clicking elsewhere deactivates it, and it closes
            SetTimer(self.hwnd, 1, 16, None); // the light round the item under the mouse flows on
        }
        self.shown = true;
        self.ensure_target(w as u32, h as u32);
        self.redraw();
    }

    pub fn close(&mut self) {
        if !self.shown {
            return;
        }
        self.shown = false;
        self.hover = None;
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let _ = KillTimer(self.hwnd, 1);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn set_hover(&mut self, h: Option<usize>) {
        if h != self.hover {
            self.hover = h;
            self.redraw();
        }
    }

    /// The id of entry i, if it can be chosen.
    pub fn id_of(&self, i: usize) -> Option<usize> {
        match self.entries.get(i) {
            Some(Entry::Item { id, enabled: true, .. }) => Some(*id),
            _ => None,
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
        self.stops = self.rt.as_ref().and_then(|rt| ui::glow_stops(rt));
    }

    pub fn paint(&mut self) {
        let Some(rt) = self.rt.clone() else { return };
        let s = self.s;
        unsafe {
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            rt.Clear(Some(&color(BG)));
            let brush = |c: u32| rt.CreateSolidColorBrush(&color(c), None).ok();
            if let (Some(fg), Some(dis), Some(hov), Some(line)) = (brush(FG), brush(DISABLED), brush(SECONDARY), brush(BORDER)) {
                for (i, (e, r)) in self.entries.iter().zip(&self.layout.rows).enumerate() {
                    match e {
                        Entry::Sep => {
                            let y = (r.y + r.h / 2.0).round() + 0.5;
                            let _ = rt.DrawLine(D2D_POINT_2F { x: 0.0, y }, D2D_POINT_2F { x: self.layout.w, y }, &line, 1.0, None);
                        }
                        Entry::Item { label, checked, enabled, bold, ok, .. } => {
                            if self.hover == Some(i) {
                                rt.FillRoundedRectangle(&rounded(r, 4.0 * s), &hov);
                                if let Some(st) = &self.stops {
                                    ui::aura(&rt, st, r, 4.0 * s, 6, s);
                                }
                            }
                            let pen = if *enabled { &fg } else { &dis };
                            if *ok {
                                // Windows' setting is right: a check in EzDesk's flowing light
                                if let Some(st) = &self.stops {
                                    ui::glow_check(&rt, st, r.x + 16.0 * s, r.y + r.h / 2.0, 10.0 * s, s);
                                }
                            } else if *checked {
                                // a check, drawn: ✓
                                let (cx, cy) = (r.x + 16.0 * s, r.y + r.h / 2.0);
                                let p = |dx: f32, dy: f32| D2D_POINT_2F { x: cx + dx * s, y: cy + dy * s };
                                let _ = rt.DrawLine(p(-4.5, 0.5), p(-1.5, 3.5), pen, 1.6 * s, None);
                                let _ = rt.DrawLine(p(-1.5, 3.5), p(4.5, -3.5), pen, 1.6 * s, None);
                            }
                            let t = R { x: r.x + 32.0 * s - 4.0 * s, w: r.w - 32.0 * s, ..*r };
                            ui::text(&rt, label, if *bold { &self.text_bold } else { &self.text }, &t, pen);
                        }
                    }
                }
            }
            if rt.EndDraw(None, None).is_err() {
                self.rt = None; // device lost: made again next time
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tray() -> Vec<Entry> {
        vec![
            Entry::item(1, "Task View").bold(),
            Entry::Sep,
            Entry::item(2, "Hold a window").checked(true),
            Entry::item(5, "Starts with Windows (admin mode)").checked(true).enabled(false),
            Entry::Sep,
            Entry::item(8, "Exit"),
        ]
    }

    #[test]
    fn rows_stack_and_the_widest_label_fits() {
        let e = tray();
        let l = layout(&e, &[60.0, 0.0, 300.0, 250.0, 0.0, 30.0], 1.0);
        assert_eq!(l.rows.len(), 6);
        assert!(l.rows[1].h < l.rows[0].h, "a separator is thinner");
        assert_eq!(l.rows[2].y, l.rows[1].y + l.rows[1].h);
        assert!(l.w >= 300.0 + 32.0, "room for the check column and the widest label");
        assert!(l.h > l.rows[5].y + l.rows[5].h);
    }

    #[test]
    fn only_usable_items_are_hit_and_reached_by_keys() {
        let e = tray();
        let l = layout(&e, &[60.0; 6], 1.0);
        let mid = |r: &R| (r.x + 5.0, r.y + r.h / 2.0);
        let (x, y) = mid(&l.rows[0]);
        assert_eq!(hit(&l, &e, x, y), Some(0));
        let (x, y) = mid(&l.rows[1]);
        assert_eq!(hit(&l, &e, x, y), None, "separator");
        let (x, y) = mid(&l.rows[3]);
        assert_eq!(hit(&l, &e, x, y), None, "greyed");
        assert_eq!(next(&e, None, 1), Some(0));
        assert_eq!(next(&e, Some(2), 1), Some(5), "skips the greyed one and the separator");
        assert_eq!(next(&e, Some(5), 1), Some(0), "goes round");
        assert_eq!(next(&e, None, -1), Some(5));
    }

    #[test]
    fn opens_down_right_else_flips_to_stay_on_screen() {
        let work = RECT { left: 0, top: 48, right: 1920, bottom: 1080 };
        assert_eq!(place(240, 300, 1000, 60, work), (1000, 60), "taskbar at the top: below the cursor");
        assert_eq!(place(240, 300, 1900, 1000, work), (1660, 700), "bottom right: up and left");
    }
}
