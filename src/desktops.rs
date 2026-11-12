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
    /// Straight to desktop i, no slide.
    fn go_to(&self, i: i32);
    /// Windows posts msg to hwnd on every switch.
    fn listen(&self, hwnd: Hwnd, msg: u32);
    /// The desktop a window is on (-1: can't tell).
    fn window_desktop(&self, hwnd: Hwnd) -> i32;
    /// Make, close and rename desktops without going there: Windows 11 only.
    fn can_manage(&self) -> bool;
    /// One more at the end, without going there.
    fn create(&self);
    /// Closes desktop i; its windows go to desktop fallback.
    fn remove(&self, i: i32, fallback: i32);
    fn set_name(&self, i: i32, name: &str);
    /// Its wallpaper's file, when the adapter knows it.
    fn wallpaper(&self, i: i32) -> Option<String>;
    /// Desktops can be reordered (Windows 11 24H2+, compiled in).
    fn can_reorder(&self) -> bool;
    /// Desktop i moves to position to; the others shift over.
    fn move_desktop(&self, i: i32, to: i32);
}

/// Either adapter, picked at start-up by Windows build.
impl Adapter for Box<dyn Adapter> {
    fn broken(&self) -> bool { (**self).broken() }
    fn can_pin(&self) -> bool { (**self).can_pin() }
    fn count(&self) -> i32 { (**self).count() }
    fn current(&self) -> i32 { (**self).current() }
    fn name(&self, i: i32) -> Option<String> { (**self).name(i) }
    fn move_window(&self, hwnd: Hwnd, i: i32) { (**self).move_window(hwnd, i) }
    fn pin(&self, hwnd: Hwnd) -> bool { (**self).pin(hwnd) }
    fn unpin(&self, hwnd: Hwnd) { (**self).unpin(hwnd) }
    fn slide_left(&self) { (**self).slide_left() }
    fn slide_right(&self) { (**self).slide_right() }
    fn slide_new(&self) { (**self).slide_new() }
    fn go_to(&self, i: i32) { (**self).go_to(i) }
    fn listen(&self, hwnd: Hwnd, msg: u32) { (**self).listen(hwnd, msg) }
    fn window_desktop(&self, hwnd: Hwnd) -> i32 { (**self).window_desktop(hwnd) }
    fn can_manage(&self) -> bool { (**self).can_manage() }
    fn create(&self) { (**self).create() }
    fn remove(&self, i: i32, fallback: i32) { (**self).remove(i, fallback) }
    fn set_name(&self, i: i32, name: &str) { (**self).set_name(i, name) }
    fn wallpaper(&self, i: i32) -> Option<String> { (**self).wallpaper(i) }
    fn can_reorder(&self) -> bool { (**self).can_reorder() }
    fn move_desktop(&self, i: i32, to: i32) { (**self).move_desktop(i, to) }
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

    /// Straight to desktop i, if there's one.
    pub fn go_to(&self, i: i32) {
        if i >= 0 && i < self.count() {
            self.a.go_to(i);
        }
    }

    pub fn window_desktop(&self, hwnd: Hwnd) -> i32 {
        self.a.window_desktop(hwnd)
    }

    /// Desktops can be made, closed and renamed from EzDesk (Windows 11).
    pub fn can_manage(&self) -> bool {
        !self.a.broken() && self.a.can_manage()
    }

    /// A new desktop at the end, without going there; where it can't be made that way (Windows
    /// 10), Windows makes it and slides there. True if you stayed where you were.
    pub fn create(&self) -> bool {
        if self.can_manage() {
            self.a.create();
            true
        } else {
            self.a.slide_new();
            false
        }
    }

    /// Closes desktop i: its windows go to the one before it (the next one for desktop 1). Never
    /// the last desktop. True if it was closed.
    pub fn remove(&self, i: i32) -> bool {
        let count = self.count();
        if !self.can_manage() || count < 2 || i < 0 || i >= count {
            return false;
        }
        self.a.remove(i, if i > 0 { i - 1 } else { 1 });
        true
    }

    pub fn can_reorder(&self) -> bool {
        !self.a.broken() && self.a.can_reorder()
    }

    /// Desktop i to position to (both 0-based, the others shift over). True if it moved.
    pub fn move_desktop(&self, i: i32, to: i32) -> bool {
        let count = self.count();
        if !self.can_reorder() || i == to || i < 0 || to < 0 || i >= count || to >= count {
            return false;
        }
        self.a.move_desktop(i, to);
        true
    }

    /// Renames desktop i (an empty name gives it back its "Desktop n").
    pub fn rename(&self, i: i32, name: &str) {
        if self.can_manage() && i >= 0 && i < self.count() {
            self.a.set_name(i, name.trim());
        }
    }

    /// "Desktop 2", or its own name.
    pub fn label(&self, i: i32) -> String {
        self.name(i).unwrap_or_else(|| format!("Desktop {}", i + 1))
    }

    /// Desktop i's wallpaper file: as the adapter knows it, else as Windows keeps it.
    pub fn wallpaper(&self, i: i32) -> Option<String> {
        self.a.wallpaper(i).filter(|p| !p.is_empty() && std::path::Path::new(p).exists()).or_else(|| crate::system::desktop_wallpaper(i))
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
        pub can_manage: bool,
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
                can_manage: true,
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
        fn go_to(&self, i: i32) {
            self.log.borrow_mut().push(format!("goto {i}"));
            *self.cur.borrow_mut() = i;
        }
        fn listen(&self, _: Hwnd, _: u32) {}
        fn window_desktop(&self, _: Hwnd) -> i32 { 0 }
        fn can_manage(&self) -> bool { self.can_manage }
        fn create(&self) {
            self.log.borrow_mut().push("create".into());
            *self.n.borrow_mut() += 1;
        }
        fn remove(&self, i: i32, fallback: i32) {
            self.log.borrow_mut().push(format!("remove {i} {fallback}"));
            *self.n.borrow_mut() -= 1;
        }
        fn set_name(&self, i: i32, name: &str) { self.log.borrow_mut().push(format!("name {i} {name}")) }
        fn wallpaper(&self, _: i32) -> Option<String> { None }
        fn can_reorder(&self) -> bool { self.can_manage }
        fn move_desktop(&self, i: i32, to: i32) { self.log.borrow_mut().push(format!("move desktop {i} {to}")) }
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
    fn go_to_only_existing_desktops() {
        let d = Desktops::new(FakeDesktops::new(3, 0));
        d.go_to(2);
        d.go_to(3);
        d.go_to(-1);
        assert_eq!(d.a.says(), "goto 2");
    }

    #[test]
    fn remove_never_the_last_and_windows_go_next_door() {
        let d = Desktops::new(FakeDesktops::new(3, 0));
        assert!(d.remove(0));
        assert!(d.remove(1));
        assert!(!d.remove(0), "the last one");
        assert_eq!(d.a.says(), "remove 0 1, remove 1 0");
    }

    #[test]
    fn windows_10_makes_and_goes_but_never_closes() {
        let mut f = FakeDesktops::new(2, 0);
        f.can_manage = false;
        let d = Desktops::new(f);
        assert!(!d.create(), "made by sliding there");
        assert!(!d.remove(1));
        d.rename(0, "Mail");
        assert_eq!(d.a.says(), "new");
    }

    #[test]
    fn reorder_only_real_moves() {
        let d = Desktops::new(FakeDesktops::new(3, 0));
        assert!(d.move_desktop(0, 2));
        assert!(!d.move_desktop(1, 1), "same place");
        assert!(!d.move_desktop(1, 3), "no position 4");
        assert_eq!(d.a.says(), "move desktop 0 2");
        let mut f = FakeDesktops::new(3, 0);
        f.can_manage = false;
        assert!(!Desktops::new(f).move_desktop(0, 2), "not with the DLL");
    }

    #[test]
    fn create_stays_and_rename_trims() {
        let d = Desktops::new(FakeDesktops::new(2, 0));
        assert!(d.create());
        d.rename(1, "  Mail ");
        d.rename(5, "nope");
        assert_eq!(d.a.says(), "create, name 1 Mail");
        assert_eq!(d.label(0), "Desktop 1");
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
