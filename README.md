# win11-desktop-switcher

Fast virtual-desktop switching for Windows 11, with a tray icon that shows which desktop you're on. Built on [AutoHotkey v2](https://www.autohotkey.com) and [VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor).

![Tray icons](docs/icons.png)

## Shortcuts

| Action | Effect |
|---|---|
| `Alt + 1…9` | Go to desktop N (does nothing if it doesn't exist) |
| `Alt + Shift + 1…9` | Move the active window to desktop N and follow it |
| Scroll on the taskbar | Previous / next desktop |
| Scroll on the tray (volume, clock…) | Normal Windows behavior, volume still works |
| Click the tray icon | Task View |
| Hover the tray icon | "Desktop 2 of 3", plus the desktop's name if it has one |

## Install

Download or clone the repo, then from its folder:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

You'll get one UAC prompt. The installer:

1. Installs AutoHotkey v2 with winget if it's missing
2. Copies everything to `C:\Program Files\DesktopSwitcher\`, including its own copy of `AutoHotkey64.exe`
3. Downloads `VirtualDesktopAccessor.dll` and checks its SHA256
4. Generates the tray icons
5. Makes the folder admin-only for writing
6. Registers a **Desktop Switcher** scheduled task that starts it at logon with admin rights, then starts it

Run it again at any time to update or repair. Options: `-InstallDir <path>`, `-Outline 0|1|2` (icon border).

**Uninstall:** `powershell -ExecutionPolicy Bypass -File .\uninstall.ps1`. AutoHotkey itself stays installed.

## Why admin rights?

Windows doesn't let a normal program send keystrokes to an admin window, so without admin rights the shortcuts stop working over Task Manager, installers or an admin terminal. The scheduled task starts the script with admin rights at logon, without a UAC prompt each time.

Because it runs as admin, everything it executes or loads lives in `Program Files` and is admin-only for writing: the script, `AutoHotkey64.exe` and the DLL. Otherwise any program running as you could replace one of those files and gain admin rights at your next login.

## Customizing

Edit `C:\Program Files\DesktopSwitcher\DesktopSwitcher.ahk` in an editor opened as admin, then right-click the tray icon → **Reload Script**. Or edit the copy in this repo and run `install.ps1` again.

## Troubleshooting

- **"DLL may not match this Windows build" error after a Windows update:** VirtualDesktopAccessor uses undocumented Windows internals that sometimes change between builds. Check its [releases](https://github.com/Ciantic/VirtualDesktopAccessor/releases) for a newer DLL. Update `$DllUrl` and `$DllSha256` in `install.ps1`, then run it again.
- **Tray icon missing:** look in the `^` overflow next to the clock and drag the icon onto the taskbar.
- **Shortcuts don't work over admin windows:** the script was started directly instead of through the task. Exit it from the tray and run `Start-ScheduledTask 'Desktop Switcher'`.

Tested on Windows 11 26H2 (build 26300).

## License

[MIT](LICENSE). VirtualDesktopAccessor is MIT-licensed by Jari Pennanen. AutoHotkey is GPLv2. Neither is included in this repo: the installer downloads them.
