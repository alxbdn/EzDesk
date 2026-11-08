#Requires AutoHotkey v2.0
#SingleInstance Force
Persistent

; =====================================================================
;  EzDesk (formerly Desktop Switcher)
;    EzDesk.exe is the AutoHotkey v2 runtime, renamed: it runs EzDesk.ahk next to it
;    Alt + 1…9          go to desktop N
;    Alt + Shift + 1…9  move the active window to desktop N (and follow it)
;    Scroll on taskbar  previous / next desktop
;    Tray icon          shows the current desktop number; click = Task View
; =====================================================================
;
; Start-up order matters: icon, menu and hotkeys are set up first, and the
; virtual-desktop DLL is only called afterwards, from a timer. If Windows'
; virtual desktops aren't reachable yet (login, Explorer restart) the app
; still shows its own icon, its menu still works and Exit still exits.

; Our icon right away, never AutoHotkey's green "H".
TraySetIcon(A_ScriptDir "\icons\1.ico")
A_IconTip := "EzDesk (starting…)"

; ---------------------------------------------------------------------
;  VirtualDesktopAccessor: Windows 11 and Windows 10 need different builds
; ---------------------------------------------------------------------
global WIN_BUILD := Integer(StrSplit(A_OSVersion, ".")[3])
global VDA  := A_ScriptDir (WIN_BUILD >= 22000 ? "\VirtualDesktopAccessor.dll" : "\VirtualDesktopAccessor-win10.dll")
global hVDA := DllCall("LoadLibrary", "Str", VDA, "Ptr")
if !hVDA {
    MsgBox("Could not load " VDA, "EzDesk", "Iconx")
    ExitApp(1)
}

Proc(name, required := true) {
    p := DllCall("GetProcAddress", "Ptr", hVDA, "AStr", name, "Ptr")
    if (!p && required) {
        MsgBox("VirtualDesktopAccessor.dll has no " name "().`nIt may not match this Windows build (" WIN_BUILD ").", "EzDesk", "Iconx")
        ExitApp(1)
    }
    return p
}

global pCount   := Proc("GetDesktopCount")
global pCurrent := Proc("GetCurrentDesktopNumber")
global pGoTo    := Proc("GoToDesktopNumber")
global pMove    := Proc("MoveWindowToDesktopNumber")
global pOnCur   := Proc("IsWindowOnCurrentVirtualDesktop")
global pHook    := Proc("RegisterPostMessageHook")
global pName    := Proc("GetDesktopName", false)    ; Windows 11 build only
global pCreate  := Proc("CreateDesktop", false)     ; Windows 11 build only (Windows 10: Ctrl+Win+D)

; Every call into the DLL goes through VdaCall(). A DLL that doesn't match this Windows build crashes
; (access violation 0xC0000005) instead of returning an error. Without this, AutoHotkey shows an
; error dialog every second and the app looks frozen. Three crashes and we stop calling it at all.
global vdaErrors := 0, vdaBroken := false

VdaCall(p, args*) {
    global vdaErrors, vdaBroken
    if vdaBroken
        return -1
    try return DllCall(p, args*)
    catch {
        vdaErrors += 1
        if (vdaErrors >= 3 && !vdaBroken) {
            vdaBroken := true
            A_IconTip := "EzDesk: not compatible with this Windows (build " WIN_BUILD ")"
            TrayTip("The virtual-desktop library doesn't work on this Windows version (build " WIN_BUILD ").`nShortcuts are disabled. See Help in this icon's menu.", "EzDesk", "Iconx Mute")
        }
        return -1
    }
}

; The DLL counts desktops from 0; everything the user sees counts from 1.
DesktopCount()   => VdaCall(pCount, "Int")
CurrentDesktop() => VdaCall(pCurrent, "Int")

DesktopName(i) {
    if !pName
        return ""
    buf := Buffer(1024, 0)
    if (VdaCall(pName, "Int", i, "Ptr", buf, "UPtr", buf.Size, "Int") = -1 && vdaBroken)
        return ""
    return StrGet(buf, "UTF-8")                     ; "" when the desktop has no custom name
}

; ---------------------------------------------------------------------
;  Switching
; ---------------------------------------------------------------------
; Returns the 0-based desktop to use when you press Alt+N (n = 1…9), or -1 to do nothing.
; Used by both "go to" and "move window to". If desktop n doesn't exist yet, ONE new desktop is
; created at the end, whatever n is: with 2 desktops, Alt+4 creates desktop 3, not 3 and 4.
ResolveTarget(n) {
    count := DesktopCount()
    if (count <= 0)
        return -1
    if (n <= count)
        return n - 1
    return NewDesktop(count)
}

; Creates one desktop at the end; returns its 0-based index, or -1 if it didn't appear.
; Success is read from the desktop count, not from the call's return value.
NewDesktop(count) {
    if pCreate
        VdaCall(pCreate, "Int")
    else
        Send("^#d")                                 ; Windows 10 DLL can't create desktops: Windows' own shortcut does (and switches to it)
    deadline := A_TickCount + 1500
    while (A_TickCount < deadline) {
        if (DesktopCount() > count)
            return count
        Sleep(50)
    }
    return -1
}

; Switches desktop without making taskbar buttons flash.
; When the active window vanishes with its desktop, Windows tries to activate a window on the
; new one; that request doesn't come from the user, so it's refused and the button flashes
; instead ("wants attention"). Parking focus on the taskbar first, then focusing the top window
; of the new desktop ourselves, avoids it.
SwitchTo(i, focus := true) {
    try WinActivate("ahk_class Shell_TrayWnd")
    VdaCall(pGoTo, "Int", i, "Int")
    if focus
        FocusTopWindow()
}

FocusTopWindow() {
    for hwnd in WinGetList() {                      ; top-most first
        if IsRealWindow(hwnd) && VdaCall(pOnCur, "Ptr", hwnd, "Int") = 1 {
            try WinActivate(hwnd)
            return
        }
    }
}

IsRealWindow(hwnd) {
    try {
        style := WinGetStyle(hwnd), ex := WinGetExStyle(hwnd), cls := WinGetClass(hwnd)
        if !(style & 0x10000000) || (style & 0x20000000) || (ex & 0x80)      ; hidden, minimized, tool window
            return false
        if (cls = "Shell_TrayWnd" || cls = "Shell_SecondaryTrayWnd" || cls = "Progman" || cls = "WorkerW")
            return false
        return WinGetTitle(hwnd) != ""
    }
    return false
}

GoToDesktop(n) {
    i := ResolveTarget(n)
    if (i >= 0 && i != CurrentDesktop())
        SwitchTo(i)
}

MoveActiveToDesktop(n) {
    hwnd := WinExist("A")
    if !hwnd
        return
    cls := WinGetClass(hwnd)
    if (cls = "Shell_TrayWnd" || cls = "Progman" || cls = "WorkerW")   ; never move the taskbar or wallpaper
        return
    i := ResolveTarget(n)
    if (i < 0)
        return
    VdaCall(pMove, "Ptr", hwnd, "Int", i, "Int")
    SwitchTo(i, false)
    try WinActivate(hwnd)                           ; follow the moved window
}

Step(delta) {
    static last := 0
    if (A_TickCount - last < 250)                   ; touchpads send bursts of wheel events
        return
    last := A_TickCount
    i := CurrentDesktop() + delta
    if (i >= 0 && i < DesktopCount())
        SwitchTo(i)
}

; ---------------------------------------------------------------------
;  Tray menu and "Start with Windows"
; ---------------------------------------------------------------------
; Three ways to run, three ways to start with Windows:
;   Simple mode (zip)        a shortcut in the user's Startup folder (no admin needed)
;   Admin mode (install.ps1) a scheduled task: the item is shown checked and locked
;   Store app (MSIX)         a startup task declared in the package; Windows manages it in
;                            Settings > Apps > Startup (a packaged app can't write the real Startup folder)
global STARTUP_LNK := A_Startup "\EzDesk.lnk"
global OLD_STARTUP_LNK := A_Startup "\Desktop Switcher.lnk"     ; before the rename to EzDesk
STARTED_AT_LOGIN := A_Args.Length && A_Args[1] = "--startup"

; Running from an MSIX package? GetCurrentPackageFullName returns APPMODEL_ERROR_NO_PACKAGE (15700) if not.
IsPackaged() {
    len := 0
    return DllCall("GetCurrentPackageFullName", "UInt*", &len, "Ptr", 0, "Int") != 15700
}
global PACKAGED := IsPackaged()

IsStartupOn() => FileExist(STARTUP_LNK) != ""

SetStartup(on) {
    if on                                           ; (re)written each launch, so it follows the folder if it moves
        FileCreateShortcut(A_AhkPath, STARTUP_LNK, A_ScriptDir, '"' A_ScriptFullPath '" --startup', "EzDesk")   ; icon: the exe's own
    else if IsStartupOn()
        FileDelete(STARTUP_LNK)
}

ToggleStartup(itemName, *) {
    SetStartup(!IsStartupOn())
    IsStartupOn() ? A_TrayMenu.Check(itemName) : A_TrayMenu.Uncheck(itemName)
}

; Upgrading from Desktop Switcher: "Start with Windows" was on under the old shortcut name.
; Move it to the new name, pointing at this copy, so it stays on and only one copy starts.
if !PACKAGED && !A_IsAdmin && FileExist(OLD_STARTUP_LNK) {
    try FileDelete(OLD_STARTUP_LNK)
    SetStartup(true)
}

A_TrayMenu.Delete()
A_TrayMenu.Add("Task View", (*) => Send("#{Tab}"))
A_TrayMenu.Add()
if PACKAGED {
    A_TrayMenu.Add("Start with Windows…", (*) => Run("ms-settings:startupapps"))
} else if A_IsAdmin {
    A_TrayMenu.Add("Starts with Windows (admin mode)", (*) => 0)
    A_TrayMenu.Check("Starts with Windows (admin mode)")
    A_TrayMenu.Disable("Starts with Windows (admin mode)")
} else {
    A_TrayMenu.Add("Start with Windows", ToggleStartup)
    if IsStartupOn() {
        SetStartup(true)
        A_TrayMenu.Check("Start with Windows")
    }
}
A_TrayMenu.Add("Help", (*) => Run("https://github.com/alxbdn/ezdesk#readme"))
A_TrayMenu.Add()
A_TrayMenu.Add("Restart", (*) => Reload())
A_TrayMenu.Add("Exit", (*) => ExitApp())
A_TrayMenu.Default := "Task View"
A_TrayMenu.ClickCount := 1                          ; single left-click opens Task View

; ---------------------------------------------------------------------
;  Hotkeys (registered before any call into the DLL)
; ---------------------------------------------------------------------
MakeGo(n) {
    return (*) => GoToDesktop(n)
}
MakeMove(n) {
    return (*) => (MaskAltShift(), MoveActiveToDesktop(n))
}
; Windows switches the keyboard layout when Alt+Shift is pressed and released with no other key
; in between. The hotkey swallows the number key, so Windows only sees Alt+Shift and cycles the
; layout. Sending an unassigned key (vkE8) while Alt+Shift are still down cancels that toggle.
MaskAltShift() {
    Send("{Blind}{vkE8}")
}
; Bound by physical key (scan codes sc002…sc00A = the 1…9 row), not by character.
; "!1" means "Alt + the character 1", and on AZERTY that character needs Shift: "!1" and "!+1"
; then collapse into the same Alt+Shift combo, so Alt+& did nothing and Alt+Shift+& moved the window.
Loop 9 {
    sc := Format("sc{:03X}", A_Index + 1)
    Hotkey("!" sc,  MakeGo(A_Index))
    Hotkey("!+" sc, MakeMove(A_Index))
}

MouseOverTaskbar() {
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y, &hwnd)
    try cls := WinGetClass(hwnd)
    catch
        return false
    if (cls != "Shell_TrayWnd" && cls != "Shell_SecondaryTrayWnd")
        return false
    return !OverTrayArea(hwnd, x, y)                ; the tray keeps its own scrolling (volume)
}

; True when (x, y) is inside the notification area: tray icons, network/volume, clock.
OverTrayArea(taskbar, x, y) {
    tray := DllCall("FindWindowEx", "Ptr", taskbar, "Ptr", 0, "Str", "TrayNotifyWnd", "Ptr", 0, "Ptr")
    if !tray
        return false
    rect := Buffer(16)
    DllCall("GetWindowRect", "Ptr", tray, "Ptr", rect)
    return x >= NumGet(rect, 0, "Int") && x < NumGet(rect, 8, "Int")
        && y >= NumGet(rect, 4, "Int") && y < NumGet(rect, 12, "Int")
}

#HotIf MouseOverTaskbar()
WheelUp::Step(-1)
WheelDown::Step(1)
#HotIf

; ---------------------------------------------------------------------
;  Tray indicator: connects to the DLL from a timer, retrying with back-off
; ---------------------------------------------------------------------
global shownIndex := -2, shownCount := -2
global connected := false, failures := 0, warned := false
global MSG_DESKTOP_CHANGED := 0x1400 + 30

UpdateTray(*) {
    global shownIndex, shownCount, connected, failures, warned
    i := CurrentDesktop(), c := DesktopCount()
    if vdaBroken {                                  ; incompatible DLL: stop polling, the menu keeps working
        SetTimer(UpdateTray, 0)
        return
    }
    if (i < 0 || c <= 0) {                          ; virtual desktops not reachable (yet)
        failures += 1
        A_IconTip := "EzDesk: waiting for Windows virtual desktops…"
        SetTimer(UpdateTray, Min(1000 * 2 ** Min(failures, 4), 10000))   ; back off: 2 s … 10 s
        if (failures = 6 && !warned) {
            warned := true
            TrayTip("Can't reach Windows' virtual desktops (Windows build " WIN_BUILD ").`nTry Restart from this icon's menu.", "EzDesk", "Iconx Mute")
        }
        return
    }
    if !connected {                                 ; first success: listen for switches, poll as a safety net
        connected := true, failures := 0
        VdaCall(pHook, "Ptr", A_ScriptHwnd, "Int", MSG_DESKTOP_CHANGED, "Int")
        OnMessage(MSG_DESKTOP_CHANGED, UpdateTray)
        SetTimer(UpdateTray, 1000)
    }
    if (i = shownIndex && c = shownCount)
        return
    shownIndex := i, shownCount := c
    n := i + 1
    TraySetIcon(A_ScriptDir "\icons\" (n <= 9 ? n : "more") ".ico")
    name := DesktopName(i)
    A_IconTip := "Desktop " n " of " c (name != "" ? " — " name : "")
}

; Welcome tip on the very first launch only. (The Store's startup task can't pass --startup,
; so "not started at login" isn't enough to avoid showing it at every logon.)
FirstRun() {
    flag := A_AppData "\EzDesk\welcome-shown"               ; per-package private copy when packaged
    if FileExist(flag) || FileExist(A_AppData "\DesktopSwitcher\welcome-shown")   ; already welcomed before the rename
        return false
    try {
        DirCreate(A_AppData "\EzDesk")
        FileAppend("", flag)
    }
    return true
}
if !STARTED_AT_LOGIN && FirstRun()
    TrayTip("Alt + 1…9 switches desktops.`nNo numbered icon near the clock? Click ^ and drag it onto the taskbar.", "EzDesk is running", "Mute")

SetTimer(UpdateTray, -100)                          ; first contact with the DLL happens after start-up is complete
