# Microsoft Store submission: ready-to-paste answers

## Product identity → `store/identity.json`

After reserving the name, copy these 3 values from **Partner Center → your app → Product management → Product identity** into `store/identity.json`:

| Partner Center | identity.json |
|---|---|
| Package/Identity/Name | `IdentityName` |
| Package/Identity/Publisher | `Publisher` (starts with `CN=`) |
| Package/Properties/PublisherDisplayName | `PublisherDisplayName` |

## Pricing and availability

- **Price:** Free
- **Markets:** all
- **Visibility:** Public

## Properties

- **Category:** Productivity (or Utilities & tools)
- **Privacy policy URL:** https://github.com/alxbdn/ezdesk/blob/main/PRIVACY.md
- **Website:** https://github.com/alxbdn/ezdesk
- **Support contact:** https://github.com/alxbdn/ezdesk/issues

## Age ratings

Answer "No" to every content question (no violence, chat, user-generated content, purchases, location…). The result is 3+ / Everyone.

## Packages

Upload `EzDesk.msix` from the GitHub Actions artifact **EzDesk-store-msix**, built with your real identity.

### Restricted capability: `runFullTrust`

Partner Center asks you to justify it. Paste:

> EzDesk is a classic Win32 desktop app, written in Rust and open source. Full trust is required to watch window drags and mouse presses system-wide (holding a window brings up drops of light on the screen's edges, to slide it to another desktop), to show those drops over other windows, to react to the mouse wheel over the taskbar, to show its notification-area icon with the current desktop number and a mini-dock of desktops with live previews of their windows, and to call the Windows virtual desktop APIs through the open-source VirtualDesktopAccessor library. It does not access the network, user files or personal data. Source: https://github.com/alxbdn/ezdesk

## Store listing (English)

**Product name:** EzDesk (reserved; Store ID `9MV9P36W441C`). It must match `DisplayName` in `store/identity.json`. Earlier name: Desktop Switcher - Virtual Desktop Shortcuts.

**Short description**

> Move windows between virtual desktops with the mouse alone, and always see which desktop you're on.

**Description**

> Windows has virtual desktops, but moving a window to another one takes a trip through Task View, and nothing tells you which desktop you're on. EzDesk makes it a mouse gesture.
>
> • Hold a window by its title bar for a moment: a drop of light clings to each edge of the screen. Bring the window to an edge and the drop grows, with the number of the desktop you'd land on next to it. Hold it there: the drop fills with white and you slide to that desktop with the window in hand. Past the last desktop, it makes a new one.
> • Rest the mouse on the desktop number near the clock: a mini-dock shows your desktops with their wallpaper and live previews. Click one to go there, × to close it, + for a new one, drag to reorder them, right-click to rename one or choose its background.
> • Scroll on the taskbar: previous / next desktop. Scrolling over the volume icon still changes the volume.
> • The tray icon shows the current desktop's number in large white digits, rolling up or down when you change desktop. Click it for Task View.
>
> Organize your work the way you think: one desktop for meetings, one for email and the web, one for code. No shortcuts to learn.
>
> Works on Windows 11 and Windows 10. Free and open source (MIT). No account, no tracking, no internet connection.
>
> Tip: if you don't see the numbered icon, click the ^ arrow next to the clock and drag the icon onto the taskbar.

**Features** (one per line in Partner Center)

> Hold a window and bring it to a side of the screen to take it to the next or previous desktop
> Drops of light on the screen edges that grow as the window comes close
> Mini-dock on the tray number: live previews, switch, add, close, reorder and rename desktops
> Scroll on the taskbar to step through desktops
> Tray icon with the current desktop number, large and easy to read
> Starts with Windows (can be turned off in Settings › Apps › Startup)
> Free, open source, no tracking

**Keywords** (up to 7): virtual desktop, desktop switcher, workspaces, productivity, drag and drop, multitasking, window manager

**Screenshots:** at least 1, 1366 × 768 or larger (PNG). Real screenshots of your taskbar with the numbered icon work best, or frames from the promo video.

**What's new in this version**

> • A mini-dock on the tray number: rest the mouse on it to see your desktops with their wallpaper and live previews. Click one to go there, × to close it, + for a new one, drag to reorder them (Windows 11 24H2+), right-click to rename one or choose its background.
> • Drops of light on the screen edges replace the bumps: they grow as you bring a window close, show the desktop you'd land on, and fill with white before you slide there.
> • A bigger desktop number in the tray, in white, rolling up or down when you change desktop.
> • The menu options for the taskbar icon and for showing apps from all desktops open the right Windows Settings page.
> • Native on Windows 11 24H2 and later. Older Windows keeps working as before.

**Logo for the listing:** `brand/ezdesk-1080.png` (1:1 box art). Larger sizes come from `brand/ezdesk.svg`.
