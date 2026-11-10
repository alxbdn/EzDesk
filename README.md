https://github.com/user-attachments/assets/f2da5e65-471d-4356-8566-0b6103217ed0

https://github.com/user-attachments/assets/49fcb27b-0a43-4b74-a364-c29653e6bf3f

**[Get it from the Microsoft Store](https://apps.microsoft.com/detail/9MV9P36W441C)** · or [download the zip](https://github.com/alxbdn/ezdesk/releases/latest/download/EzDesk.zip)

<img src="brand/ezdesk.svg" width="96" alt="EzDesk icon">

# EzDesk

Move windows between virtual desktops with the mouse alone, on Windows 11, with a tray icon that shows which desktop you're on. No keyboard shortcuts to learn. Formerly called Desktop Switcher. It also runs on Windows 10. Built on [AutoHotkey v2](https://www.autohotkey.com) and [VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor).

![Tray icons](docs/icons.png)

## Install

### From the Microsoft Store (recommended)

**[Get EzDesk from the Microsoft Store](https://apps.microsoft.com/detail/9MV9P36W441C)**, click **Get**, then **Open**. It's signed by Microsoft, so there's no warning, and it updates itself.

It starts with Windows; you can turn that off in **Settings › Apps › Startup**. No numbered icon near the clock? Click the **^** arrow next to it and drag the icon onto the taskbar.

### From the zip (no Store, no admin, no console)

1. **[Download EzDesk.zip](https://github.com/alxbdn/ezdesk/releases/latest/download/EzDesk.zip)**
2. Right-click the zip → **Properties** → tick **Unblock** → **OK**
3. Right-click the zip → **Extract All…** → pick your **Documents** folder → **Extract**
4. Open the `EzDesk` folder and double-click **`EzDesk.exe`**
5. No numbered icon near the clock? Click the **^** arrow next to it and drag the icon onto the taskbar
6. Right-click the icon → **Start with Windows**, so it starts by itself next time

> **"Windows protected your PC"?** Click **More info** → **Run anyway**. The exe isn't code-signed, so Windows doesn't recognize it. `EzDesk.exe` is the official AutoHotkey v2 runtime, renamed and given the app's own icon and name, and the zip is built by [GitHub Actions](.github/workflows/release.yml) from the code in this repo. Step 2 usually prevents the warning.

Keep the folder where you extracted it: "Start with Windows" points to it. If you move the folder, start `EzDesk.exe` once from the new place and the shortcut follows.

## How it works

Everything is done with the mouse.

| Action | Effect |
|---|---|
| Grab a window by its title bar (moving it or not) | A dock of your desktops rises at the bottom of the screen, a dark rounded panel ringed with a flowing blue glow, each desktop a live miniature, like the Spaces bar on a Mac (your windows don't move around as in Task View). The window you hold glows too. The dock stays in front of everything |
| Bring it over the dock | The dock turns see-through, so you see what you're carrying under it, and the window shrinks to a small picture of itself, like on a Mac; both come back when it leaves |
| Drop it on a desktop in the dock | It goes there, and you stay where you are. On Windows 11, **+** makes a new desktop |
| Keep it over a desktop in the dock for a second | You slide to that desktop, the window still in your hand |
| Bring it into the glowing bump on the left or right of the screen | Hold it there: you slide to the previous / next desktop with the window in hand (a new one after the last), one every 0.75 s |
| Drop it anywhere else | An ordinary move; the dock goes away |
| Scroll on the taskbar | Previous / next desktop |
| Scroll on the tray (volume, clock…) | Normal Windows behavior, volume still works |
| Click the tray icon (the desktop number) | Opens the dock without a window in hand: click a desktop to go there, its **×** to close it (Windows 11), **+** to make one; grab any window by its title bar and it glows at once, drop it on a desktop to send it there. Click anywhere else, or the number again, to close the dock. Task View is in the icon's menu |
| Right-click the tray icon | Hold a window to move it between desktops, Performance mode, show apps from all desktops on the taskbar, Start with Windows, Help, Restart, Exit |

**Performance mode** (tray menu): on a slow PC, EzDesk already lightens its animations by itself while the PC is struggling, and goes back to full ones when it recovers. Tick this to keep them light all the time.

Windows' Snap keeps working while the dock is open, on any desktop: let go against the edge before the bump fills (0.75 s) and the window snaps to half the screen as usual.

The first time you switch desktops while the taskbar only shows the current desktop's apps, EzDesk offers, in a notification, to show them all: click it and it's done (the Store version opens the Settings page where it's one switch).

**Taskbar: show apps from all desktops** (tray menu) turns on Windows' own setting, so the taskbar marks every open app on every desktop, not only the ones on the current desktop. Clicking an app that's on another desktop takes you there. In the Store version, it opens the Settings page (Multitasking › Desktops) where you change it.

EzDesk 1.3 dropped the keyboard shortcuts of earlier versions (`Alt + 1…9`, `Alt + Shift + 1…9`): it's mouse-first now.

## Uninstall

- **Store version:** Settings › Apps › Installed apps → EzDesk (Desktop Switcher in older versions) → **Uninstall**.
- **Zip version:** right-click the tray icon → untick **Start with Windows** → **Exit**, then delete the folder.

Don't keep both installed: they'd both react to the same mouse moves.

## Power users: admin mode

Windows won't let a normal program act on admin windows. In the simple install, holding Task Manager, an installer or an admin terminal therefore doesn't open the dock. Scrolling on the taskbar still works.

Admin mode fixes that. It copies the app to `C:\Program Files\EzDesk\`, makes that folder admin-only for writing, and registers an **EzDesk** scheduled task that starts it at logon with admin rights, with no UAC prompt each time.

- **From the zip:** double-click `admin\Install admin mode.cmd` (one UAC prompt). Remove it with `admin\Uninstall admin mode.cmd`.
- **From a clone of this repo:** `powershell -ExecutionPolicy Bypass -File .\admin\install.ps1`. It installs AutoHotkey with winget if needed and downloads the DLL (SHA256-checked).

It also removes the simple install's startup shortcut, so only one copy runs. Installing over an admin mode from before the rename (`C:\Program Files\DesktopSwitcher\` and its "Desktop Switcher" task) replaces it.

The folder is locked because the app runs as admin, and so does everything it executes or loads. Otherwise any program running as you could replace one of those files and gain admin rights at your next login. In admin mode, edit `C:\Program Files\EzDesk\EzDesk.ahk` from an editor opened as admin, then use **Restart** in the tray menu. Or edit your copy and run the installer again.

## Build the zip yourself

```powershell
powershell -ExecutionPolicy Bypass -File .\build-release.ps1 -Version 1.0.0
```

It downloads pinned versions of AutoHotkey and VirtualDesktopAccessor, checks their SHA256, generates the icons with `make-icons.ps1` and writes `dist\EzDesk.zip`. Pushing a `v*` tag runs the same build on GitHub Actions and publishes the release.

The code is `EzDesk.ahk` plus three modules in `lib\`: `Desktops.ahk` (Windows' virtual desktops), `Hold.ahk` (the hold gesture, from the press to the drop) and `Glow.ahk` (the light round the dock and the bumps). Their tests run without touching your desktops or your screen:

```powershell
dist\EzDesk\EzDesk.exe /ErrorStdOut tests\run-tests.ahk
```

## Troubleshooting

- **"DLL may not match this Windows build" after a Windows update:** VirtualDesktopAccessor relies on undocumented Windows internals that sometimes change between builds. Check its [releases](https://github.com/Ciantic/VirtualDesktopAccessor/releases) for a newer DLL. Update the URL and SHA256 in `build-release.ps1` and `admin\install.ps1`.
- **Tooltip says "waiting for Windows virtual desktops":** Windows isn't answering yet, which is normal for a few seconds after login. If it stays that way, use **Restart** from the icon's menu. Before v1.0.1, this showed as a green "H" icon that couldn't be closed.
- **Can't find it after double-clicking:** it's running, but Windows put the icon under the `^` arrow next to the clock. Windows doesn't let apps show their own icon, so each time it starts, if the icon is hidden, EzDesk says so in a notification: click it to open the Settings page where you turn it on (Windows 11: Taskbar › Other system tray icons). Don't want it? Untick **Remind me when this icon is hidden** in the tray menu; it also stops by itself after three reminders closed without a click. You can also drag it out of `^` onto the taskbar. Double-clicking the exe again just restarts it.
Tested on Windows 11 26H2 (build 26300).

## License

[MIT](LICENSE). The release zip bundles AutoHotkey (GPLv2) and VirtualDesktopAccessor (MIT), unmodified: see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
