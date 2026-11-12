//! EzDesk: move windows between Windows' virtual desktops with the mouse, and see which desktop
//! you're on.
//!
//!   Hold a window        by its title bar for a moment: a glowing bump rises on each side of the
//!                        screen; bring the window into one and hold it there to slide to the
//!                        previous / next desktop with it (past the last: a new one)
//!   Scroll on taskbar    previous / next desktop
//!   Tray icon            the current desktop's number; click: Task View
//!
//! Modules: `desktops` (Windows' virtual desktops, behind one seam; `vda` is its adapter), `hold`
//! (the gesture, from the press to the drop, as intents), `held` (the window in hand), `glow` and
//! `bumps` (the light), `tray` and `badge` (the number), `hooks` (the mouse), `system` (settings,
//! startup, taskbar).
//! This file wires them to the main window's messages.

#![windows_subsystem = "windows"]

mod badge;
mod bumps;
mod coach;
mod desktops;
mod glow;
mod held;
mod hold;
mod hooks;
mod layered;
mod menu;
mod minidock;
mod system;
mod tray;
mod ui;
mod native;
mod vda;
mod win;

use bumps::Bumps;
use menu::Entry;
use minidock::{Dock, DockData, Hit};
use desktops::Desktops;
use held::{HeldWindow, WinOps};
use hold::{Frame, HoldGesture, Intent, Phase};
use std::cell::RefCell;
use std::path::PathBuf;
use system::Settings;
use tray::Tray;
use desktops::Adapter;
use native::NativeAdapter;
use vda::VdaAdapter;
use win::{pcw, wide};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_LWIN, VK_RBUTTON, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::*;

const MSG_DESKTOP_CHANGED: u32 = 0x1400 + 30; // the DLL posts it on every switch
const WM_DROP: u32 = WM_APP + 10;

const T_TRAY: usize = 1;
const T_HOLD: usize = 2;
const T_BADGE: usize = 3;
const T_CHECK_TRAY: usize = 4;
const T_WELCOME: usize = 5;
const T_OFFER: usize = 6;
const T_WARM: usize = 7;
const T_DOCK: usize = 8;
const T_DOCK_REFRESH: usize = 9;
const T_COACH: usize = 10;

// The mini-dock's right-click menu
const D_RENAME: usize = 101;
const D_BACKGROUND: usize = 102;
const D_CLOSE: usize = 103;
const WM_RENAME_DONE: u32 = WM_APP + 11; // wParam 1: keep the new name, 0: drop it
const WM_MOUSELEAVE_: u32 = 0x02A3; // WM_MOUSELEAVE (not in the bindings this module imports)
const WM_CLOSE_MENU: u32 = WM_APP + 12;

// Tray menu
const M_TASK_VIEW: usize = 1;
const M_HOLD: usize = 2;
const M_TRAY_ICON: usize = 3;
const M_TASKBAR: usize = 4;
const M_STARTUP: usize = 5;
const M_HELP: usize = 6;
const M_RESTART: usize = 7;
const M_EXIT: usize = 8;

const HELP_URL: &str = "https://github.com/alxbdn/ezdesk#readme";

/// What a notification does when it's clicked, or closed without a click.
#[derive(Clone, Copy, PartialEq)]
enum Note {
    ShowAllApps,
    ShowTrayIcon,
    TrayReminderIgnored,
}

/// The window in hand's border, for the HeldWindow module: DWMWA_BORDER_COLOR (34), Windows 11.
struct Dwm {
    can: bool,
}

impl WinOps for Dwm {
    fn can_border(&self) -> bool {
        self.can
    }
    fn pos(&self, h: isize) -> Option<(i32, i32, i32, i32)> {
        let mut r = windows::Win32::Foundation::RECT::default();
        unsafe { GetWindowRect(win::hwnd(h), &mut r) }.ok().map(|_| (r.left, r.top, r.right, r.bottom))
    }
    fn border(&self, h: isize, rgb: Option<u32>) {
        let colorref: u32 = match rgb {
            None => 0xFFFF_FFFF, // DWMWA_COLOR_DEFAULT
            Some(c) => ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF),
        };
        unsafe {
            let _ = DwmSetWindowAttribute(win::hwnd(h), DWMWINDOWATTRIBUTE(34), &colorref as *const u32 as *const _, 4);
        }
    }
}

struct App {
    hwnd: HWND,
    desk: Desktops<Box<dyn Adapter>>,
    hold: HoldGesture,
    held: HeldWindow<Dwm>,
    bumps: Bumps,
    bumps_pinned: bool,
    tray: Tray,
    settings: Settings,
    hold_on: bool,
    tray_reminder: bool,
    build: u32,
    packaged: bool,
    admin: bool,
    swapped: bool,
    shown: (i32, i32), // the desktop and count the icon shows
    connected: bool,
    failures: u32,
    warned: bool,
    last_step: u64,
    note: Option<(Note, Option<Note>)>,
    badge: Option<badge::Badge>, // the tray number, drawn live (None: the .ico files)
    badge_busy: bool,
    taskbar_created: u32,
    dock: Option<Dock>,
    dock_anchor: RECT, // the tray icon, when the dock opened
    dock_left: u64, // when the mouse left the icon and the dock (0: it's on them)
    dock_menu: bool,
    edit: HWND, // the mini-dock's rename field
    edit_brush: windows::Win32::Graphics::Gdi::HBRUSH,
    menu: Option<menu::Menu>,
    coach: Option<coach::Coach>, // lighting up an option in Settings
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs f on the app, unless it's busy (a message arriving while another is being handled).
fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

impl App {
    fn icon_path(&self, n: i32) -> PathBuf {
        win::exe_dir().join("icons").join(if (1..=9).contains(&n) { format!("{n}.ico") } else { "more.ico".into() })
    }

    fn timer(&self, id: usize, ms: u32) {
        unsafe {
            SetTimer(self.hwnd, id, ms, None);
        }
    }

    fn stop(&self, id: usize) {
        unsafe {
            let _ = KillTimer(self.hwnd, id);
        }
    }

    fn notify(&mut self, title: &str, text: &str, action: Note, ignored: Option<Note>) {
        self.note = Some((action, ignored));
        self.tray.notify(title, text);
    }

    // ---- The tray number: connects to the DLL from a timer, retrying with back-off ----

    fn update_tray(&mut self) {
        if self.desk.broken() {
            self.stop(T_TRAY); // incompatible DLL: stop polling, the menu keeps working
            self.tray.set_rich(false); // no mini-dock: the plain tooltip says why
            self.tray.set_tip(&format!("EzDesk: not compatible with this Windows (build {})", self.build));
            return;
        }
        let (i, c) = (self.desk.current(), self.desk.count());
        if i < 0 || c <= 0 {
            // virtual desktops not reachable (yet)
            self.failures += 1;
            self.tray.set_rich(false);
            self.tray.set_tip("EzDesk: waiting for Windows virtual desktops…");
            self.timer(T_TRAY, (1000 * 2u32.pow(self.failures.min(4))).min(10_000)); // back off: 2 s … 10 s
            if self.failures == 6 && !self.warned {
                self.warned = true;
                self.tray.notify("EzDesk", &format!("Can't reach Windows' virtual desktops (Windows build {}).\nTry Restart from this icon's menu.", self.build));
            }
            return;
        }
        if !self.connected {
            // first success: listen for switches, poll as a safety net
            self.connected = true;
            self.failures = 0;
            self.tray.set_rich(true); // hovering the number shows the mini-dock
            self.desk.listen(win::id(self.hwnd), MSG_DESKTOP_CHANGED);
            self.timer(T_TRAY, 1000);
        }
        if (i, c) == self.shown {
            return;
        }
        let switched = self.shown.0 >= 0 && i != self.shown.0; // a real switch, not the first reading
        self.shown = (i, c);
        match self.badge.as_mut() {
            Some(b) => {
                b.set(i + 1, win::now()); // rolls on a switch
                self.badge_tick();
            }
            None => self.tray.set_icon(&self.icon_path(i + 1)),
        }
        if switched {
            self.timer(T_OFFER, 1500); // once the slide is over
        }
        let tip = match self.desk.name(i) {
            Some(n) => format!("Desktop {} of {c} — {n}", i + 1),
            None => format!("Desktop {} of {c}", i + 1),
        };
        self.tray.set_tip(&tip); // read by screen readers; on screen, the mini-dock says it
        if self.dock.as_ref().is_some_and(|d| d.shown) {
            self.dock_refresh();
        }
    }

    // ---- The mini-dock: hovering the number ----

    /// What the mini-dock shows, for the screen at (ax, ay).
    fn dock_data(&self, ax: i32, ay: i32) -> Option<DockData> {
        let (cur, count) = (self.desk.current(), self.desk.count());
        if cur < 0 || count <= 0 {
            return None;
        }
        let dock = self.dock.as_ref().map(|d| d.hwnd).unwrap_or_default();
        let mut windows: Vec<(isize, i32)> = vec![];
        for h in win::top_level_windows().into_iter().rev() {
            // bottom-most first: the ones on top are drawn last, over them
            if h == dock || h == self.hwnd || !win::is_real_window(h) || win::cloaked_by_app(h) {
                continue;
            }
            let d = self.desk.window_desktop(win::id(h));
            if d >= 0 && d < count {
                windows.push((win::id(h), d));
            }
        }
        let can_manage = self.desk.can_manage();
        Some(DockData {
            labels: (0..count).map(|i| self.desk.label(i)).collect(),
            current: cur,
            can_close: can_manage && count > 1,
            can_reorder: self.desk.can_reorder(),
            with_new: true,
            walls: (0..count).map(|i| self.desk.wallpaper(i)).collect(),
            windows,
            screen: win::monitor_at(ax, ay),
        })
    }

    /// Opens it next to the icon: where Windows says the icon is (in the taskbar, or in the ^
    /// panel), else round the cursor, which is on it.
    fn dock_open(&mut self) {
        if !self.connected || self.desk.broken() || self.hold.phase != Phase::Idle {
            return;
        }
        let icon = self.tray.rect().filter(|r| r.right > r.left).unwrap_or_else(|| {
            let (x, y) = win::cursor();
            RECT { left: x - 8, top: y - 8, right: x + 8, bottom: y + 8 }
        });
        let Some(data) = self.dock_data((icon.left + icon.right) / 2, (icon.top + icon.bottom) / 2) else { return };
        self.dock_anchor = icon;
        self.dock_left = 0;
        if let Some(d) = self.dock.as_mut() {
            d.show(data, icon);
            self.timer(T_DOCK, 16);
        }
    }

    fn dock_refresh(&mut self) {
        let icon = self.dock_anchor;
        if let Some(data) = self.dock_data((icon.left + icon.right) / 2, (icon.top + icon.bottom) / 2) {
            if let Some(d) = self.dock.as_mut().filter(|d| d.shown) {
                d.show(data, icon);
            }
        }
    }

    fn dock_hide(&mut self) {
        self.stop(T_DOCK);
        if let Some(d) = self.dock.as_mut() {
            if d.renaming.is_some() {
                unsafe {
                    let _ = ShowWindow(self.edit, SW_HIDE);
                }
            }
            d.hide();
        }
    }

    /// Every 16 ms while it's open: the light round what's under the mouse flows on; and the dock
    /// stays while the mouse is on the icon, the dock, or between them (or while renaming, or its
    /// menu is open), and goes 0.3 s after it left.
    fn dock_tick(&mut self) {
        let Some(d) = self.dock.as_ref() else { return };
        if !d.shown {
            return self.stop(T_DOCK);
        }
        if d.hover != Hit::None || d.drag.is_some_and(|g| g.active) {
            d.redraw(); // only then: nothing moves otherwise
        }
        let now = win::now();
        if d.renaming.is_some() || self.dock_menu || d.drag.is_some() {
            self.dock_left = 0;
            return;
        }
        let (x, y) = win::cursor();
        let m = (12.0 * win::scale()) as i32;
        let mut r = d.screen_rect();
        if let Some(i) = self.tray.rect() {
            r = RECT { left: r.left.min(i.left), top: r.top.min(i.top), right: r.right.max(i.right), bottom: r.bottom.max(i.bottom) };
        }
        let inside = x >= r.left - m && x < r.right + m && y >= r.top - m && y < r.bottom + m;
        if inside {
            self.dock_left = 0;
        } else if self.dock_left == 0 {
            self.dock_left = now;
        } else if now.saturating_sub(self.dock_left) >= 300 {
            self.dock_hide();
        }
    }

    /// A click in the dock: go to that desktop, close it, or make a new one.
    fn dock_click(&mut self, hit: Hit) {
        if self.dock.as_ref().is_some_and(|d| d.renaming.is_some()) {
            return;
        }
        match hit {
            Hit::Card(i) => {
                self.dock_hide();
                self.desk.go_to(i);
            }
            Hit::Close(i) => {
                if self.desk.remove(i) {
                    self.timer(T_DOCK_REFRESH, 250);
                }
            }
            Hit::New => {
                if self.desk.create() {
                    self.timer(T_DOCK_REFRESH, 250); // made without going there: the dock shows it
                } else {
                    self.dock_hide(); // Windows 10: made by going there
                }
            }
            Hit::None => {}
        }
    }

    /// A card dropped at another place: that desktop moves there.
    fn dock_reorder(&mut self, from: i32, to: i32) {
        if self.desk.move_desktop(from, to) {
            self.timer(T_DOCK_REFRESH, 200);
        }
    }

    /// The right-click menu of desktop i's card.
    fn dock_menu_entries(&self, i: i32) -> Vec<Entry> {
        let can_close = self.dock.as_ref().is_some_and(|d| d.can_close);
        vec![
            Entry::item(D_RENAME, "Rename").enabled(self.desk.can_manage()),
            Entry::item(D_BACKGROUND, "Choose background…"),
            Entry::Sep,
            Entry::item(D_CLOSE, "Close desktop").enabled(can_close && i >= 0),
        ]
    }

    /// Opens a menu at the cursor; tag -1: the tray icon's, else the mini-dock card of that desktop.
    fn menu_open(&mut self, entries: Vec<Entry>, tag: i32) {
        let (x, y) = win::cursor();
        self.dock_menu = tag >= 0; // the dock stays open under its card's menu
        if let Some(m) = self.menu.as_mut() {
            m.open(entries, x, y, tag);
        }
    }

    fn menu_close(&mut self) {
        if let Some(m) = self.menu.as_mut() {
            m.close();
        }
        self.dock_menu = false;
    }

    /// Item i of the open menu was chosen.
    fn menu_chosen(&mut self, i: usize) {
        let Some((id, tag)) = self.menu.as_ref().and_then(|m| m.id_of(i).map(|id| (id, m.tag))) else { return };
        self.menu_close();
        if tag < 0 {
            self.command(id);
        } else {
            self.dock_command(id, tag);
        }
    }

    fn dock_command(&mut self, cmd: usize, i: i32) {
        match cmd {
            D_RENAME => self.rename_start(i),
            D_BACKGROUND => {
                self.dock_hide();
                self.desk.go_to(i); // Settings sets the background of the desktop you're on
                system::open("ms-settings:personalization-background");
            }
            D_CLOSE => self.dock_click(Hit::Close(i)),
            _ => {}
        }
    }

    // ---- Renaming, in the card's label (an edit control in shadcn's input look) ----

    fn rename_start(&mut self, i: i32) {
        let s = win::scale() as f32;
        let Some(d) = self.dock.as_mut() else { return };
        let Some(c) = d.layout.cards.iter().find(|c| c.target == i).copied() else { return };
        unsafe {
            if self.edit.is_invalid() {
                let inst = GetModuleHandleW(None).unwrap_or_default();
                self.edit = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    windows::core::w!("EDIT"),
                    None,
                    WS_CHILD | WINDOW_STYLE(0x80), // ES_AUTOHSCROLL
                    0,
                    0,
                    0,
                    0,
                    d.hwnd,
                    None,
                    inst,
                    None,
                )
                .unwrap_or_default();
                let font = windows::Win32::Graphics::Gdi::CreateFontW(
                    -(13.0 * s) as i32, 0, 0, 0, 400, 0, 0, 0, 0, 0, 0, 5, 0, windows::core::w!("Segoe UI Variable Text"),
                ); // CLEARTYPE_QUALITY
                SendMessageW(self.edit, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                let _ = windows::Win32::UI::Shell::SetWindowSubclass(self.edit, Some(edit_proc), 1, 0);
            }
            let r = c.label;
            let _ = SetWindowPos(
                self.edit,
                None,
                (r.x + 8.0 * s) as i32,
                (r.y + 7.0 * s) as i32,
                (r.w - 16.0 * s) as i32,
                (r.h - 14.0 * s) as i32,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
            let name = wide(d.labels.get(i as usize).map(|s| s.as_str()).unwrap_or(""));
            let _ = SetWindowTextW(self.edit, pcw(&name));
            d.renaming = Some(i);
            d.redraw();
            let _ = SetForegroundWindow(d.hwnd);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.edit);
            SendMessageW(self.edit, 0xB1, WPARAM(0), LPARAM(-1)); // EM_SETSEL: all of it
        }
    }

    fn rename_done(&mut self, keep: bool) {
        let Some(i) = self.dock.as_mut().and_then(|d| d.renaming.take()) else { return };
        if keep {
            let mut buf = [0u16; 256];
            let n = unsafe { GetWindowTextW(self.edit, &mut buf) } as usize;
            self.desk.rename(i, &String::from_utf16_lossy(&buf[..n]));
        }
        unsafe {
            let _ = ShowWindow(self.edit, SW_HIDE);
        }
        if let Some(d) = self.dock.as_ref() {
            d.redraw();
        }
        self.timer(T_DOCK_REFRESH, 150); // the new name
    }

    /// The tray number's next frame. It's still, except while it rolls: frames come every
    /// BUSY_MS then, and stop with the last one, the number at rest.
    fn badge_tick(&mut self) {
        let now = win::now();
        let Some(b) = self.badge.as_mut() else { return };
        let busy = b.busy(now);
        if let Some(icon) = b.frame(now) {
            self.tray.set_drawn(icon);
        }
        if busy && !self.badge_busy {
            self.timer(T_BADGE, badge::BUSY_MS);
        } else if !busy && self.badge_busy {
            self.stop(T_BADGE);
        }
        self.badge_busy = busy;
    }

    // ---- Scroll on the taskbar ----

    fn step(&mut self, delta: i32) {
        let now = win::now();
        if now.saturating_sub(self.last_step) < 250 {
            return; // touchpads send bursts of wheel events
        }
        self.last_step = now;
        self.desk.slide(delta, false);
    }

    // ---- Holding a window ----

    /// A button press anywhere: on a title bar (HTCAPTION), it's the start of a hold.
    fn press(&mut self, x: i32, y: i32) {
        if !self.hold_on || !self.desk.can_carry() || self.hold.phase != Phase::Idle {
            return;
        }
        let root = unsafe { GetAncestor(WindowFromPoint(POINT { x, y }), GA_ROOT) };
        if root.is_invalid() || !win::is_real_window(root) || hit_test(root, x, y) != 2 {
            return;
        }
        let i = self.hold.press(win::id(root), x, y, win::now());
        self.perform(i);
        self.timer(T_HOLD, 15);
    }

    fn move_size(&mut self, start: bool, h: isize) {
        if start {
            let w = win::hwnd(h);
            let (x, y) = win::cursor();
            if !self.hold_on || !self.desk.can_carry() || !matches!(hit_test(w, x, y), 1 | 2) {
                return; // a resize by a border, not a drag by the title bar (or a custom one in the client area)
            }
            let i = self.hold.drag_start(h, x, y, win::now());
            self.perform(i);
            self.timer(T_HOLD, 15); // often enough for smooth pulses
        } else {
            let i = self.hold.drag_end(h);
            self.perform(i);
        }
    }

    /// Every 15 ms while a window is in hand: the bumps' frame, then the gesture is told what
    /// happened, then what it answers is done. Drawing comes first, so a desktop switch never
    /// happens in the middle of a frame.
    fn hold_tick(&mut self) {
        if self.hold.phase == Phase::Idle {
            self.stop(T_HOLD);
            return;
        }
        let now = win::now();
        let (x, _) = win::cursor();
        let edge = if self.bumps.shown {
            let (cur, count) = (self.desk.current(), self.desk.count());
            let hold = &self.hold;
            let e = self.bumps.frame(x, now, cur, count, |side| hold.edge_fill(side, now));
            if !self.bumps_pinned {
                // on every desktop, so they stay through a switch
                self.bumps_pinned = self.bumps.hwnds().iter().all(|&h| self.desk.pin(h));
            }
            e
        } else {
            0
        };
        self.held.frame(now);
        let button = if self.swapped { VK_RBUTTON } else { VK_LBUTTON };
        let down = unsafe { GetAsyncKeyState(button.0 as i32) } < 0;
        let i = self.hold.tick(Frame { now, down, open: self.bumps.shown, edge });
        self.perform(i);
    }

    /// Does what the hold gesture says.
    fn perform(&mut self, intents: Vec<Intent>) {
        let now = win::now();
        for it in intents {
            match it {
                Intent::Open { hwnd, x, y } => {
                    self.bumps.open(win::work_area_at(x, y), now, self.desk.current() == 0);
                    self.held.grab(hwnd, now);
                }
                // at the drag's start, not at the switch: a window pinned just before the slide
                // goes off with the old desktop and pops back
                Intent::Carry(h) => {
                    self.desk.pin(h);
                }
                Intent::Close => {
                    self.bumps.hide();
                    self.held.release();
                }
                Intent::Step(side) => {
                    if self.desk.slide(side, true) {
                        self.bumps.bounce(side);
                    }
                }
                Intent::Drop { hwnd, carried } => {
                    self.bumps.hide();
                    self.held.release();
                    unsafe {
                        let _ = PostMessageW(self.hwnd, WM_DROP, WPARAM(carried as usize), LPARAM(hwnd)); // out of Windows' callback first
                    }
                }
            }
        }
    }

    fn drop_window(&mut self, h: isize, carried: bool) {
        self.desk.drop_here(h, carried);
        if carried {
            unsafe {
                let _ = SetForegroundWindow(win::hwnd(h));
            }
        }
    }

    // ---- Notifications ----

    /// The first time you switch desktops while the taskbar only shows the current desktop's
    /// apps, a notification offers to show them all (once, ever).
    fn offer_taskbar_all_desktops(&mut self) {
        if system::taskbar_all_desktops() || self.settings.get("TaskbarOffered", 0) != 0 {
            return;
        }
        self.settings.set("TaskbarOffered", 1);
        self.notify(
            "See all your apps on the taskbar",
            "Apps open on your other desktops don't show on the taskbar. Click here to see them all, wherever you are.",
            Note::ShowAllApps,
            None,
        );
    }

    /// Windows decides whether a tray icon is shown or hidden under ^, and apps can't change it.
    /// If it's hidden, a notification says so; clicked, it opens the Settings page where it's one
    /// switch. Never nagging: it stops for good when "Remind me when this icon is hidden" is
    /// unticked, or after three reminders in a row closed or left without a click.
    fn check_tray_icon(&mut self) {
        if !self.tray_reminder || self.settings.get("TrayIgnored", 0) >= 3 {
            return;
        }
        if system::tray_icon_shown(self.hwnd, tray::ID, self.build) != Some(false) {
            return;
        }
        self.notify(
            "Show EzDesk next to the clock",
            "Your desktop number is hidden under ^ next to the clock. Click here to keep it in view.",
            Note::ShowTrayIcon,
            Some(Note::TrayReminderIgnored),
        );
    }

    fn welcome(&mut self) {
        let shown = system::tray_icon_shown(self.hwnd, tray::ID, self.build);
        if shown == Some(false) {
            return self.check_tray_icon(); // the one thing that matters first: see the number
        }
        let mut tip = String::from("Hold a window by its title bar, then bring it to a side of the screen to take it to another desktop.");
        tip += "\nScroll on the taskbar to go from one desktop to the next.";
        if shown.is_none() {
            tip += "\nDon't see the number next to the clock? Click ^ and drag it out.";
        }
        self.tray.notify("EzDesk is ready", &tip);
    }

    fn note_done(&mut self, clicked: bool) {
        let Some((action, ignored)) = self.note.take() else { return };
        match if clicked { Some(action) } else { ignored } {
            Some(Note::ShowAllApps) => self.open_taskbar_settings(),
            Some(Note::ShowTrayIcon) => {
                self.settings.set("TrayIgnored", 0);
                self.open_tray_icon_settings();
            }
            Some(Note::TrayReminderIgnored) => {
                let n = self.settings.get("TrayIgnored", 0);
                self.settings.set("TrayIgnored", n + 1);
            }
            None => {}
        }
    }

    /// Windows' own page for "show apps from all desktops" (Multitasking › Desktops). Explorer only
    /// takes the setting into account when that page changes it, not when an app writes it.
    /// On Windows 11 the option is shown in Settings itself (coach.rs); Windows 10 gets a tip.
    fn open_taskbar_settings(&mut self) {
        system::open("ms-settings:multitasking");
        if !self.coach(coach::Spot::TaskbarFilter) {
            self.tray.notify("One switch to change", "Under Desktops, set \"On the taskbar, show all the open windows\" to \"On all desktops\".");
        }
    }

    /// Windows' own page for showing this icon on the taskbar (Taskbar › Other system tray icons).
    fn open_tray_icon_settings(&mut self) {
        system::open_tray_icon_settings(self.build);
        let lit = match system::tray_icon_key(tray::ID) {
            Some(key) => self.coach(coach::Spot::TrayIcon(key)),
            None => false,
        };
        if !lit {
            let text = if self.build >= 22000 {
                "Open Other system tray icons in the window that just opened, and turn on EzDesk."
            } else {
                "Turn on EzDesk in the window that just opened."
            };
            self.tray.notify("One switch to turn on", text);
        }
    }

    /// Lights up that option in Settings, which is opening (Windows 11). False if it can't.
    fn coach(&mut self, spot: coach::Spot) -> bool {
        if self.build < 22000 {
            return false;
        }
        self.coach = coach::Coach::new(spot, win::scale() as f32);
        if self.coach.is_some() {
            self.timer(T_COACH, 16);
        }
        self.coach.is_some()
    }

    // ---- Tray menu ----
    // EzDesk's own option, then Windows' own settings: those open the Settings page where they're
    // changed (their check shows how they are now).

    fn menu_entries(&self) -> Vec<Entry> {
        let icon_shown = system::tray_icon_shown(self.hwnd, tray::ID, self.build) == Some(true);
        let mut e = vec![
            Entry::item(M_TASK_VIEW, "Task View").bold(), // what clicking the icon does
            Entry::Sep,
            Entry::item(M_HOLD, "Hold a window to move it between desktops").checked(self.hold_on),
            Entry::Sep,
            Entry::item(M_TRAY_ICON, "Show this icon on the taskbar…").ok(icon_shown),
            Entry::item(M_TASKBAR, "Taskbar: show apps from all desktops…").ok(system::taskbar_all_desktops()),
        ];
        e.push(Entry::Sep);
        e.push(if self.packaged {
            Entry::item(M_STARTUP, "Start with Windows…")
        } else if self.admin {
            Entry::item(M_STARTUP, "Starts with Windows (admin mode)").checked(true).enabled(false)
        } else {
            Entry::item(M_STARTUP, "Start with Windows").checked(system::startup_on())
        });
        e.extend([Entry::item(M_HELP, "Help"), Entry::Sep, Entry::item(M_RESTART, "Restart"), Entry::item(M_EXIT, "Exit")]);
        e
    }

    fn command(&mut self, id: usize) {
        match id {
            M_TASK_VIEW => win::press(&[VK_LWIN, VK_TAB]),
            M_HOLD => {
                self.hold_on = !self.hold_on;
                self.settings.set("HoldWindow", self.hold_on as i32);
            }
            M_TRAY_ICON => self.open_tray_icon_settings(),
            M_TASKBAR => self.open_taskbar_settings(),
            M_STARTUP if self.packaged => system::open("ms-settings:startupapps"),
            M_STARTUP if !self.admin => system::set_startup(!system::startup_on()),
            M_HELP => system::open(HELP_URL),
            M_RESTART => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn(); // it closes this one when it starts
                }
            }
            M_EXIT => unsafe {
                let _ = PostMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            _ => {}
        }
    }

    /// Never leave a window pinned to every desktop, or lit, behind us.
    fn shut_down(&mut self) {
        let h = self.hold.dragging();
        if h != 0 {
            self.desk.drop_here(h, true);
        }
        self.held.release();
        self.bumps.hide();
        self.dock_hide();
        self.tray.remove();
    }
}

/// WM_NCHITTEST at (x, y): 2 = title bar (HTCAPTION), 1 = client area. 0 if it doesn't answer.
fn hit_test(h: HWND, x: i32, y: i32) -> usize {
    let mut r = 0usize;
    let l = ((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF);
    unsafe {
        SendMessageTimeoutW(h, WM_NCHITTEST, WPARAM(0), LPARAM(l as i32 as isize), SMTO_ABORTIFHUNG | SMTO_BLOCK, 200, Some(&mut r));
    }
    r
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            let id = wp.0;
            with(|a| match id {
                T_TRAY => a.update_tray(),
                T_HOLD => a.hold_tick(),
                T_BADGE => a.badge_tick(),
                T_OFFER => {
                    a.stop(T_OFFER);
                    a.offer_taskbar_all_desktops();
                }
                T_CHECK_TRAY => {
                    a.stop(T_CHECK_TRAY);
                    a.check_tray_icon();
                }
                T_WELCOME => {
                    a.stop(T_WELCOME);
                    a.welcome();
                }
                T_WARM => {
                    a.stop(T_WARM);
                    a.bumps.warm(); // making the bumps' shapes takes a moment: done ahead of time
                }
                T_DOCK => a.dock_tick(),
                T_DOCK_REFRESH => {
                    a.stop(T_DOCK_REFRESH);
                    a.dock_refresh();
                }
                T_COACH => {
                    if !a.coach.as_mut().is_some_and(|c| c.tick()) {
                        a.coach = None; // its light is gone
                        a.stop(T_COACH);
                    }
                }
                _ => {}
            });
            LRESULT(0)
        }
        WM_RENAME_DONE => {
            let keep = wp.0 == 1;
            with(|a| a.rename_done(keep));
            LRESULT(0)
        }
        tray::WM_TRAY => {
            // version 4: the event in lParam's low word (wParam's anchor point isn't used: it's off
            // for an icon in the ^ panel; the dock asks where the icon is)
            match lp.0 as u32 & 0xFFFF {
                tray::POPUP_OPEN => {
                    with(|a| a.dock_open());
                }
                WM_LBUTTONUP => {
                    with(|a| a.dock_hide());
                    win::press(&[VK_LWIN, VK_TAB]); // Task View
                }
                WM_CONTEXTMENU => {
                    with(|a| {
                        a.dock_hide();
                        let e = a.menu_entries();
                        a.menu_open(e, -1);
                    });
                }
                tray::BALLOON_CLICK => {
                    with(|a| a.note_done(true));
                }
                tray::BALLOON_GONE => {
                    with(|a| a.note_done(false));
                }
                _ => {}
            }
            LRESULT(0)
        }
        hooks::WM_WHEEL => {
            let delta = wp.0 as isize as i32;
            with(|a| a.step(delta));
            LRESULT(0)
        }
        hooks::WM_PRESS => {
            let (x, y) = (wp.0 as isize as i32, lp.0 as i32);
            with(|a| a.press(x, y));
            LRESULT(0)
        }
        hooks::WM_MOVESIZE => {
            let (start, target) = (wp.0 == 1, lp.0);
            with(|a| a.move_size(start, target));
            LRESULT(0)
        }
        WM_DROP => {
            let (carried, target) = (wp.0 != 0, lp.0);
            with(|a| a.drop_window(target, carried));
            LRESULT(0)
        }
        MSG_DESKTOP_CHANGED => {
            with(|a| a.update_tray());
            LRESULT(0)
        }
        WM_CLOSE => {
            with(|a| a.shut_down());
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            if msg != 0 && with(|a| a.taskbar_created == msg) == Some(true) {
                with(|a| {
                    a.tray.add(); // Explorer restarted: the icon again
                    a.shown = (-2, -2);
                    a.update_tray();
                });
                return LRESULT(0);
            }
            DefWindowProcW(h, msg, wp, lp)
        }
    }
}

/// The mini-dock's window: it draws, follows the mouse, and takes clicks.
unsafe extern "system" fn dock_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let at = || ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32);
    let hit_at = |x: f32, y: f32| with(|a| a.dock.as_ref().map(|d| minidock::hit(&d.layout, x, y, d.can_close))).flatten().unwrap_or(Hit::None);
    match msg {
        WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            windows::Win32::Graphics::Gdi::BeginPaint(h, &mut ps);
            with(|a| {
                if let Some(d) = a.dock.as_mut() {
                    d.paint();
                }
            });
            let _ = windows::Win32::Graphics::Gdi::EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1), // Direct2D paints it all
        WM_MOUSEACTIVATE => {
            // not activated by a click (the window you were in keeps the focus), unless renaming
            let renaming = with(|a| a.dock.as_ref().is_some_and(|d| d.renaming.is_some())).unwrap_or(false);
            LRESULT(if renaming { 1 } else { 3 }) // MA_ACTIVATE / MA_NOACTIVATE
        }
        WM_MOUSEMOVE => {
            let (x, y) = at();
            let hit = hit_at(x, y);
            with(|a| {
                if let Some(d) = a.dock.as_mut() {
                    if d.drag.is_some() {
                        d.drag_to(x); // a card follows the mouse
                    } else {
                        d.set_hover(hit);
                    }
                }
            });
            let mut tme = windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT>() as u32,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,
                hwndTrack: h,
                dwHoverTime: 0,
            };
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut tme);
            LRESULT(0)
        }
        WM_MOUSELEAVE_ => {
            with(|a| {
                if let Some(d) = a.dock.as_mut() {
                    d.set_hover(Hit::None);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = at();
            if let Hit::Card(i) = hit_at(x, y) {
                // maybe a click, maybe the start of a drag to reorder
                with(|a| {
                    if let Some(d) = a.dock.as_mut().filter(|d| d.renaming.is_none()) {
                        d.press(i, x);
                    }
                });
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(h);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = at();
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            let hit = hit_at(x, y);
            with(|a| {
                let dragged = a.dock.as_mut().and_then(|d| {
                    let was = d.drag.is_some_and(|g| g.active);
                    let moved = d.release();
                    was.then_some(moved)
                });
                match dragged {
                    Some(Some((from, to))) => a.dock_reorder(from, to),
                    Some(None) => {} // dragged, then put back where it was
                    None => a.dock_click(hit),
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = at();
            let target = match hit_at(x, y) {
                Hit::Card(i) | Hit::Close(i) => i,
                _ => return LRESULT(0),
            };
            with(|a| {
                let e = a.dock_menu_entries(target);
                a.menu_open(e, target);
            });
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            // the rename field, in the dock's colours
            let dc = windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut _);
            windows::Win32::Graphics::Gdi::SetTextColor(dc, windows::Win32::Foundation::COLORREF(0x00FA_FAFA));
            windows::Win32::Graphics::Gdi::SetBkColor(dc, windows::Win32::Foundation::COLORREF(0x001B_1818)); // #18181B, as BGR
            let brush = with(|a| a.edit_brush).unwrap_or_default();
            LRESULT(brush.0 as isize)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

/// The rename field: Enter keeps the new name, Esc drops it, clicking elsewhere keeps it.
unsafe extern "system" fn edit_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    let parent = GetParent(h).unwrap_or_default();
    match msg {
        WM_KEYDOWN if wp.0 == 0x0D || wp.0 == 0x1B => {
            let _ = PostMessageW(parent, WM_RENAME_DONE, WPARAM((wp.0 == 0x0D) as usize), LPARAM(0)); // VK_RETURN / VK_ESCAPE
            LRESULT(0)
        }
        WM_CHAR if wp.0 == 0x0D || wp.0 == 0x1B => LRESULT(0), // no beep
        WM_KILLFOCUS => {
            let _ = PostMessageW(parent, WM_RENAME_DONE, WPARAM(1), LPARAM(0));
            windows::Win32::UI::Shell::DefSubclassProc(h, msg, wp, lp)
        }
        _ => windows::Win32::UI::Shell::DefSubclassProc(h, msg, wp, lp),
    }
}

/// The menu's window: it draws, follows the mouse and the keyboard, and closes when you click
/// elsewhere (it's deactivated).
unsafe extern "system" fn menu_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let at = || ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32);
    let hit_at = |x: f32, y: f32| with(|a| a.menu.as_ref().and_then(|m| menu::hit(&m.layout, &m.entries, x, y))).flatten();
    let with_menu = |f: &mut dyn FnMut(&mut menu::Menu)| {
        with(|a| {
            if let Some(m) = a.menu.as_mut() {
                f(m);
            }
        });
    };
    match msg {
        WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            windows::Win32::Graphics::Gdi::BeginPaint(h, &mut ps);
            with_menu(&mut |m| m.paint());
            let _ = windows::Win32::Graphics::Gdi::EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER => {
            with_menu(&mut |m| {
                if m.hover.is_some() {
                    m.redraw(); // the light flows on
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = at();
            let hit = hit_at(x, y);
            with_menu(&mut |m| m.set_hover(hit));
            let mut tme = windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT>() as u32,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,
                hwndTrack: h,
                dwHoverTime: 0,
            };
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut tme);
            LRESULT(0)
        }
        WM_MOUSELEAVE_ => {
            with_menu(&mut |m| m.set_hover(None));
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = at();
            if let Some(i) = hit_at(x, y) {
                with(|a| a.menu_chosen(i));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wp.0 as u16 {
                0x1B => {
                    with(|a| a.menu_close()); // Esc
                }
                0x26 | 0x28 => {
                    // ↑ ↓
                    let dir = if wp.0 == 0x28 { 1 } else { -1 };
                    with_menu(&mut |m| {
                        let n = menu::next(&m.entries, m.hover, dir);
                        m.set_hover(n);
                    });
                }
                0x0D | 0x20 => {
                    // Enter, Space
                    if let Some(i) = with(|a| a.menu.as_ref().and_then(|m| m.hover)).flatten() {
                        with(|a| a.menu_chosen(i));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if wp.0 & 0xFFFF == 0 {
                // WA_INACTIVE: clicked elsewhere
                let _ = PostMessageW(h, WM_CLOSE_MENU, WPARAM(0), LPARAM(0)); // out of the activation first
            }
            LRESULT(0)
        }
        WM_CLOSE_MENU => {
            with(|a| a.menu_close());
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

const MAIN_CLASS: &str = "EzDeskMain";

/// Another EzDesk is running: it's closed, this one takes over (a double-click restarts it).
fn close_other_instance() {
    let class = wide(MAIN_CLASS);
    unsafe {
        if let Ok(other) = FindWindowW(pcw(&class), windows::core::PCWSTR::null()) {
            let _ = PostMessageW(other, WM_CLOSE, WPARAM(0), LPARAM(0));
            for _ in 0..60 {
                if !IsWindow(other).as_bool() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

fn main() {
    let build = system::windows_build();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--probe") {
        let what = args.get(i + 1).map_or("dll", |s| s.as_str());
        std::process::exit(vda::probe(build, what)); // the child process that tries an adapter
    }
    let started_at_login = std::env::args().any(|a| a == "--startup");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    close_other_instance();

    let class = wide(MAIN_CLASS);
    let title = wide("EzDesk");
    let hwnd = unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: pcw(&class), ..Default::default() };
        RegisterClassW(&wc);
        match CreateWindowExW(WS_EX_TOOLWINDOW, pcw(&class), pcw(&title), WS_POPUP, 0, 0, 0, 0, None, None, inst, None) {
            Ok(h) => h,
            Err(e) => {
                win::log(&format!("can't create the main window: {e}"));
                return;
            }
        }
    };
    win::log(&format!("started (Windows build {build})"));
    let tc = wide("TaskbarCreated");
    let taskbar_created = unsafe { RegisterWindowMessageW(pcw(&tc)) };
    // In admin mode, Explorer (not elevated) must still be able to reach us.
    for m in [tray::WM_TRAY, taskbar_created, MSG_DESKTOP_CHANGED] {
        unsafe {
            let _ = ChangeWindowMessageFilterEx(hwnd, m, MSGFLT_ALLOW, None);
        }
    }

    // Our icon right away, before any call into the DLL: if Windows' virtual desktops aren't
    // reachable yet (login, Explorer restart), the icon and its menu still work.
    let mut tray = Tray::new(hwnd);
    tray.set_tip("EzDesk (starting…)");
    tray.set_icon(&win::exe_dir().join("icons").join("1.ico"));
    tray.add();

    win::log("tray icon added");
    // Windows 11 24H2+: compiled in. Else (or if it doesn't see desktops yet): the DLL. Each one is
    // tried in a child process first, so one that doesn't match this Windows can't crash us.
    let native_ok = build >= native::MIN_BUILD && vda::probe_ok("native");
    let adapter: Box<dyn Adapter> = if native_ok {
        Box::new(NativeAdapter::new())
    } else {
        match VdaAdapter::load(build) {
            Ok(a) if vda::probe_ok("dll") => Box::new(a),
            Ok(_) => {
                tray.notify("EzDesk", &format!("The virtual-desktop library doesn't work on this Windows version (build {build}).\nMoving windows between desktops is off. See Help in this icon's menu."));
                Box::new(VdaAdapter::broken())
            }
            Err(_) if build >= native::MIN_BUILD => Box::new(NativeAdapter::new()), // no DLL: the compiled-in one, ready or not
            Err(why) => {
                let (t, c) = (wide(&why), wide("EzDesk"));
                unsafe {
                    MessageBoxW(None, pcw(&t), pcw(&c), MB_ICONERROR);
                }
                tray.remove();
                return;
            }
        }
    };
    win::log(if native_ok { "desktops: compiled in (winvd)" } else { "desktops: DLL" });

    let settings = Settings::new();
    let packaged = system::is_packaged();
    let admin = system::is_admin();
    if !packaged && !admin {
        // Upgrading from the AutoHotkey version: its startup shortcut becomes the Run entry.
        if system::remove_startup_shortcuts() || system::startup_on() {
            system::set_startup(true); // (re)written each launch, so it follows the folder if it moves
        }
    }
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    // The mini-dock's window: on top, not in the taskbar, not activated by a click.
    let dock = unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let dc = wide("EzDeskDock");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(dock_proc),
            hInstance: inst.into(),
            lpszClassName: pcw(&dc),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE, pcw(&dc), None, WS_POPUP | WS_CLIPCHILDREN, 0, 0, 0, 0, None, None, inst, None)
            .ok()
            .and_then(|h| Dock::new(h, win::scale() as f32))
    };
    // The menus' window (the tray icon's, a dock card's): on top, not in the taskbar.
    let menu = unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let mc = wide("EzDeskMenu");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(menu_proc),
            hInstance: inst.into(),
            lpszClassName: pcw(&mc),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, pcw(&mc), None, WS_POPUP, 0, 0, 0, 0, None, None, inst, None)
            .ok()
            .and_then(|h| menu::Menu::new(h, win::scale() as f32))
    };
    if dock.is_none() {
        win::log("no mini-dock (Direct2D unavailable)");
    }
    let app = App {
        hwnd,
        desk: Desktops::new(adapter),
        hold: HoldGesture::new(),
        held: HeldWindow::new(Dwm { can: build >= 22000 }),
        bumps: Bumps::new(win::scale()),
        bumps_pinned: false,
        tray,
        hold_on: settings.get("HoldWindow", 1) == 1,
        tray_reminder: settings.get("TrayReminder", 1) == 1,
        settings,
        build,
        packaged,
        admin,
        swapped,
        shown: (-2, -2),
        connected: false,
        failures: 0,
        warned: false,
        last_step: 0,
        note: None,
        badge: badge::Badge::new(),
        badge_busy: false,
        taskbar_created,
        dock,
        dock_anchor: RECT::default(),
        dock_left: 0,
        dock_menu: false,
        edit: HWND::default(),
        edit_brush: unsafe { windows::Win32::Graphics::Gdi::CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x001B_1818)) },
        menu,
        coach: None,
    };
    // Every start checks that the number is on the taskbar (the very first one with the welcome
    // tip); a few seconds in, so Windows has placed the icon.
    if !started_at_login && system::first_run() {
        app.timer(T_WELCOME, 3000);
    } else {
        app.timer(T_CHECK_TRAY, 8000);
    }
    app.timer(T_TRAY, 100); // first contact with the DLL happens after start-up is complete
    app.timer(T_WARM, 5000);
    APP.with(|a| *a.borrow_mut() = Some(app));
    hooks::start(hwnd, swapped);
    win::log("ready");

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
