# Third-party notices

The release zip bundles two third-party components.

## AutoHotkey v2 (`EzDesk.exe`)

`EzDesk.exe` is `AutoHotkey64.exe` from [AutoHotkey v2.0.31](https://github.com/AutoHotkey/AutoHotkey/releases/tag/v2.0.31), renamed so that it runs `EzDesk.ahk` when double-clicked. The build (`rebrand-exe.ps1`) also replaces two resources in it, so Windows shows the app's own name and icon: the main icon and the version information. The program code itself is unchanged. It's licensed under the GNU General Public License v2: see `LICENSE-AutoHotkey.txt` in the zip. Source code: https://github.com/AutoHotkey/AutoHotkey

## VirtualDesktopAccessor (`VirtualDesktopAccessor.dll`, `VirtualDesktopAccessor-win10.dll`)

From [Ciantic/VirtualDesktopAccessor](https://github.com/Ciantic/VirtualDesktopAccessor): release `2024-12-16-windows11` for Windows 11, and release `2019-windows10`, renamed `VirtualDesktopAccessor-win10.dll`, for Windows 10. Same author and license.

```
MIT License

Copyright (c) 2015-2023 Jari Otto Oskari Pennanen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
