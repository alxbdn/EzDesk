//! The side bumps: where you take a held window to the previous / next desktop.
//!
//! While a window is held, a small glowing bump sits on each side of its screen, half-way down.
//! Bring the window towards a side and that bump swells, smoothly with the distance, from 200 px
//! away. Into the bump (about 20 px from the edge) and hold: it fills with light for 0.75 s, then
//! you slide to that desktop, window in hand, and it gives a little bounce. Windows' Snap still
//! works all along: let go against the edge before the 0.75 s and the window snaps there. On
//! desktop 1 there's no left bump. Past the last desktop, the right one makes a new desktop, and
//! shows a little "+".
//!
//! Shape: a bell (glow::bell_mask), filled, with the glow's haze round it. Its size follows
//! q = closeness (+ bounce): LEVELS masks per side, made once.

use crate::glow::{self, Stamp};
use crate::layered::Layered;
use std::collections::HashMap;
use windows::Win32::Foundation::RECT;

const LEVELS: usize = 15;
const QMAX: f64 = 1.4;
const GLOW: f64 = 10.0; // the haze's width, in pixels at 100 %
const REACH: f64 = 200.0; // from how far it starts to swell
const RAISE_MS: u64 = 100; // back on top this often: Snap's preview comes up over everything

pub struct Bumps {
    win: [Layered; 2], // [left, right]
    s: f64,
    mg: f64,
    core: f64,
    w: usize,
    h: usize,
    masks: HashMap<(usize, i32), Vec<u8>>,
    plus: Vec<u32>,
    plus_size: usize,
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

impl Bumps {
    pub fn new(scale: f64) -> Self {
        let s = scale;
        let mg = (GLOW * s).round();
        let w = (depth(QMAX, s).ceil() + mg + 2.0) as usize;
        let h = (2.0 * (half(1.0, s).ceil() + mg)) as usize;
        // the "+": dark, opaque, thin
        let size = (12.0 * s).round().max(3.0) as usize;
        let t = (1.6 * s).round().max(1.0) as usize;
        let o = (size - t) / 2;
        let mut plus = vec![0u32; size * size];
        for r in o..o + t {
            for c in 0..size {
                plus[r * size + c] = 0xFF16_171C;
                plus[c * size + r] = 0xFF16_171C;
            }
        }
        Bumps {
            win: [Layered::new(w, h), Layered::new(w, h)],
            s,
            mg,
            core: (3.0 * s).round().max(2.0),
            w,
            h,
            masks: HashMap::new(),
            plus,
            plus_size: size,
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

    /// On the screen whose work area is `work`. first: on desktop 1 (no left bump).
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

    /// Makes the masks ahead of time, so the first frames are quick.
    pub fn warm(&mut self) {
        for k in 0..LEVELS {
            for side in [-1, 1] {
                self.mask(k, side);
            }
        }
    }

    fn mask(&mut self, k: usize, side: i32) -> &Vec<u8> {
        let (w, h, s, mg, core) = (self.w, self.h, self.s, self.mg, self.core);
        self.masks.entry((k, side)).or_insert_with(|| {
            let q = k as f64 / (LEVELS - 1) as f64 * QMAX;
            glow::bell_mask(w, h, depth(q, s), half(q, s), mg, core, side)
        })
    }

    /// One frame of both bumps, cursor at (x, y). cur / count: the desktop you're on, and how
    /// many. fill(side): how full of light that one is. Returns the side the cursor is in (-1 left,
    /// +1 right, 0 neither).
    pub fn frame(&mut self, x: i32, now: u64, cur: i32, count: i32, fill: impl Fn(i32) -> f64) -> i32 {
        if !self.shown {
            return 0;
        }
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
            let q = ((p + 0.4 * self.squash[i]) * grow).clamp(0.0, QMAX);
            if self.avail[i] > 0.5 && dist >= -1.0 && dist <= depth(q, self.s) + 2.0 * self.s {
                contact = side;
            }
            let alpha = (255.0 * (0.45 + 0.55 * p) * (grow.max(0.0) * 1.5).min(1.0)).round();
            if alpha < 3.0 {
                if self.win[i].visible {
                    self.win[i].hide();
                }
                continue;
            }
            let plus = if side > 0 && cur + 1 >= count { ((q - 0.45) / 0.4).clamp(0.0, 1.0) } else { 0.0 }; // last desktop: a new one
            let k = (q / QMAX * (LEVELS - 1) as f64).round() as usize;
            let mask = self.mask(k, side).clone();
            let stamp = (plus > 0.0).then(|| {
                let cx = if side > 0 { self.w as f64 - depth(q, self.s) * 0.55 } else { depth(q, self.s) * 0.55 };
                Stamp {
                    px: &self.plus,
                    size: self.plus_size,
                    x: (cx - self.plus_size as f64 / 2.0).round() as i32,
                    y: ((self.h - self.plus_size) / 2) as i32,
                    alpha: plus,
                }
            });
            let (w, h) = (self.w, self.h);
            let mut buf = vec![0u32; w * h];
            glow::compose(&mut buf, &mask, w, h, now, 2, fill(side), stamp.as_ref());
            let win = &mut self.win[i];
            win.pixels().copy_from_slice(&buf);
            let wx = if side < 0 { wl } else { wr - w as i32 };
            win.put(wx, (yc - h as f64 / 2.0).round() as i32, alpha.min(255.0) as u8);
            if !win.visible || now.saturating_sub(win.raised_at) > RAISE_MS {
                win.raise(now);
            }
        }
        contact
    }
}

/// How far it sticks out.
fn depth(q: f64, s: f64) -> f64 {
    (5.0 + 15.0 * q * q) * s
}

/// Half its length along the edge.
fn half(q: f64, s: f64) -> f64 {
    (120.0 + 85.0 * q.min(1.0)) * s
}
