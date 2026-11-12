//! A guide in Windows' Settings: when EzDesk opens a Settings page for one of Windows' own options,
//! it finds that option, opens its section if it's folded, scrolls to it, and lights it up with
//! EzDesk's flowing light. As soon as the option is set the right way, a check appears beside it
//! in the same light, and it all fades.
//!
//! Settings says nothing about itself to other apps, but it shows everything to UI Automation
//! (what screen readers use): every option has a stable id, whatever the language. The light is a
//! click-through window laid over Settings, following the option as you scroll.
//!
//! `Spot` (what to find, when it's right) and `fade` are pure; `Coach` asks Settings and draws.

use crate::layered::Layered;
use crate::ui::{self, R};
use crate::win;
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::System::Com::{CoCreateInstance, CoIncrementMTAUsage, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Accessibility::*;

/// The option to light up.
#[derive(Debug, Clone, PartialEq)]
pub enum Spot {
    /// Multitasking › Desktops › "On the taskbar, show all the open windows".
    TaskbarFilter,
    /// Taskbar › Other system tray icons › EzDesk's own switch; its key under NotifyIconSettings.
    TrayIcon(String),
}

impl Spot {
    /// Its UI Automation id in Settings.
    pub fn id(&self) -> String {
        match self {
            Spot::TaskbarFilter => "SystemSettings_VirtualDesktops_TaskbarFilter_ComboBox".into(),
            Spot::TrayIcon(key) => format!("SystemSettings_DesktopTaskbar_SystemTrayAppListItem{key}_ToggleSwitch"),
        }
    }

    /// Set the right way (as Windows keeps it).
    pub fn done(&self) -> bool {
        match self {
            Spot::TaskbarFilter => crate::system::taskbar_all_desktops(),
            Spot::TrayIcon(key) => crate::system::tray_key_promoted(key),
        }
    }
}

/// How much is left of the light, `ms` after the option was set: all of it for a moment, then fading.
pub fn fade(ms: u64) -> f32 {
    const HOLD: u64 = 1400;
    const OUT: u64 = 600;
    if ms <= HOLD {
        1.0
    } else {
        (1.0 - (ms - HOLD) as f32 / OUT as f32).max(0.0)
    }
}

const GIVE_UP_MS: u64 = 8000; // Settings never showed the option
const LIFE_MS: u64 = 90_000; // lit for at most this long

enum Stage {
    /// Looking for the option; i: the next folded section to try opening.
    Find { i: usize, opened: Option<IUIAutomationElement> },
    Lit,
    Set { at: u64 },
}

pub struct Coach {
    spot: Spot,
    auto: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    cache: IUIAutomationCacheRequest,
    all: IUIAutomationCondition,
    frame: Option<(HWND, IUIAutomationElement)>,
    target: Option<IUIAutomationElement>,
    stage: Stage,
    started: u64,
    asked: u64,
    row: Option<RECT>, // the option's row, on screen
    s: f32,
    d2d: ID2D1Factory,
    rt: Option<ID2D1DCRenderTarget>,
    stops: Option<ID2D1GradientStopCollection>,
    win: Option<Layered>,
}

impl Coach {
    pub fn new(spot: Spot, s: f32) -> Option<Self> {
        unsafe {
            let _ = CoIncrementMTAUsage();
            let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let cache = auto.CreateCacheRequest().ok()?;
            cache.AddProperty(UIA_AutomationIdPropertyId).ok()?;
            cache.AddProperty(UIA_ClassNamePropertyId).ok()?;
            Some(Coach {
                spot,
                walker: auto.ControlViewWalker().ok()?,
                all: auto.CreateTrueCondition().ok()?,
                cache,
                auto,
                frame: None,
                target: None,
                stage: Stage::Find { i: 0, opened: None },
                started: win::now(),
                asked: 0,
                row: None,
                s,
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?,
                rt: None,
                stops: None,
                win: None,
            })
        }
    }

    /// Every 16 ms. False once it's over (the light is gone).
    pub fn tick(&mut self) -> bool {
        let now = win::now();
        let age = now.saturating_sub(self.started);
        if age > LIFE_MS {
            return self.end();
        }
        // Settings closed: over.
        if let Some((h, _)) = &self.frame {
            if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(*h) }.as_bool() {
                return self.end();
            }
        }
        let ask = now.saturating_sub(self.asked) >= 250; // asking Settings takes a few ms: 4 times a second
        match &mut self.stage {
            Stage::Find { .. } => {
                if age > GIVE_UP_MS {
                    return self.end();
                }
                if ask {
                    self.asked = now;
                    self.find_step();
                }
            }
            Stage::Lit => {
                if ask {
                    self.asked = now;
                    self.track();
                    if self.spot.done() {
                        self.stage = Stage::Set { at: now };
                    }
                }
            }
            Stage::Set { at } => {
                let left = fade(now.saturating_sub(*at));
                if left <= 0.0 {
                    return self.end();
                }
                if ask {
                    self.asked = now;
                    self.track();
                }
            }
        }
        self.draw(now);
        true
    }

    fn end(&mut self) -> bool {
        if let Some(w) = self.win.as_mut() {
            w.destroy();
        }
        self.win = None;
        false
    }

    // ---- Finding the option ----

    /// Settings' window: the frame in front (EzDesk just opened it), once it shows Settings' ids.
    fn settings_frame(&self) -> Option<(HWND, IUIAutomationElement)> {
        let h = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if win::class_name(h) != "ApplicationFrameWindow" {
            return None;
        }
        let e = unsafe { self.auto.ElementFromHandle(h) }.ok()?;
        Some((h, e))
    }

    /// Everything in Settings' page, with ids and classes fetched at once.
    fn elements(&self, root: &IUIAutomationElement) -> Vec<(IUIAutomationElement, String, String)> {
        let mut out = vec![];
        unsafe {
            let Ok(arr) = root.FindAllBuildCache(TreeScope_Descendants, &self.all, &self.cache) else { return out };
            for i in 0..arr.Length().unwrap_or(0) {
                if let Ok(e) = arr.GetElement(i) {
                    let id = e.CachedAutomationId().map(|b| b.to_string()).unwrap_or_default();
                    let class = e.CachedClassName().map(|b| b.to_string()).unwrap_or_default();
                    out.push((e, id, class));
                }
            }
        }
        out
    }

    /// One step: the option visible? Light it. Else open the next folded section (folding the
    /// last one back if the option wasn't there).
    fn find_step(&mut self) {
        if self.frame.is_none() {
            self.frame = self.settings_frame();
        }
        let Some((_, root)) = self.frame.clone() else { return };
        let els = self.elements(&root);
        if !els.iter().any(|(_, id, _)| id.starts_with("SystemSettings_")) {
            self.frame = None; // not Settings, or not loaded yet
            return;
        }
        let want = self.spot.id();
        if let Some((t, _, _)) = els.iter().find(|(_, id, _)| *id == want) {
            unsafe {
                if let Ok(sc) = t.GetCurrentPatternAs::<IUIAutomationScrollItemPattern>(UIA_ScrollItemPatternId) {
                    let _ = sc.ScrollIntoView();
                }
            }
            self.target = Some(t.clone());
            self.stage = if self.spot.done() { Stage::Set { at: win::now() } } else { Stage::Lit };
            self.track();
            return;
        }
        let Stage::Find { i, opened } = &mut self.stage else { return };
        unsafe {
            if let Some(prev) = opened.take() {
                if let Ok(ec) = prev.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId) {
                    let _ = ec.Collapse(); // not in that one: as it was
                }
            }
            let folded: Vec<&IUIAutomationElement> = els
                .iter()
                .filter(|(_, _, class)| class == "ExpanderToggleButton")
                .map(|(e, _, _)| e)
                .filter(|e| {
                    e.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
                        .and_then(|ec| ec.CurrentExpandCollapseState())
                        .map(|st| st == ExpandCollapseState_Collapsed)
                        .unwrap_or(false)
                })
                .collect();
            if let Some(e) = folded.get(*i) {
                if let Ok(ec) = e.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId) {
                    let _ = ec.Expand();
                    *opened = Some((*e).clone());
                }
            }
            *i += 1;
        }
    }

    /// Where the option's row is now: from the left of its section to the option, a little taller.
    fn track(&mut self) {
        let Some(t) = &self.target else { return };
        unsafe {
            let Ok(r) = t.CurrentBoundingRectangle() else { return };
            if r.right <= r.left {
                self.row = None; // scrolled away, or Settings minimized
                return;
            }
            let mut left = r.left;
            let mut e = self.walker.GetParentElement(t).ok();
            for _ in 0..3 {
                let Some(p) = e else { break };
                if let Ok(pr) = p.CurrentBoundingRectangle() {
                    if pr.left < r.left && pr.right >= r.right && (pr.right - pr.left) as f32 > (r.right - r.left) as f32 * 1.5 {
                        left = pr.left;
                        break;
                    }
                }
                e = self.walker.GetParentElement(&p).ok();
            }
            let pad = (10.0 * self.s) as i32;
            self.row = Some(RECT { left: left + pad / 2, top: r.top - pad, right: r.right + pad, bottom: r.bottom + pad });
        }
    }

    // ---- The light ----

    fn draw(&mut self, now: u64) {
        let s = self.s;
        let in_front = self.frame.as_ref().is_some_and(|(h, _)| unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() } == *h);
        let Some(row) = self.row.filter(|_| in_front) else {
            if let Some(w) = self.win.as_mut() {
                if w.visible {
                    w.hide(); // Settings behind another window: no light over that one
                }
            }
            return;
        };
        let m = (28.0 * s) as i32; // room for the glow and the badge
        let (w, h) = ((row.right - row.left + 2 * m) as usize, (row.bottom - row.top + 2 * m) as usize);
        if self.win.as_ref().map(|l| (l.w, l.h)) != Some((w, h)) {
            if let Some(old) = self.win.as_mut() {
                old.destroy();
            }
            self.win = Some(Layered::new(w, h));
            self.rt = None;
        }
        let win = self.win.as_mut().unwrap();
        if self.rt.is_none() {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            self.rt = unsafe { self.d2d.CreateDCRenderTarget(&props) }.ok();
            self.stops = self.rt.as_ref().and_then(|rt| rt.cast::<ID2D1RenderTarget>().ok()).and_then(|rt| ui::glow_stops(&rt));
        }
        let Some(rt) = self.rt.clone() else { return };
        let inner = R { x: m as f32, y: m as f32, w: (row.right - row.left) as f32, h: (row.bottom - row.top) as f32 };
        let mut alpha = 1.0;
        unsafe {
            if rt.BindDC(win.dc(), &RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 }).is_err() {
                return;
            }
            rt.BeginDraw();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            if let (Some(st), Ok(base)) = (&self.stops, rt.cast::<ID2D1RenderTarget>()) {
                ui::aura(&base, st, &inner, 8.0 * s, 10, s);
                if let Stage::Set { at } = self.stage {
                    // set: a check in the same light on the row's right, then it all fades
                    alpha = fade(now.saturating_sub(at));
                    ui::glow_check(&base, st, inner.x + inner.w + 4.0 * s, inner.y + inner.h / 2.0, 18.0 * s, s);
                }
            }
            let _ = rt.EndDraw(None, None);
        }
        win.put(row.left - m, row.top - m, (255.0 * alpha) as u8);
        if !win.visible || now.saturating_sub(win.raised_at) > 500 {
            win.raise(now); // over Settings
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_settings_shows() {
        assert_eq!(Spot::TaskbarFilter.id(), "SystemSettings_VirtualDesktops_TaskbarFilter_ComboBox");
        assert_eq!(Spot::TrayIcon("3666125188473074216".into()).id(), "SystemSettings_DesktopTaskbar_SystemTrayAppListItem3666125188473074216_ToggleSwitch");
    }

    #[test]
    fn the_check_holds_then_fades() {
        assert_eq!(fade(0), 1.0);
        assert_eq!(fade(1400), 1.0);
        assert!((fade(1700) - 0.5).abs() < 1e-6);
        assert_eq!(fade(2000), 0.0);
        assert_eq!(fade(9999), 0.0);
    }
}
