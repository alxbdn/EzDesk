https://github.com/user-attachments/assets/3f30b27d-8ac1-476c-b2e6-5dcabe5aa908

# win11-desktop-switcher

Fast virtual-desktop switching for Windows 11, with a tray icon that shows which desktop you're on. Built on [AutoHotkey v2](https://www.autohotkey.com) and [VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor).

![Tray icons](docs/icons.png)

## Install (2 minutes, no admin, no console)

1. **[Download DesktopSwitcher.zip](https://github.com/alxbdn/win11-desktop-switcher/releases/latest/download/DesktopSwitcher.zip)**
2. Right-click the zip → **Properties** → tick **Unblock** → **OK**
3. Right-click the zip → **Extract All…** → pick your **Documents** folder → **Extract**
4. Open the `DesktopSwitcher` folder and double-click **`DesktopSwitcher.exe`**
5. No numbered icon near the clock? Click the **^** arrow next to it and drag the icon onto the taskbar
6. Right-click the icon → **Start with Windows**, so it starts by itself next time

> **"Windows protected your PC"?** Click **More info** → **Run anyway**. The exe isn't code-signed, so Windows doesn't recognize it. `DesktopSwitcher.exe` is the official AutoHotkey v2 runtime, renamed, and the zip is built by [GitHub Actions](.github/workflows/release.yml) from the code in this repo. Step 2 usually prevents the warning.

Keep the folder where you extracted it: "Start with Windows" points to it. If you move the folder, start `DesktopSwitcher.exe` once from the new place and the shortcut follows.

## Shortcuts

| Action | Effect |
|---|---|
| `Alt + 1…9` | Go to desktop N (does nothing if it doesn't exist) |
| `Alt + Shift + 1…9` | Move the active window to desktop N and follow it |
| Scroll on the taskbar | Previous / next desktop |
| Scroll on the tray (volume, clock…) | Normal Windows behavior, volume still works |
| Click the tray icon | Task View |
| Right-click the tray icon | Start with Windows, Help, Restart, Exit |

## Uninstall

Right-click the tray icon → untick **Start with Windows** → **Exit**, then delete the folder.

## Power users: admin mode

Windows won't let a normal program receive keystrokes meant for an admin window. In the simple install, `Alt + 1…9` therefore doesn't work while Task Manager, an installer or an admin terminal is the active window. Scrolling on the taskbar still works.

Admin mode fixes that. It copies the app to `C:\Program Files\DesktopSwitcher\`, makes that folder admin-only for writing, and registers a **Desktop Switcher** scheduled task that starts it at logon with admin rights, with no UAC prompt each time.

- **From the zip:** double-click `admin\Install admin mode.cmd` (one UAC prompt). Remove it with `admin\Uninstall admin mode.cmd`.
- **From a clone of this repo:** `powershell -ExecutionPolicy Bypass -File .\admin\install.ps1`. It installs AutoHotkey with winget if needed and downloads the DLL (SHA256-checked).

It also removes the simple install's startup shortcut, so only one copy runs.

The folder is locked because the app runs as admin, and so does everything it executes or loads. Otherwise any program running as you could replace one of those files and gain admin rights at your next login. In admin mode, edit `C:\Program Files\DesktopSwitcher\DesktopSwitcher.ahk` from an editor opened as admin, then use **Restart** in the tray menu. Or edit your copy and run the installer again.

## Build the zip yourself

```powershell
powershell -ExecutionPolicy Bypass -File .\build-release.ps1 -Version 1.0.0
```

It downloads pinned versions of AutoHotkey and VirtualDesktopAccessor, checks their SHA256, generates the icons with `make-icons.ps1` and writes `dist\DesktopSwitcher.zip`. Pushing a `v*` tag runs the same build on GitHub Actions and publishes the release.

## Troubleshooting

- **"DLL may not match this Windows build" after a Windows update:** VirtualDesktopAccessor relies on undocumented Windows internals that sometimes change between builds. Check its [releases](https://github.com/Ciantic/VirtualDesktopAccessor/releases) for a newer DLL. Update the URL and SHA256 in `build-release.ps1` and `admin\install.ps1`.
- **Can't find it after double-clicking:** it's running, but Windows put the icon under the `^` arrow next to the clock. Drag it onto the taskbar. Double-clicking the exe again just restarts it.

Tested on Windows 11 26H2 (build 26300).

## License

[MIT](LICENSE). The release zip bundles AutoHotkey (GPLv2) and VirtualDesktopAccessor (MIT), unmodified: see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
