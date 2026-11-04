#Requires AutoHotkey v2.0
#SingleInstance Force
Persistent

; =====================================================================
;  Desktop Switcher
;    Alt + 1…9          go to desktop N
;    Alt + Shift + 1…9  move the active window to desktop N (and follow it)
;    Scroll on taskbar  previous / next desktop
;    Tray icon          shows the current desktop number; click = Task View
; =====================================================================

global VDA  := A_ScriptDir "\VirtualDesktopAccessor.dll"
global hVDA := DllCall("LoadLibrary", "Str", VDA, "Ptr")
if !hVDA {
    MsgBox("Could not load " VDA, "Desktop Switcher", "Iconx")
    ExitApp(1)
}

Proc(name) {
    p := DllCall("GetProcAddress", "Ptr", hVDA, "AStr", name, "Ptr")
    if !p {
        MsgBox("VirtualDesktopAccessor.dll has no " name "().`nIt may not match this Windows build.", "Desktop Switcher", "Iconx")
        ExitApp(1)
    }
    return p
}

global pCount   := Proc("GetDesktopCount")
global pCurrent := Proc("GetCurrentDesktopNumber")
global pGoTo    := Proc("GoToDesktopNumber")
global pMove    := Proc("MoveWindowToDesktopNumber")
global pCreate  := Proc("CreateDesktop")
global pName    := Proc("GetDesktopName")
global pHook    := Proc("RegisterPostMessageHook")
global pUnhook  := Proc("UnregisterPostMessageHook")

; The DLL counts desktops from 0; everything the user sees counts from 1.
DesktopCount()   => DllCall(pCount, "Int")
CurrentDesktop() => DllCall(pCurrent, "Int")
CreateDesktop()  => DllCall(pCreate, "Int")        ; returns the new desktop's 0-based index

DesktopName(i) {
    buf := Buffer(1024, 0)
    DllCall(pName, "Int", i, "Ptr", buf, "UPtr", buf.Size, "Int")
    return StrGet(buf, "UTF-8")                     ; "" when the desktop has no custom name
}

; ---------------------------------------------------------------------
;  Which desktop does "N" mean?
; ---------------------------------------------------------------------
; Returns the 0-based desktop to use when you press Alt+N (n = 1…9),
; or -1 to do nothing. Used by both "go to" and "move window to".
ResolveTarget(n) {
    count := DesktopCount()
    if (n <= count)
        return n - 1
    return -1                                       ; desktop n doesn't exist: do nothing (no surprise empty desktops)
}

GoToDesktop(n) {
    i := ResolveTarget(n)
    if (i >= 0 && i != CurrentDesktop())
        DllCall(pGoTo, "Int", i, "Int")
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
    DllCall(pMove, "Ptr", hwnd, "Int", i, "Int")
    DllCall(pGoTo, "Int", i, "Int")
    WinActivate(hwnd)
}

Step(delta) {
    static last := 0
    if (A_TickCount - last < 250)                   ; touchpads send bursts of wheel events
        return
    last := A_TickCount
    i := CurrentDesktop() + delta
    if (i >= 0 && i < DesktopCount())
        DllCall(pGoTo, "Int", i, "Int")
}

; ---------------------------------------------------------------------
;  Tray indicator
; ---------------------------------------------------------------------
global shownIndex := -2, shownCount := -2

UpdateTray(*) {
    global shownIndex, shownCount
    i := CurrentDesktop(), c := DesktopCount()
    if (i < 0 || c <= 0)                            ; Explorer not ready (login, restart): try again next tick
        return
    if (i = shownIndex && c = shownCount)
        return
    shownIndex := i, shownCount := c
    n := i + 1
    TraySetIcon(A_ScriptDir "\icons\" (n <= 9 ? n : "more") ".ico")
    name := DesktopName(i)
    A_IconTip := "Desktop " n " of " c (name != "" ? " — " name : "")
}

A_TrayMenu.Insert("1&", "Task View", (*) => Send("#{Tab}"))
A_TrayMenu.Insert("2&")
A_TrayMenu.Default := "Task View"
A_TrayMenu.ClickCount := 1                          ; single left-click opens Task View

MSG_DESKTOP_CHANGED := 0x1400 + 30
DllCall(pHook, "Ptr", A_ScriptHwnd, "Int", MSG_DESKTOP_CHANGED, "Int")
OnMessage(MSG_DESKTOP_CHANGED, UpdateTray)          ; instant update on every switch
SetTimer(UpdateTray, 1000)                          ; safety net: added/removed desktops, Explorer restarts
OnExit((*) => DllCall(pUnhook, "Ptr", A_ScriptHwnd, "Int"))
UpdateTray()

; ---------------------------------------------------------------------
;  Hotkeys
; ---------------------------------------------------------------------
MakeGo(n) {
    return (*) => GoToDesktop(n)
}
MakeMove(n) {
    return (*) => MoveActiveToDesktop(n)
}
Loop 9 {
    Hotkey("!" A_Index,  MakeGo(A_Index))
    Hotkey("!+" A_Index, MakeMove(A_Index))
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
