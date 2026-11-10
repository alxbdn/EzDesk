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

> EzDesk is a classic Win32 desktop app (the AutoHotkey v2 runtime running an open-source script). Full trust is required to watch window drags and mouse presses system-wide (holding a window opens a dock of desktops to drop it on), to show that dock and its glow over other windows, to react to the mouse wheel over the taskbar, to show its notification-area icon with the current desktop number, and to call the Windows virtual desktop APIs through the open-source VirtualDesktopAccessor library. It does not access the network, user files or personal data. Source: https://github.com/alxbdn/ezdesk

## Store listing (English)

**Product name:** EzDesk (reserved; Store ID `9MV9P36W441C`). It must match `DisplayName` in `store/identity.json`. Earlier name: Desktop Switcher - Virtual Desktop Shortcuts.

**Short description**

> Move windows between virtual desktops with the mouse alone, and always see which desktop you're on.

**Description**

> Windows has virtual desktops, but moving a window to another one takes a trip through Task View, and nothing tells you which desktop you're on. EzDesk makes it a mouse gesture.
>
> • Hold a window by its title bar for a moment: a dock of your desktops rises, each a live miniature. Drop the window on one: it goes there, and you stay where you are. "+" makes a new desktop.
> • Or bring it into the glowing bump on either side of the screen: you slide to the next or previous desktop with the window in hand.
> • Scroll on the taskbar: previous / next desktop. Scrolling over the volume icon still changes the volume.
> • A tray icon shows the current desktop's number. Click it to open the dock: go to a desktop, close one, or make a new one.
>
> Organize your work the way you think: one desktop for meetings, one for email and the web, one for code. No shortcuts to learn.
>
> Works on Windows 11 and Windows 10. Free and open source (MIT). No account, no tracking, no internet connection.
>
> Tip: if you don't see the numbered icon, click the ^ arrow next to the clock and drag the icon onto the taskbar.

**Features** (one per line in Partner Center)

> Hold a window to get a dock of your desktops, and drop it on one
> Live miniatures of every desktop, in a glowing dock
> Glowing side bumps to slide a window to the next or previous desktop
> Scroll on the taskbar to step through desktops
> Tray icon with the current desktop number
> Starts with Windows (can be turned off in Settings › Apps › Startup)
> Free, open source, no tracking

**Keywords** (up to 7): virtual desktop, desktop switcher, workspaces, productivity, drag and drop, multitasking, window manager

**Screenshots:** at least 1, 1366 × 768 or larger (PNG). Real screenshots of your taskbar with the numbered icon work best, or frames from the promo video.

**What's new in this version**

> All mouse now: hold a window by its title bar for a moment, a dock of your desktops rises; drop it on the one you want. Or bring it into the glowing bump on either side of the screen to take it to the next or previous desktop. The keyboard shortcuts are gone. The tray menu can now show apps from all desktops on the taskbar.

**Logo for the listing:** `brand/ezdesk-1080.png` (1:1 box art). Larger sizes come from `brand/ezdesk.svg`.
