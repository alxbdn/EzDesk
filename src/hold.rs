//! The hold gesture: what holding a window means, from the press to the drop.
//!
//! It's told what happens (a title bar pressed, Windows starting or ending a drag, and every frame:
//! the time, the mouse button, the side bump the cursor is in) and answers with what to do, as a
//! list of intents. It never reads the mouse, a window or the clock itself, and does nothing on
//! screen, so a test can play a whole gesture with made-up times.
//!
//! Phases: idle → pressed (button down on a title bar, Windows hasn't seen it move) → dragging
//! (Windows is moving it) → idle. A drag can also start without a press (the press wasn't seen).

/// Held this long, moving or not: the side bumps come up.
pub const OPEN_MS: u64 = 300;
/// In a side bump this long: switch (macOS's workspaces-edge-delay).
pub const EDGE_MS: u64 = 750;

pub type Hwnd = isize;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Held long enough: bring up the bumps on the screen at (x, y), the window lit.
    Open { hwnd: Hwnd, x: i32, y: i32 },
    /// A drag started: pin it, so it comes along through a switch.
    Carry(Hwnd),
    /// Let go without ever moving: the bumps go.
    Close,
    /// Held in a side bump: one desktop that way (past the last: a new one).
    Step(i32),
    /// The drag ended: the window goes back on the desktop you're on.
    Drop { hwnd: Hwnd, carried: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Idle,
    Pressed,
    Dragging,
}

/// One frame, as the app sees it.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub now: u64,
    /// The left button is down.
    pub down: bool,
    /// The bumps are on screen.
    pub open: bool,
    /// The side bump the cursor is in: -1, +1, or 0.
    pub edge: i32,
}

#[derive(Debug)]
pub struct HoldGesture {
    pub phase: Phase,
    pub hwnd: Hwnd,
    x: i32,
    y: i32,
    since: u64,
    carried: bool,
    edge_side: i32,
    edge_since: u64,
}

impl Default for HoldGesture {
    fn default() -> Self {
        Self::new()
    }
}

impl HoldGesture {
    pub fn new() -> Self {
        HoldGesture { phase: Phase::Idle, hwnd: 0, x: 0, y: 0, since: 0, carried: false, edge_side: 0, edge_since: 0 }
    }

    fn clear(&mut self) {
        *self = Self::new();
    }

    /// The window being dragged by Windows right now, or 0.
    pub fn dragging(&self) -> Hwnd {
        if self.phase == Phase::Dragging { self.hwnd } else { 0 }
    }

    /// A title bar pressed.
    pub fn press(&mut self, hwnd: Hwnd, x: i32, y: i32, now: u64) -> Vec<Intent> {
        if self.phase != Phase::Idle {
            return vec![];
        }
        self.clear();
        self.phase = Phase::Pressed;
        self.hwnd = hwnd;
        self.x = x;
        self.y = y;
        self.since = now;
        vec![]
    }

    /// Windows started moving a window (it only says so once the mouse has moved a few pixels).
    pub fn drag_start(&mut self, hwnd: Hwnd, x: i32, y: i32, now: u64) -> Vec<Intent> {
        if self.phase == Phase::Dragging {
            return vec![];
        }
        if !(self.phase == Phase::Pressed && hwnd == self.hwnd) {
            // a press already counts from when it was picked up
            self.x = x;
            self.y = y;
            self.since = now;
        }
        self.phase = Phase::Dragging;
        self.hwnd = hwnd;
        self.carried = true;
        self.edge_side = 0;
        self.edge_since = 0;
        vec![Intent::Carry(hwnd)]
    }

    /// Windows says the drag of hwnd ended: the mouse was released.
    pub fn drag_end(&mut self, hwnd: Hwnd) -> Vec<Intent> {
        if self.phase != Phase::Dragging || hwnd != self.hwnd {
            return vec![];
        }
        let out = vec![Intent::Drop { hwnd, carried: self.carried }];
        self.clear();
        out
    }

    /// Every frame.
    pub fn tick(&mut self, f: Frame) -> Vec<Intent> {
        match self.phase {
            Phase::Idle => return vec![],
            Phase::Pressed if !f.down => {
                self.clear(); // let go without ever moving
                return vec![Intent::Close];
            }
            Phase::Dragging if !f.down => return vec![], // released: Windows' end of the drag is on its way
            _ => {}
        }
        let mut out = vec![];
        if !f.open && f.now.saturating_sub(self.since) >= OPEN_MS {
            out.push(Intent::Open { hwnd: self.hwnd, x: self.x, y: self.y });
        }
        if self.phase != Phase::Dragging || !f.open {
            return out;
        }
        // In a side bump: the cursor has to stay there the whole time, leaving starts the count again.
        if f.edge != self.edge_side {
            self.edge_side = f.edge;
            self.edge_since = f.now;
        }
        if f.edge != 0 && f.now.saturating_sub(self.edge_since) >= EDGE_MS {
            out.push(Intent::Step(f.edge));
            self.edge_since = f.now; // still held there: the next one 0.75 s later
        }
        out
    }

    /// How full of light the bump on that side is (0-1): how long the cursor has been in it.
    pub fn edge_fill(&self, side: i32, now: u64) -> f64 {
        if self.phase != Phase::Dragging || side != self.edge_side || side == 0 {
            return 0.0;
        }
        (now.saturating_sub(self.edge_since) as f64 / EDGE_MS as f64).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(now: u64, down: bool, open: bool, edge: i32) -> Frame {
        Frame { now, down, open, edge }
    }

    #[test]
    fn quick_click_does_nothing() {
        let mut h = HoldGesture::new();
        assert!(h.press(1, 0, 0, 0).is_empty());
        assert!(h.tick(f(100, true, false, 0)).is_empty());
        assert_eq!(h.tick(f(150, false, false, 0)), vec![Intent::Close]);
        assert_eq!(h.phase, Phase::Idle);
    }

    #[test]
    fn held_still_opens_at_300_ms_where_pressed() {
        let mut h = HoldGesture::new();
        h.press(1, 500, 700, 0);
        assert!(h.tick(f(299, true, false, 0)).is_empty());
        assert_eq!(h.tick(f(300, true, false, 0)), vec![Intent::Open { hwnd: 1, x: 500, y: 700 }]);
        assert!(h.tick(f(320, true, true, 0)).is_empty(), "once open, no more");
    }

    #[test]
    fn press_then_drag_keeps_its_clock_and_carries() {
        let mut h = HoldGesture::new();
        h.press(7, 0, 0, 0);
        assert_eq!(h.drag_start(7, 30, 0, 200), vec![Intent::Carry(7)]);
        assert_eq!(h.tick(f(300, true, false, 0)), vec![Intent::Open { hwnd: 7, x: 0, y: 0 }]);
        assert_eq!(h.dragging(), 7);
    }

    #[test]
    fn drag_alone_counts_from_its_start() {
        let mut h = HoldGesture::new();
        h.drag_start(7, 0, 0, 1000);
        assert!(h.tick(f(1299, true, false, 0)).is_empty());
        assert_eq!(h.tick(f(1300, true, false, 0)).len(), 1);
    }

    #[test]
    fn drop_is_carried_and_only_ours() {
        let mut h = HoldGesture::new();
        h.drag_start(7, 0, 0, 0);
        assert!(h.drag_end(8).is_empty(), "another window's drag");
        assert_eq!(h.drag_end(7), vec![Intent::Drop { hwnd: 7, carried: true }]);
        assert_eq!(h.phase, Phase::Idle);
    }

    #[test]
    fn edge_steps_every_750_ms_and_restarts_on_leaving() {
        let mut h = HoldGesture::new();
        h.drag_start(7, 0, 0, 0);
        h.tick(f(1000, true, true, 1));
        assert!((h.edge_fill(1, 1375) - 0.5).abs() < 1e-9);
        assert_eq!(h.edge_fill(-1, 1375), 0.0);
        assert!(h.tick(f(1749, true, true, 1)).is_empty());
        assert_eq!(h.tick(f(1750, true, true, 1)), vec![Intent::Step(1)]);
        assert_eq!(h.tick(f(2500, true, true, 1)), vec![Intent::Step(1)]);
        h.tick(f(2600, true, true, 0));
        h.tick(f(2700, true, true, -1));
        assert!(h.tick(f(3449, true, true, -1)).is_empty());
        assert_eq!(h.tick(f(3450, true, true, -1)), vec![Intent::Step(-1)]);
    }

    #[test]
    fn only_a_dragged_window_steps() {
        let mut h = HoldGesture::new();
        h.press(1, 0, 0, 0);
        h.tick(f(400, true, true, 1));
        assert!(h.tick(f(2000, true, true, 1)).is_empty());
        assert_eq!(h.edge_fill(1, 2000), 0.0);
    }

    #[test]
    fn released_mid_drag_waits_for_windows() {
        let mut h = HoldGesture::new();
        h.drag_start(7, 0, 0, 0);
        assert!(h.tick(f(2000, false, true, 0)).is_empty());
        assert_eq!(h.phase, Phase::Dragging);
    }

    #[test]
    fn a_second_press_is_ignored() {
        let mut h = HoldGesture::new();
        h.press(1, 0, 0, 0);
        h.press(2, 0, 0, 50);
        assert_eq!(h.hwnd, 1);
    }
}
