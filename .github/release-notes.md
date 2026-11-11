**Easiest: [get it from the Microsoft Store](https://apps.microsoft.com/detail/9MV9P36W441C)**: no warning, automatic updates.

**Or download `EzDesk.zip` below**, then:

1. Right-click the zip → **Properties** → tick **Unblock** → **OK**
2. Right-click the zip → **Extract All…** → pick your **Documents** folder
3. Open the folder and double-click **EzDesk.exe**
4. No numbered icon near the clock? Click **^** and drag it onto the taskbar
5. Right-click the icon → **Start with Windows**

**New in 1.4: rewritten in Rust.** One small exe, no script runtime, quicker to start. Hold a window by its title bar for a moment: a glowing bump rises on each side of the screen; bring the window into one and hold it there to slide to the next or previous desktop with it (past the last: a new one). Scrolling on the taskbar still steps through desktops. The dock of desktops from 1.3 is gone, and clicking the tray number now opens Task View. Updating from the zip: "Start with Windows" carries over by itself.

If Windows says *"Windows protected your PC"*: **More info** → **Run anyway**. The exe isn't code-signed. This zip was built by GitHub Actions from this repository's code; its SHA256 is in `EzDesk.zip.sha256`.

Power users: admin mode (also works with admin windows) is in the zip's `admin\` folder. See the [README](https://github.com/alxbdn/ezdesk#power-users-admin-mode).
