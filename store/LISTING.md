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
- **Privacy policy URL:** https://github.com/alxbdn/win11-desktop-switcher/blob/main/PRIVACY.md
- **Website:** https://github.com/alxbdn/win11-desktop-switcher
- **Support contact:** https://github.com/alxbdn/win11-desktop-switcher/issues

## Age ratings

Answer "No" to every content question (no violence, chat, user-generated content, purchases, location…). The result is 3+ / Everyone.

## Packages

Upload `EzDesk.msix` from the GitHub Actions artifact **EzDesk-store-msix**, built with your real identity.

### Restricted capability: `runFullTrust`

Partner Center asks you to justify it. Paste:

> EzDesk is a classic Win32 desktop app (the AutoHotkey v2 runtime running an open-source script). Full trust is required to register its global keyboard shortcuts (Alt+1…9 to switch virtual desktops, Alt+Shift+1…9 to move the active window), to react to the mouse wheel over the taskbar, to show its notification-area icon with the current desktop number, and to call the Windows virtual desktop APIs through the open-source VirtualDesktopAccessor library. It does not access the network, user files or personal data. Source: https://github.com/alxbdn/win11-desktop-switcher

## Store listing (English)

**Product name:** EzDesk (reserved; Store ID `9MV9P36W441C`). It must match `DisplayName` in `store/identity.json`. Earlier name: Desktop Switcher - Virtual Desktop Shortcuts.

**Short description**

> Switch virtual desktops in one shortcut, and always see which one you're on.

**Description**

> Windows has virtual desktops, but no shortcut to jump straight to a given desktop, and no way to see which one you're on. EzDesk adds both.
>
> • Alt + 1…9: go straight to desktop N, or create it if it doesn't exist yet
> • Alt + Shift + 1…9: send the active window to desktop N (a new one if needed) and follow it
> • Scroll on the taskbar: previous / next desktop. Scrolling over the volume icon still changes the volume.
> • A tray icon shows the current desktop's number. Click it for Task View.
>
> Organize your work the way you think: one desktop for meetings, one for email and the web, one for code. Switch between them as fast as you type.
>
> Works on Windows 11 and Windows 10, on any keyboard layout (QWERTY, AZERTY, QWERTZ…). Free and open source (MIT). No account, no tracking, no internet connection.
>
> Tip: if you don't see the numbered icon, click the ^ arrow next to the clock and drag the icon onto the taskbar.

**Features** (one per line in Partner Center)

> Alt + 1…9 to jump to any virtual desktop, or create a new one
> Alt + Shift + 1…9 to move the active window to another desktop, even a new one
> Scroll on the taskbar to step through desktops
> Tray icon with the current desktop number
> Starts with Windows (can be turned off in Settings › Apps › Startup)
> Works with any keyboard layout
> Free, open source, no tracking

**Keywords** (up to 7): virtual desktop, desktop switcher, workspaces, productivity, keyboard shortcuts, multitasking, window manager

**Screenshots:** at least 1, 1366 × 768 or larger (PNG). Real screenshots of your taskbar with the numbered icon work best, or frames from the promo video.

**What's new in this version**

> Desktop Switcher is now EzDesk: a new name and a new icon. Same shortcuts, same app, nothing to set up again.

**Logo for the listing:** `brand/ezdesk-1080.png` (1:1 box art). Larger sizes come from `brand/ezdesk.svg`.
