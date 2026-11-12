https://github.com/user-attachments/assets/f2da5e65-471d-4356-8566-0b6103217ed0

https://github.com/user-attachments/assets/49fcb27b-0a43-4b74-a364-c29653e6bf3f

**[Get it from the Microsoft Store](https://apps.microsoft.com/detail/9MV9P36W441C)** · or [download the zip](https://github.com/alxbdn/ezdesk/releases/latest/download/EzDesk.zip)

<img src="brand/ezdesk.svg" width="96" alt="EzDesk icon">

# EzDesk

Move windows between virtual desktops with the mouse alone, on Windows 11, with a tray icon that shows which desktop you're on. No keyboard shortcuts to learn. Formerly called Desktop Switcher. It also runs on Windows 10. Written in Rust, on top of [VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor): compiled in on Windows 11 24H2 and later, its DLL on older Windows.

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

> **"Windows protected your PC"?** Click **More info** → **Run anyway**. The exe isn't code-signed, so Windows doesn't recognize it. The zip is built by [GitHub Actions](.github/workflows/release.yml) from the code in this repo. Step 2 usually prevents the warning.

Keep the folder where you extracted it: "Start with Windows" points to it. If you move the folder, start `EzDesk.exe` once from the new place and it follows.

## How it works

Everything is done with the mouse.

| Action | Effect |
|---|---|
| Hold a window by its title bar for a moment (moving it or not) | A glowing bump rises on the left and right of the screen, half-way down. The window you hold glows too (Windows 11) |
| Bring it into a bump and hold it there | The bump fills with light, then you slide to the previous / next desktop with the window in hand, one every 0.75 s. Past the last desktop, the right bump shows a **+** and makes a new one |
| Let go | The window stays on the desktop you're on; the bumps go away |
| Scroll on the taskbar | Previous / next desktop |
| Scroll on the tray (volume, clock…) | Normal Windows behavior, volume still works |
| Rest the mouse on the tray icon (the desktop number) | A mini-dock of your desktops opens next to it, like the one on Windows 11's Task View button: each desktop's name and a live preview, the current one ringed. Click one to go there, its **×** to close it, **+** for a new one, drag one sideways to reorder them (Windows 11 24H2+); right-click one to rename it or choose its background. What's under the mouse lights up in EzDesk's flowing blue-to-white light. Windows 10: go there and **+** only |
| Click the tray icon | Task View |
| Right-click the tray icon | Hold a window to move it between desktops, show apps from all desktops on the taskbar, Start with Windows, Help, Restart, Exit |

Windows' Snap keeps working: let go against the edge before the bump fills (0.75 s) and the window snaps to half the screen as usual.

The first time you switch desktops while the taskbar only shows the current desktop's apps, EzDesk offers, in a notification, to show them all: click it and it's done (the Store version opens the Settings page where it's one switch).

**Taskbar: show apps from all desktops** (tray menu) turns on Windows' own setting, so the taskbar marks every open app on every desktop, not only the ones on the current desktop. Clicking an app that's on another desktop takes you there. In the Store version, it opens the Settings page (Multitasking › Desktops) where you change it.

EzDesk 1.3 dropped the keyboard shortcuts of earlier versions (`Alt + 1…9`, `Alt + Shift + 1…9`), and 1.4 its dock of desktops: it's the bumps and the taskbar now.

## Uninstall

- **Store version:** Settings › Apps › Installed apps → EzDesk (Desktop Switcher in older versions) → **Uninstall**.
- **Zip version:** right-click the tray icon → untick **Start with Windows** → **Exit**, then delete the folder.

Don't keep both installed: they'd both react to the same mouse moves.

## Power users: admin mode

Windows won't let a normal program act on admin windows. In the simple install, holding Task Manager, an installer or an admin terminal therefore doesn't bring up the bumps. Scrolling on the taskbar still works.

Admin mode fixes that. It copies the app to `C:\Program Files\EzDesk\`, makes that folder admin-only for writing, and registers an **EzDesk** scheduled task that starts it at logon with admin rights, with no UAC prompt each time.

- **From the zip:** double-click `admin\Install admin mode.cmd` (one UAC prompt). Remove it with `admin\Uninstall admin mode.cmd`.
- **From a clone of this repo:** `cargo build --release`, then `powershell -ExecutionPolicy Bypass -File .\admin\install.ps1`. It downloads the DLLs (SHA256-checked).

It also turns off the simple install's "Start with Windows", so only one copy runs. Installing over an admin mode from before the rename (`C:\Program Files\DesktopSwitcher\` and its "Desktop Switcher" task), or from the AutoHotkey versions (1.3 and earlier), replaces it.

The folder is locked because the app runs as admin, and so does everything it loads. Otherwise any program running as you could replace one of those files and gain admin rights at your next login.

## Build it yourself

You need [Rust](https://rustup.rs). With the MSVC toolchain and the Windows SDK, the exe also gets its own icon and version information.

```powershell
cargo test
powershell -ExecutionPolicy Bypass -File .\build-release.ps1 -Version 1.4.0
```

The script builds `EzDesk.exe`, downloads pinned versions of VirtualDesktopAccessor, checks their SHA256, generates the icons with `make-icons.ps1` and writes `dist\EzDesk.zip`. Pushing a `v*` tag runs the same build on GitHub Actions and publishes the release.

The code, in `src\`: `hold.rs` (the hold gesture, from the press to the drop, as intents), `desktops.rs` (Windows' virtual desktops behind one seam, with two adapters: `native.rs`, compiled in, for Windows 11 24H2+, and `vda.rs`, the DLL, for older Windows), `held.rs` (the window in hand), `glow.rs` and `bumps.rs` (the light), `tray.rs`, `hooks.rs` (the mouse), `system.rs` (settings, startup, taskbar) and `main.rs`, which wires them together. The tests (`cargo test`) never touch your desktops or your screen.

## Troubleshooting

- **"Not compatible with this Windows" after a Windows update:** VirtualDesktopAccessor relies on undocumented Windows internals that sometimes change between builds. EzDesk tries it in a separate process first (compiled in, then the DLL), so a mismatch doesn't crash it: the icon stays, moving windows between desktops is off. Check its [releases](https://github.com/Ciantic/VirtualDesktopAccessor/releases) for a newer `winvd` (`Cargo.toml`) or DLL (URL and SHA256 in `build-release.ps1` and `admin\install.ps1`).
- **Tooltip says "waiting for Windows virtual desktops":** Windows isn't answering yet, which is normal for a few seconds after login. If it stays that way, use **Restart** from the icon's menu.
- **Can't find it after double-clicking:** it's running, but Windows put the icon under the `^` arrow next to the clock. Windows doesn't let apps show their own icon, so each time it starts, if the icon is hidden, EzDesk says so in a notification: click it to open the Settings page where you turn it on (Windows 11: Taskbar › Other system tray icons). Don't want it? Untick **Remind me when this icon is hidden** in the tray menu; it also stops by itself after three reminders closed without a click. You can also drag it out of `^` onto the taskbar. Double-clicking the exe again just restarts it.
- **Something else:** `%TEMP%\EzDesk-debug.log` says what went wrong at start-up.

Tested on Windows 11 26H2 (build 26300).

## License

[MIT](LICENSE). The exe includes VirtualDesktopAccessor's `winvd` (MIT) and Microsoft's windows-rs (MIT), and the release zip bundles VirtualDesktopAccessor's DLLs (MIT), unmodified, for older Windows: see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
