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
//! `bumps` (the light), `tray`, `hooks` (the mouse), `system` (settings, startup, taskbar).
//! This file wires them to the main window's messages.

#![windows_subsystem = "windows"]

mod bumps;
mod desktops;
mod glow;
mod held;
mod hold;
mod hooks;
mod layered;
mod system;
mod tray;
mod vda;
mod win;

use bumps::Bumps;
use desktops::Desktops;
use held::{HeldWindow, WinOps};
use hold::{Frame, HoldGesture, Intent, Phase};
use std::cell::RefCell;
use std::path::PathBuf;
use system::Settings;
use tray::Tray;
use vda::VdaAdapter;
use win::{pcw, wide};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_LWIN, VK_RBUTTON, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::*;

const MSG_DESKTOP_CHANGED: u32 = 0x1400 + 30; // the DLL posts it on every switch
const WM_DROP: u32 = WM_APP + 10;

const T_TRAY: usize = 1;
const T_HOLD: usize = 2;
const T_FLASH: usize = 3;
const T_CHECK_TRAY: usize = 4;
const T_WELCOME: usize = 5;
const T_OFFER: usize = 6;
const T_WARM: usize = 7;

// Tray menu
const M_TASK_VIEW: usize = 1;
const M_HOLD: usize = 2;
const M_REMIND: usize = 3;
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
    desk: Desktops<VdaAdapter>,
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
    flash: Vec<HICON>,
    flash_step: usize,
    flash_base: PathBuf,
    taskbar_created: u32,
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
            self.tray.set_tip(&format!("EzDesk: not compatible with this Windows (build {})", self.build));
            return;
        }
        let (i, c) = (self.desk.current(), self.desk.count());
        if i < 0 || c <= 0 {
            // virtual desktops not reachable (yet)
            self.failures += 1;
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
            self.desk.listen(win::id(self.hwnd), MSG_DESKTOP_CHANGED);
            self.timer(T_TRAY, 1000);
        }
        if (i, c) == self.shown {
            return;
        }
        let switched = self.shown.0 >= 0 && i != self.shown.0; // a real switch, not the first reading
        self.shown = (i, c);
        let path = self.icon_path(i + 1);
        self.tray.set_icon(&path);
        if switched {
            self.timer(T_OFFER, 1500); // once the slide is over
            self.flash = self.tray.flash(&path);
            self.flash_step = 0;
            self.flash_base = path;
            self.timer(T_FLASH, 60);
        }
        let tip = match self.desk.name(i) {
            Some(n) => format!("Desktop {} of {c} — {n}", i + 1),
            None => format!("Desktop {} of {c}", i + 1),
        };
        self.tray.set_tip(&tip);
    }

    fn flash_frame(&mut self) {
        if self.flash_step >= self.flash.len() {
            self.stop(T_FLASH);
            let base = self.flash_base.clone();
            self.tray.set_icon(&base); // back to the plain number
            return;
        }
        self.tray.set_frame(self.flash[self.flash_step]);
        self.flash_step += 1;
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
            Some(Note::ShowAllApps) => {
                if self.packaged {
                    system::open("ms-settings:multitasking");
                    self.tray.notify("One switch to change", "Under Desktops, set \"On the taskbar, show all the open windows\" to \"On all desktops\".");
                } else {
                    system::set_taskbar_all_desktops(true);
                    self.tray.notify("Done", "The taskbar now shows the apps of every desktop. Clicking one takes you to its desktop.");
                }
            }
            Some(Note::ShowTrayIcon) => {
                self.settings.set("TrayIgnored", 0);
                system::open_tray_icon_settings(self.build);
                let text = if self.build >= 22000 {
                    "Open Other system tray icons in the window that just opened, and turn on EzDesk."
                } else {
                    "Turn on EzDesk in the window that just opened."
                };
                self.tray.notify("One switch to turn on", text);
            }
            Some(Note::TrayReminderIgnored) => {
                let n = self.settings.get("TrayIgnored", 0);
                self.settings.set("TrayIgnored", n + 1);
            }
            None => {}
        }
    }

    // ---- Tray menu ----

    fn menu(&self) -> HMENU {
        unsafe {
            let m = CreatePopupMenu().unwrap_or_default();
            let add = |id: usize, text: &str, checked: bool, enabled: bool| {
                let mut flags = MF_STRING;
                if checked {
                    flags |= MF_CHECKED;
                }
                if !enabled {
                    flags |= MF_GRAYED;
                }
                let t = wide(text);
                let _ = AppendMenuW(m, flags, id, pcw(&t));
            };
            let sep = || {
                let _ = AppendMenuW(m, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
            };
            add(M_TASK_VIEW, "Task View", false, true);
            let _ = SetMenuDefaultItem(m, M_TASK_VIEW as u32, 0);
            sep();
            add(M_HOLD, "Hold a window to move it between desktops", self.hold_on, true);
            add(M_REMIND, "Remind me when this icon is hidden", self.tray_reminder, true);
            if self.packaged {
                add(M_TASKBAR, "Taskbar: show apps from all desktops…", false, true);
            } else {
                add(M_TASKBAR, "Taskbar: show apps from all desktops", system::taskbar_all_desktops(), true);
            }
            sep();
            if self.packaged {
                add(M_STARTUP, "Start with Windows…", false, true);
            } else if self.admin {
                add(M_STARTUP, "Starts with Windows (admin mode)", true, false);
            } else {
                add(M_STARTUP, "Start with Windows", system::startup_on(), true);
            }
            add(M_HELP, "Help", false, true);
            sep();
            add(M_RESTART, "Restart", false, true);
            add(M_EXIT, "Exit", false, true);
            m
        }
    }

    fn command(&mut self, id: usize) {
        match id {
            M_TASK_VIEW => win::press(&[VK_LWIN, VK_TAB]),
            M_HOLD => {
                self.hold_on = !self.hold_on;
                self.settings.set("HoldWindow", self.hold_on as i32);
            }
            M_REMIND => {
                self.tray_reminder = !self.tray_reminder;
                self.settings.set("TrayReminder", self.tray_reminder as i32);
                self.settings.set("TrayIgnored", 0); // ticked again: a fresh start
            }
            M_TASKBAR if self.packaged => system::open("ms-settings:multitasking"),
            M_TASKBAR => system::set_taskbar_all_desktops(!system::taskbar_all_desktops()),
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
                T_FLASH => a.flash_frame(),
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
                _ => {}
            });
            LRESULT(0)
        }
        tray::WM_TRAY => {
            match lp.0 as u32 & 0xFFFF {
                WM_LBUTTONUP => win::press(&[VK_LWIN, VK_TAB]), // Task View
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    // built with the app, shown without it: the menu runs its own message loop
                    if let Some(m) = with(|a| a.menu()) {
                        let (x, y) = win::cursor();
                        let _ = SetForegroundWindow(h);
                        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, x, y, 0, h, None);
                        let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0));
                        let _ = DestroyMenu(m);
                        if cmd.0 != 0 {
                            with(|a| a.command(cmd.0 as usize));
                        }
                    }
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
    if std::env::args().any(|a| a == "--probe") {
        std::process::exit(vda::probe(build)); // the child process that tries the DLL
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
    let adapter = match VdaAdapter::load(build) {
        Ok(a) if vda::probe_ok() => a,
        Ok(_) => {
            tray.notify("EzDesk", &format!("The virtual-desktop library doesn't work on this Windows version (build {build}).\nMoving windows between desktops is off. See Help in this icon's menu."));
            VdaAdapter::broken()
        }
        Err(why) => {
            let (t, c) = (wide(&why), wide("EzDesk"));
            unsafe {
                MessageBoxW(None, pcw(&t), pcw(&c), MB_ICONERROR);
            }
            tray.remove();
            return;
        }
    };

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
        flash: vec![],
        flash_step: 0,
        flash_base: PathBuf::new(),
        taskbar_created,
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
