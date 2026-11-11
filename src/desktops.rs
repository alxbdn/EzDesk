//! Everything EzDesk asks of Windows' virtual desktops.
//!
//! Callers never call the DLL or send Windows' shortcuts themselves: they ask this module, which
//! picks the way that works on this Windows. Desktops count from 0 here; the user sees them from 1.
//!
//! The raw work is done by an [`Adapter`]: `vda::VdaAdapter` in the app (the VirtualDesktopAccessor
//! DLL, its Windows 11 or Windows 10 build, and Windows' own Ctrl+Win shortcuts, which slide where
//! the DLL's switch is instant), an in-memory fake in the tests.

use crate::hold::Hwnd;

pub trait Adapter {
    /// The DLL crashed on this Windows build, or isn't there: everything is a no-op.
    fn broken(&self) -> bool;
    /// Pin, unpin: not in every build of the DLL.
    fn can_pin(&self) -> bool;
    /// -1 when Windows doesn't answer.
    fn count(&self) -> i32;
    fn current(&self) -> i32;
    /// None when it has no name of its own (or the build can't tell).
    fn name(&self, i: i32) -> Option<String>;
    fn move_window(&self, hwnd: Hwnd, i: i32);
    fn pin(&self, hwnd: Hwnd) -> bool;
    fn unpin(&self, hwnd: Hwnd);
    fn slide_left(&self);
    fn slide_right(&self);
    /// Makes a desktop at the end and slides there.
    fn slide_new(&self);
    /// Windows posts msg to hwnd on every switch.
    fn listen(&self, hwnd: Hwnd, msg: u32);
}

pub struct Desktops<A: Adapter> {
    pub a: A,
}

impl<A: Adapter> Desktops<A> {
    pub fn new(adapter: A) -> Self {
        Desktops { a: adapter }
    }

    /// A window can be taken along a switch.
    pub fn can_carry(&self) -> bool {
        !self.a.broken() && self.a.can_pin()
    }

    pub fn broken(&self) -> bool {
        self.a.broken()
    }

    pub fn count(&self) -> i32 {
        self.a.count()
    }

    pub fn current(&self) -> i32 {
        self.a.current()
    }

    /// Its own name, if it has one.
    pub fn name(&self, i: i32) -> Option<String> {
        self.a.name(i).filter(|n| !n.is_empty())
    }

    pub fn listen(&self, hwnd: Hwnd, msg: u32) {
        self.a.listen(hwnd, msg)
    }

    /// On every desktop, so it comes along through a switch; true if it worked.
    pub fn pin(&self, hwnd: Hwnd) -> bool {
        self.can_carry() && self.a.pin(hwnd)
    }

    /// A window let go after it was carried: it stays on the desktop you're on. Moved there first,
    /// then unpinned, so it never vanishes in between.
    pub fn drop_here(&self, hwnd: Hwnd, carried: bool) {
        if !carried {
            return;
        }
        let cur = self.current();
        if cur >= 0 {
            self.a.move_window(hwnd, cur);
        }
        self.a.unpin(hwnd);
    }

    /// One desktop left (side -1) or right (+1), with Windows' own slide. or_new: past the last
    /// one, make a new one and go there. Returns true if it switched.
    pub fn slide(&self, side: i32, or_new: bool) -> bool {
        let (cur, count) = (self.current(), self.count());
        if cur < 0 || count <= 0 {
            return false;
        }
        let i = cur + side;
        if i >= 0 && i < count {
            if side < 0 { self.a.slide_left() } else { self.a.slide_right() }
            return true;
        }
        if or_new && side > 0 {
            self.a.slide_new();
            return true;
        }
        false
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Desktops in memory; every call written to `log`, so a test can check what was done, and in
    /// what order. Switches happen at once.
    pub struct FakeDesktops {
        pub n: RefCell<i32>,
        pub cur: RefCell<i32>,
        pub pinned: RefCell<HashMap<Hwnd, bool>>,
        pub names: HashMap<i32, String>,
        pub log: RefCell<Vec<String>>,
        pub can_pin: bool,
    }

    impl FakeDesktops {
        pub fn new(n: i32, cur: i32) -> Self {
            FakeDesktops {
                n: RefCell::new(n),
                cur: RefCell::new(cur),
                pinned: RefCell::new(HashMap::new()),
                names: HashMap::new(),
                log: RefCell::new(vec![]),
                can_pin: true,
            }
        }
        pub fn says(&self) -> String {
            self.log.borrow().join(", ")
        }
    }

    impl Adapter for FakeDesktops {
        fn broken(&self) -> bool { false }
        fn can_pin(&self) -> bool { self.can_pin }
        fn count(&self) -> i32 { *self.n.borrow() }
        fn current(&self) -> i32 { *self.cur.borrow() }
        fn name(&self, i: i32) -> Option<String> { self.names.get(&i).cloned() }
        fn move_window(&self, hwnd: Hwnd, i: i32) { self.log.borrow_mut().push(format!("move {hwnd} {i}")) }
        fn pin(&self, hwnd: Hwnd) -> bool {
            self.log.borrow_mut().push(format!("pin {hwnd}"));
            self.pinned.borrow_mut().insert(hwnd, true);
            true
        }
        fn unpin(&self, hwnd: Hwnd) {
            self.log.borrow_mut().push(format!("unpin {hwnd}"));
            self.pinned.borrow_mut().remove(&hwnd);
        }
        fn slide_left(&self) { self.log.borrow_mut().push("left".into()); *self.cur.borrow_mut() -= 1 }
        fn slide_right(&self) { self.log.borrow_mut().push("right".into()); *self.cur.borrow_mut() += 1 }
        fn slide_new(&self) {
            self.log.borrow_mut().push("new".into());
            *self.n.borrow_mut() += 1;
            *self.cur.borrow_mut() = *self.n.borrow() - 1;
        }
        fn listen(&self, _: Hwnd, _: u32) {}
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeDesktops;
    use super::*;

    #[test]
    fn drop_moves_first_then_unpins() {
        let d = Desktops::new(FakeDesktops::new(3, 1));
        d.drop_here(5, true);
        assert_eq!(d.a.says(), "move 5 1, unpin 5");
    }

    #[test]
    fn drop_not_carried_does_nothing() {
        let d = Desktops::new(FakeDesktops::new(3, 1));
        d.drop_here(5, false);
        assert_eq!(d.a.says(), "");
    }

    #[test]
    fn slide_stays_inside_and_makes_a_new_one_only_when_asked() {
        let d = Desktops::new(FakeDesktops::new(3, 0));
        assert!(!d.slide(-1, false), "left of desktop 1");
        *d.a.cur.borrow_mut() = 2;
        assert!(!d.slide(1, false), "right of the last");
        assert!(d.slide(1, true));
        assert_eq!((d.count(), d.current()), (4, 3));
        assert_eq!(d.a.says(), "new");
        assert!(d.slide(-1, false));
        assert_eq!(d.current(), 2);
    }

    #[test]
    fn names() {
        let mut f = FakeDesktops::new(3, 0);
        f.names.insert(1, "Mail".into());
        f.names.insert(2, String::new());
        let d = Desktops::new(f);
        assert_eq!(d.name(1).as_deref(), Some("Mail"));
        assert_eq!(d.name(2), None, "an empty name is no name");
        assert_eq!(d.name(0), None);
    }

    #[test]
    fn no_pin_no_carry() {
        let mut f = FakeDesktops::new(3, 0);
        f.can_pin = false;
        let d = Desktops::new(f);
        assert!(!d.can_carry());
        assert!(!d.pin(5));
        assert_eq!(d.a.says(), "");
    }
}
