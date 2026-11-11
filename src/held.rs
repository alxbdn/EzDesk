//! The window in your hand: its border lit while it's held, and putting it back.
//!
//! While a window is held, its own border (DWMWA_BORDER_COLOR, the one the accent colour uses,
//! Windows 11 only) takes the glow's blue, drawn by Windows, glued to the window. It's changed only
//! while the window is still, a few times a second: each change makes Windows redraw the window's
//! frame, which made dragging lag. It's another app's window, so `release` always gives it its
//! usual border back; grab a second window and the first one is released first.

use crate::glow;
use crate::hold::Hwnd;

/// The border changes only after the window has been still this long, and this often at most.
pub const STILL_MS: u64 = 120;
pub const BORDER_MS: u64 = 150;

pub trait WinOps {
    /// DWMWA_BORDER_COLOR works here (Windows 11).
    fn can_border(&self) -> bool;
    /// Where it is, or None when it's gone.
    fn pos(&self, hwnd: Hwnd) -> Option<(i32, i32, i32, i32)>;
    /// 0xRRGGBB, or None for Windows' default.
    fn border(&self, hwnd: Hwnd, rgb: Option<u32>);
}

pub struct HeldWindow<O: WinOps> {
    pub ops: O,
    pub hwnd: Hwnd,
    lit: bool,
    color: Option<u32>,
    geo: Option<(i32, i32, i32, i32)>,
    still_at: u64,
    border_at: u64,
}

impl<O: WinOps> HeldWindow<O> {
    pub fn new(ops: O) -> Self {
        HeldWindow { ops, hwnd: 0, lit: false, color: None, geo: None, still_at: 0, border_at: 0 }
    }

    pub fn grab(&mut self, hwnd: Hwnd, now: u64) {
        if hwnd == 0 || hwnd == self.hwnd {
            return;
        }
        self.release();
        self.hwnd = hwnd;
        self.lit = self.ops.can_border();
        self.still_at = now;
    }

    /// Every frame: one colour at a time round the glow's palette, only while it's still.
    pub fn frame(&mut self, now: u64) {
        if self.hwnd == 0 || !self.lit {
            return;
        }
        let Some(pos) = self.ops.pos(self.hwnd) else { return };
        if Some(pos) != self.geo {
            self.geo = Some(pos); // moving: leave it alone
            self.still_at = now;
        }
        if now.saturating_sub(self.still_at) < STILL_MS || now.saturating_sub(self.border_at) < BORDER_MS {
            return;
        }
        self.border_at = now;
        let rgb = glow::color_at((now % 4000) as f64, 4000.0, 0.0);
        if Some(rgb) != self.color {
            self.color = Some(rgb);
            self.ops.border(self.hwnd, Some(rgb));
        }
    }

    /// Its usual border again. Safe any time.
    pub fn release(&mut self) {
        if self.hwnd != 0 && self.color.is_some() {
            self.ops.border(self.hwnd, None);
        }
        self.hwnd = 0;
        self.lit = false;
        self.color = None;
        self.geo = None;
        self.border_at = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Fake {
        can: bool,
        places: RefCell<HashMap<Hwnd, (i32, i32, i32, i32)>>,
        borders: RefCell<HashMap<Hwnd, Option<u32>>>,
        calls: RefCell<usize>,
    }
    fn fake(can: bool) -> Fake {
        Fake { can, places: RefCell::new(HashMap::new()), borders: RefCell::new(HashMap::new()), calls: RefCell::new(0) }
    }
    impl WinOps for Fake {
        fn can_border(&self) -> bool { self.can }
        fn pos(&self, hwnd: Hwnd) -> Option<(i32, i32, i32, i32)> {
            Some(*self.places.borrow().get(&hwnd).unwrap_or(&(0, 0, 800, 600)))
        }
        fn border(&self, hwnd: Hwnd, rgb: Option<u32>) {
            *self.calls.borrow_mut() += 1;
            self.borders.borrow_mut().insert(hwnd, rgb);
        }
    }

    #[test]
    fn lit_while_still_and_back_on_release() {
        let mut w = HeldWindow::new(fake(true));
        w.grab(5, 0);
        w.frame(200);
        w.frame(400);
        assert!(matches!(w.ops.borders.borrow().get(&5), Some(Some(_))));
        w.release();
        assert_eq!(w.ops.borders.borrow().get(&5), Some(&None));
        let n = *w.ops.calls.borrow();
        w.release();
        assert_eq!(*w.ops.calls.borrow(), n, "twice: nothing more");
    }

    #[test]
    fn not_while_moving() {
        let mut w = HeldWindow::new(fake(true));
        w.grab(5, 0);
        for i in 1..10 {
            w.ops.places.borrow_mut().insert(5, (i, 0, 800, 600));
            w.frame(i as u64 * 100);
        }
        assert!(w.ops.borders.borrow().get(&5).is_none());
    }

    #[test]
    fn never_on_windows_10() {
        let mut w = HeldWindow::new(fake(false));
        w.grab(5, 0);
        w.frame(200);
        w.frame(400);
        w.release();
        assert_eq!(*w.ops.calls.borrow(), 0);
    }

    #[test]
    fn another_window_releases_the_first() {
        let mut w = HeldWindow::new(fake(true));
        w.grab(5, 0);
        w.frame(200);
        w.frame(400);
        w.grab(6, 500);
        assert_eq!(w.ops.borders.borrow().get(&5), Some(&None));
        assert_eq!(w.hwnd, 6);
    }
}
