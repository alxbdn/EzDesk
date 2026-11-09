#Requires AutoHotkey v2.0
#SingleInstance Force
Persistent

; =====================================================================
;  EzDesk (formerly Desktop Switcher)
;    EzDesk.exe is the AutoHotkey v2 runtime, renamed: it runs EzDesk.ahk next to it
;    Mouse only, no keyboard shortcuts:
;    Hold a window still   the dock of desktops opens: drop it on one, or take it into
;                          the glowing bump on either side to slide to the next / previous
;    Scroll on taskbar     previous / next desktop
;    Tray icon             shows the current desktop number; click = Task View
; =====================================================================
;
; Start-up order matters: icon, menu and mouse hooks are set up first, and the
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
global pMove    := Proc("MoveWindowToDesktopNumber")
global pOnCur   := Proc("IsWindowOnCurrentVirtualDesktop")
global pHook    := Proc("RegisterPostMessageHook")
global pName    := Proc("GetDesktopName", false)    ; Windows 11 build only
global pCreate  := Proc("CreateDesktop", false)     ; Windows 11 build only (Windows 10: Ctrl+Win+D)
global pPin     := Proc("PinWindow", false)         ; drag to the screen edge needs these two
global pUnpin   := Proc("UnPinWindow", false)
global pWinDesk := Proc("GetWindowDesktopNumber", false)   ; the desktop bar's miniatures need it

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

; Focuses the top window of the current desktop (after the active one left for another desktop).
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

Step(delta) {
    static last := 0
    if (A_TickCount - last < 250)                   ; touchpads send bursts of wheel events
        return
    last := A_TickCount
    i := CurrentDesktop() + delta
    if (i >= 0 && i < DesktopCount())
        Send(delta < 0 ? "^#{Left}" : "^#{Right}")  ; Windows' own switch: it slides, where the DLL's is instant
}

; ---------------------------------------------------------------------
;  Holding a window: take it to another desktop
; ---------------------------------------------------------------------
; Windows announces the start and end of every window drag (EVENT_SYSTEM_MOVESIZESTART/END).
; While a window is held by its title bar, timers watch the cursor. Kept still for a moment, the
; dock opens (further down), with a glowing bump on each side of the screen: bring the window into
; one and hold it there, and you slide to the next / previous desktop with it, one every 0.75 s.
; The window is pinned (shown on every desktop) from the start of the drag, so switching neither
; hides it nor ends the drag; on release it's unpinned onto the desktop you're on.
; Switching uses Windows' own Ctrl+Win+arrow (Ctrl+Win+D for a new desktop): it slides like
; Windows does, where the DLL's switch is instant and blinks.
global SETTINGS := A_AppData "\EzDesk\settings.ini"         ; per-package private copy when packaged
global holdOn := IniRead(SETTINGS, "EzDesk", "HoldWindow", 1) = 1       ; the dock and its side bumps
global perfMode := IniRead(SETTINGS, "EzDesk", "Performance", 0) = 1    ; lighter animation, always
global dragHwnd := 0, edgeSide := 0, edgeSince := 0, dragPinned := false

; Pins the held window to every desktop, once. Done when the drag starts, not at the switch itself:
; Windows needs a moment to take a pin into account, and a window pinned just before the slide
; starts goes off with the old desktop and pops back after.
PinHeld(hwnd) {
    global dragPinned
    if (dragPinned || !hwnd)
        return
    VdaCall(pPin, "Ptr", hwnd, "Int")
    dragPinned := true
}
global dragFrom := {max: false, x: 0, y: 0, w: 0, h: 0}     ; where the window was picked up

OnMoveSize(hook, event, hwnd, idObject, idChild, thread, time) {
    global dragHwnd, edgeSide, edgeSince, dragPinned, dragFrom, stillX, stillY, stillSince
    if (idObject != 0)                              ; OBJID_WINDOW only
        return
    if (event = 0x000A) {                           ; drag started
        if !holdOn || !pPin || !pUnpin || !pWinDesk || !DraggedByTitle(hwnd)
            return
        dragHwnd := hwnd, edgeSide := 0, edgeSince := 0, dragPinned := false
        PinHeld(hwnd)                               ; ready to come along before any switch
        CoordMode("Mouse", "Screen")
        MouseGetPos(&stillX, &stillY), stillSince := A_TickCount   ; counts from the moment it's picked up
        try {
            WinGetPos(&fx, &fy, &fw, &fh, hwnd)
            dragFrom := {max: WinGetMinMax(hwnd) = 1, x: fx, y: fy, w: fw, h: fh}
        }
        SetTimer(WatchDrag, 15)                     ; often enough for smooth pulses
    } else if (event = 0x000B && hwnd = dragHwnd) { ; drag ended: mouse released
        SetTimer(WatchDrag, 0)
        target := BarTarget(), BarHide()            ; dropped on the desktop bar?
        pinned := dragPinned, from := dragFrom
        dragHwnd := 0, dragPinned := false
        if (target != -1)
            SetTimer(() => DropOnDesktop(hwnd, target, pinned, from), -1)   ; out of the event callback first
        else if pinned
            SetTimer(() => DropOnCurrent(hwnd), -1)
    }
}

; The window was carried across desktops: give it back to the desktop you ended up on.
; Moved first, then unpinned, so it never vanishes in between.
DropOnCurrent(hwnd) {
    i := CurrentDesktop()
    if (i >= 0)
        VdaCall(pMove, "Ptr", hwnd, "Int", i, "Int")
    VdaCall(pUnpin, "Ptr", hwnd, "Int")
    try WinActivate(hwnd)
}

; Dropped on the desktop bar (target: desktop index, or -2 for "+"): the window goes to that
; desktop, back where it was picked up, and you stay where you are, like on a Mac.
DropOnDesktop(hwnd, target, pinned, from) {
    if (target = -2)
        target := NewDesktop(DesktopCount())        ; only offered where it's made without going there
    try {
        if from.max {
            WinMaximize(hwnd)
        } else {
            if (WinGetMinMax(hwnd) = 1)             ; Windows maximized it, dropped near the top
                WinRestore(hwnd)
            WinMove(from.x, from.y, from.w, from.h, hwnd)
        }
    }
    if (target >= 0 && target != CurrentDesktop()) {
        VdaCall(pMove, "Ptr", hwnd, "Int", target, "Int")
        if pinned
            VdaCall(pUnpin, "Ptr", hwnd, "Int")
        FocusTopWindow()                            ; it left: the next window here gets the focus
    } else if pinned {
        DropOnCurrent(hwnd)
    }
}

; A drag by the title bar (or a custom one drawn in the client area), not a resize by a border.
DraggedByTitle(hwnd) {
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    try hit := SendMessage(0x84, 0, ((y & 0xFFFF) << 16) | (x & 0xFFFF), , hwnd, , , , 200)   ; WM_NCHITTEST
    catch
        return false
    return hit = 1 || hit = 2                       ; HTCLIENT, HTCAPTION
}

WatchDrag() {
    if !dragHwnd || !GetKeyState("LButton", "P")
        return
    if (HeldStill() && !barShown)
        BarShow(MonitorAt(stillX, stillY), dragHwnd)   ; held without moving: the dock and the bumps
    if barShown
        BarTrack()
}

; ---------------------------------------------------------------------
;  Glow: the flowing multicolour light round the dock and on the side bumps
; ---------------------------------------------------------------------
; Apple Intelligence style: purple, pink, blue, coral and orange, alive. Two layers of those colours
; slide at different speeds and in opposite directions; where they cross they mix, so the hues keep
; shifting instead of just sliding. Each shape is a layered window (per-pixel alpha) that clicks go
; through, never activated, not in the taskbar, and pinned to every desktop so it stays on screen
; through a desktop switch. A frame is a few GDI calls: colours, mixed, shaped by a precomputed mask.
; Careful with names: AutoHotkey ignores case, so "L" would be the same variable as "l".
global GLOW_COLORS := [0xBC82F3, 0xF5B9EA, 0x8D9FFF, 0xFF6778, 0xFFBA71, 0xC686FF]
global GLOW_FLOW_MS := 2600, GLOW_FLOW2_MS := 3700   ; time for each layer to slide one period

GlowPut(g, x, y, w, h, src, alpha) {
    DllCall("UpdateLayeredWindow", "Ptr", g.Hwnd, "Ptr", 0, "Int64*", (y << 32) | (x & 0xFFFFFFFF)
        , "Int64*", (h << 32) | w, "Ptr", src, "Int64*", 0, "UInt", 0
        , "UInt*", (1 << 24) | (alpha << 16), "UInt", 2)   ; BLENDFUNCTION: per-pixel alpha × alpha; ULW_ALPHA
}

; Brightness d pixels out from a shape's edge: a thin bright line on the edge inside a soft haze
; w wide, with a 1-pixel soft rim just inside. The two layers are mixed, not stacked, so the haze is
; lifted to what stacking them would give.
GlowFall(d, w) {
    static core := Max(2, Round(3 * A_ScreenDPI / 96))
    if (d <= -1 || d >= w)
        return 0
    rim := d < 0 ? 1 + d : 1, d := Max(d, 0), f := 1 - d / w
    v := Min(1, 0.9 * f ** 2.2 + (d < core ? 0.4 * (1 - d / core) : 0))
    return rim * v * (1 + 150 / 255 * (1 - v))
}

GlowPeriod2(h) => Round(h * 0.62)                   ; not a simple fraction of h: the mix never repeats the same way

; A premultiplied 32-bit memory DC, w × h, top-down; bits receives the pixel pointer.
NewDib(w, h, &bits := 0) {
    bits := 0                                       ; an omitted ByRef parameter starts unset
    bi := Buffer(40, 0)                             ; BITMAPINFOHEADER, 32 bpp
    NumPut("UInt", 40, bi, 0), NumPut("Int", w, bi, 4), NumPut("Int", -h, bi, 8)
    NumPut("UShort", 1, bi, 12), NumPut("UShort", 32, bi, 14)
    hdc := DllCall("CreateCompatibleDC", "Ptr", 0, "Ptr")
    hbm := DllCall("CreateDIBSection", "Ptr", hdc, "Ptr", bi, "UInt", 0, "Ptr*", &bits, "Ptr", 0, "UInt", 0, "Ptr")
    DllCall("SelectObject", "Ptr", hdc, "Ptr", hbm)
    return hdc
}

; Where each frame is put together before it goes on screen.
GlowCanvas(w, h) {
    static cache := Map()
    key := w "|" h
    if !cache.Has(key)
        cache[key] := NewDib(w, h)
    return cache[key]
}

; The glow's colour at position y of a cycle `period` long, starting `shift` colours in: it eases
; from each of GLOW_COLORS to the next. 0xRRGGBB.
GlowColorAt(y, period, shift) {
    n := GLOW_COLORS.Length
    pos := Mod(Mod(y, period) / period * n + shift, n), i := Floor(pos), k := pos - i
    k := k * k * (3 - 2 * k)
    c1 := GLOW_COLORS[i + 1], c2 := GLOW_COLORS[Mod(i + 1, n) + 1]
    cr := (c1 >> 16) + ((c2 >> 16) - (c1 >> 16)) * k
    cg := ((c1 >> 8) & 0xFF) + (((c2 >> 8) & 0xFF) - ((c1 >> 8) & 0xFF)) * k
    cb := (c1 & 0xFF) + ((c2 & 0xFF) - (c1 & 0xFF)) * k
    return (Round(cr) << 16) | (Round(cg) << 8) | Round(cb)
}

; Called every frame while the dock is open. edge: +1 = cursor in the right bump, -1 = left bump,
; 0 = neither. Returns true when it's time to change desktop.
; Same timing as macOS (Dock's workspaces-edge-delay, 0.75 s): the cursor has to stay there the
; whole time, leaving starts the count again.
global EDGE_HOLD_MS := 750
EdgeHeld(edge) {
    global edgeSide, edgeSince
    if (edge != edgeSide)
        edgeSide := edge, edgeSince := A_TickCount
    return edge != 0 && A_TickCount - edgeSince >= EDGE_HOLD_MS
}

; True once the cursor has stayed put (within a few pixels: a hand is never perfectly still) for
; BAR_HOLD_MS while the window is held. Any real move starts the count again.
global stillX := 0, stillY := 0, stillSince := 0
HeldStill() {
    global stillX, stillY, stillSince
    static slack := Round(6 * A_ScreenDPI / 96)
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    if (Abs(x - stillX) > slack || Abs(y - stillY) > slack)
        stillX := x, stillY := y, stillSince := A_TickCount
    return A_TickCount - stillSince >= BAR_HOLD_MS
}

MonitorAt(x, y) {
    Loop MonitorGetCount() {
        MonitorGet(A_Index, &l, &t, &r, &b)
        if (x >= l && x < r && y >= t && y < b)
            return A_Index
    }
    return 0
}

ToggleHold(itemName, *) {
    global holdOn := !holdOn
    SaveSetting("HoldWindow", holdOn, itemName)
}

TogglePerf(itemName, *) {
    global perfMode := !perfMode
    SaveSetting("Performance", perfMode, itemName)
}

SaveSetting(key, on, itemName) {
    try {
        DirCreate(A_AppData "\EzDesk")
        IniWrite(on ? 1 : 0, SETTINGS, "EzDesk", key)
    }
    on ? A_TrayMenu.Check(itemName) : A_TrayMenu.Uncheck(itemName)
}

global moveSizeHook := DllCall("SetWinEventHook", "UInt", 0x000A, "UInt", 0x000B, "Ptr", 0
    , "Ptr", CallbackCreate(OnMoveSize, , 7), "UInt", 0, "UInt", 0, "UInt", 0, "Ptr")   ; out of context

; Windows only announces a drag once the mouse has moved a few pixels. So a window held by its title
; bar without moving at all is watched from the button press itself: if it stays still long enough,
; the desktop bar opens. Moving then turns it into a normal drag, and WatchDrag takes over.
; Title bar only (HTCAPTION): holding the button down in a page or a document stays what it is.
global pressHwnd := 0
Hotkey("~*LButton", PressStart)                     ; ~: the click goes through untouched

PressStart(*) {
    global pressHwnd, stillX, stillY, stillSince, dragFrom
    if (!holdOn || !pWinDesk || !pPin || !pUnpin || dragHwnd || pressHwnd || barShown)
        return
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y, &under)
    hwnd := under ? DllCall("GetAncestor", "Ptr", under, "UInt", 2, "Ptr") : 0   ; GA_ROOT: the top-level window
    if (!hwnd || !IsRealWindow(hwnd))
        return
    try hit := SendMessage(0x84, 0, ((y & 0xFFFF) << 16) | (x & 0xFFFF), , hwnd, , , , 200)   ; WM_NCHITTEST
    catch
        return
    if (hit != 2)                                   ; HTCAPTION
        return
    pressHwnd := hwnd, stillX := x, stillY := y, stillSince := A_TickCount
    try {
        WinGetPos(&fx, &fy, &fw, &fh, hwnd)
        dragFrom := {max: WinGetMinMax(hwnd) = 1, x: fx, y: fy, w: fw, h: fh}
    }
    SetTimer(WatchPress, 15)
}

WatchPress() {
    global pressHwnd
    if dragHwnd {                                   ; it became a drag: WatchDrag carries on, bar included
        SetTimer(WatchPress, 0), pressHwnd := 0
        return
    }
    if !GetKeyState("LButton", "P") {               ; let go without ever moving: nothing to drop
        SetTimer(WatchPress, 0), pressHwnd := 0
        BarHide()
        return
    }
    if (HeldStill() && !barShown)
        BarShow(MonitorAt(stillX, stillY), pressHwnd)
    if barShown
        BarTrack()                                  ; keeps the glow flowing
}

; ---------------------------------------------------------------------
;  Desktop bar: hold a window without moving, drop it on a desktop
; ---------------------------------------------------------------------
; Like the Spaces bar at the top of macOS Mission Control, without the overview: your windows stay
; where they are. Keep a window held (mouse button down on it) without moving for 0.75 s, and a
; bar of desktops appears at the bottom of that screen, above the taskbar (the top belongs to Windows'
; own snapping), each a live miniature: its wallpaper and its
; windows, drawn by Windows itself (DWM thumbnails). On Windows 11 a "+" makes a new desktop. Drop
; the window on one: it goes there, back where you picked it up, and you stay where you are.
; Dropped anywhere else, it's an ordinary drop and the bar goes away.
; The bar is a squircle (corners eased like Apple's, not circle arcs) with the edge glow's light
; flowing all round it for as long as it's open.
global BAR_HOLD_MS := 750
global bar := 0, barShown := false, barTiles := [], barThumbs := [], barHover := 0, barHeld := 0
global barBase := {x: 0, y: 0}

; ---- Motion: the dock rises into place when it opens, with a little spring ----
global DECK_RISE := 40                              ; from how far below, in pixels at 100 %
global deckRise := 0                                ; how far below its place it is right now

DeckMotion() {
    global deckRise
    rise := Round((1 - BumpSpring((A_TickCount - deckShownAt) / 650)) * DECK_RISE * A_ScreenDPI / 96)   ; up, a little past, back
    if (rise = deckRise)
        return
    deckRise := rise                                ; the dock window, and its glow with it
    barBox.y := barBase.y + rise, deckGeom.y := barBox.y - deckGeom.mg
    DllCall("SetWindowPos", "Ptr", bar.Hwnd, "Ptr", 0, "Int", barBox.x, "Int", barBox.y, "Int", 0, "Int", 0, "UInt", 0x15)   ; NOSIZE | NOZORDER | NOACTIVATE
}
global barBox := {x: 0, y: 0, w: 0, h: 0, mon: 0, gap: 0, edge: 0}

; Tile and bar sizes for a screen; tiles have the screen's shape and shrink when there are many.
BarLayout(mon, n) {
    MonitorGet(mon, &ml, &mt, &mr, &mb)
    mw := mr - ml, mh := mb - mt, s := A_ScreenDPI / 96
    th := Max(Round(108 * s), Round(mh * 0.11))     ; about a ninth of the screen's height: big enough to see what's in each
    pad := Round(th * 0.14), gap := Round(th * 0.14), labelH := Round(th * 0.2)
    tw := Round(th * mw / mh)
    if (n * tw + (n - 1) * gap + 2 * pad > mw * 0.92)
        tw := Floor((mw * 0.92 - 2 * pad - (n - 1) * gap) / n), th := Round(tw * mh / mw)
    return {ml: ml, mt: mt, mr: mr, mb: mb, pad: pad, gap: gap, labelH: labelH, tw: tw, th: th
        , bw: n * tw + (n - 1) * gap + 2 * pad, bh: pad + th + labelH + Round(th * 0.07), s: s}
}

; Opens the dock (intro) or rebuilds it in place after a desktop switch: the new one is built and
; shown before the old one goes, so it never blinks.
BarShow(mon, held, intro := true) {
    global bar, barShown, barTiles, barThumbs, barHover, barBox, deckGeom, deckShownAt, barHeld
    global barBase, deckRise
    count := DesktopCount(), cur := CurrentDesktop()
    if (count <= 0 || cur < 0)
        return
    n := count + (pCreate ? 1 : 0)                  ; "+" only where a desktop can be made without going there
    lay := BarLayout(mon, n), tw := lay.tw, th := lay.th, pad := lay.pad
    MonitorGetWorkArea(mon, , , , &workBottom)      ; at the bottom, just above the taskbar: nothing of Windows' there
    mg := Round(DECK_GLOW * lay.s)                  ; how far the glow reaches out round the deck
    bx := lay.ml + (lay.mr - lay.ml - lay.bw) // 2, by := workBottom - lay.bh - mg   ; its haze ends at the taskbar, not over it
    rad := DeckRadius(lay)
    if intro {                                      ; it rises into place from a little lower
        deckShownAt := A_TickCount
        deckRise := Round(DECK_RISE * lay.s)
    }
    sx := bx, sy := by + deckRise                   ; where it is right now
    static glass := 0, glassKey := ""
    key := bx "|" by "|" lay.bw "|" lay.bh
    if (intro || key != glassKey) {                 ; the frosted glass: what's behind, taken as it opens
        if glass
            DllCall("DeleteObject", "Ptr", glass)
        glass := DeckGlass(bx, by, lay.bw, lay.bh), glassKey := key
    }
    oldBar := bar, oldThumbs := barThumbs
    bar := Gui("-Caption +AlwaysOnTop +ToolWindow -DPIScale +E0x08000000", "EzDesk desktops")   ; NOACTIVATE: the drag goes on
    bar.BackColor := "1C1C22", bar.MarginX := 0, bar.MarginY := 0
    if glass                                        ; first, so it's under the tiles
        bar.AddPicture("x0 y0 w" lay.bw " h" lay.bh, "HBITMAP:*" glass)
    barTiles := [], barThumbs := [], barHover := 0
    Loop n {
        i := A_Index - 1, x := pad + i * (tw + lay.gap), isNew := i >= count
        at := "x" x " y" pad " w" tw " h" th
        if isNew {
            bar.SetFont("s22 norm cD8D8E0", "Segoe UI")
            ctl := bar.AddText(at " Center +0x200 Background2A2A33", "+")   ; SS_CENTERIMAGE: centred both ways
            name := "New desktop"
        } else {
            hbm := WallBitmap(DesktopWallpaper(i), tw, th)
            ctl := hbm ? bar.AddPicture(at, "HBITMAP:*" hbm) : bar.AddText(at " Background2A2A33")
            name := DesktopName(i)
            if (name = "")
                name := "Desktop " (i + 1)
        }
        barTiles.Push({x: x, y: pad, w: tw, h: th, target: isNew ? -2 : i, name: name, current: i = cur, ctl: ctl})   ; names: drawn by DeckDrawOverlay
    }
    bar.Show("Hide x" sx " y" sy " w" lay.bw " h" lay.bh)           ; sized, shaped, then shown
    DllCall("SetWindowRgn", "Ptr", bar.Hwnd, "Ptr", DeckRegion(lay.bw, lay.bh, rad), "Int", false)   ; Windows owns the region now
    bar.Show("NA x" sx " y" sy " w" lay.bw " h" lay.bh)
    VdaCall(pPin, "Ptr", bar.Hwnd, "Int")           ; on every desktop: it stays put through a switch
    barBase := {x: bx, y: by}
    barBox := {x: sx, y: sy, w: lay.bw, h: lay.bh, mon: mon, gap: lay.gap, labelH: lay.labelH}
    deckGeom := {x: sx - mg, y: sy - mg, fw: lay.bw + 2 * mg, fh: lay.bh + 2 * mg, bw: lay.bw, bh: lay.bh, rad: rad, mg: mg}
    barShown := true, barHeld := held
    barHover := 0
    DeckDrawOverlay()
    BarMiniatures(count, lay, held)
    for thumb in oldThumbs                          ; a rebuild: the previous dock goes only now
        DllCall("dwmapi\DwmUnregisterThumbnail", "Ptr", thumb)
    if oldBar
        try oldBar.Destroy()
    if intro {
        BumpsOpen()
        WinGlowShow(held)
    }
    DeckGlowFrame()
    OnTop(deckGlow)                                 ; the glows stay over the dock
    Loop 2
        if bumpVis[A_Index]
            OnTop(bumpWin[A_Index])
    static glowPinned := false
    if !glowPinned
        glowPinned := VdaCall(pPin, "Ptr", deckGlow.Hwnd, "Int") != -1
}

; After a desktop switch: the dock shows the new current desktop (and the new one, if one was made).
BarRefresh() {
    if barShown
        BarShow(barBox.mon, barHeld, false)
}

; One live miniature per window, on its desktop's tile: bottom window first, so the ones on top
; are drawn last, over them. Only the part of each window on this screen.
BarMiniatures(count, lay, held) {                   ; held: the window in your hand, left out
    DetectHiddenWindows(true)                       ; AutoHotkey counts windows on other desktops as hidden
    sx := lay.tw / (lay.mr - lay.ml), sy := lay.th / (lay.mb - lay.mt)
    props := Buffer(48, 0)                          ; DWM_THUMBNAIL_PROPERTIES
    cloak := Buffer(4, 0)
    list := WinGetList(), k := list.Length
    while (k >= 1) {
        hwnd := list[k], k -= 1
        if (hwnd = held || !IsRealWindow(hwnd))
            continue
        NumPut("UInt", 0, cloak)                    ; DWMWA_CLOAKED: 2 = on another desktop, fine;
        DllCall("dwmapi\DwmGetWindowAttribute", "Ptr", hwnd, "UInt", 14, "Ptr", cloak, "UInt", 4, "Int")
        if (NumGet(cloak, "UInt") & 1)              ; 1 = hidden by the app itself (Store apps in the background)
            continue
        d := VdaCall(pWinDesk, "Ptr", hwnd, "Int")
        if (d < 0 || d >= count)
            continue
        try WinGetPos(&wx, &wy, &ww, &wh, hwnd)
        catch
            continue
        x1 := Max(wx, lay.ml), y1 := Max(wy, lay.mt), x2 := Min(wx + ww, lay.mr), y2 := Min(wy + wh, lay.mb)
        if (x2 <= x1 || y2 <= y1)                   ; not on this screen
            continue
        thumb := 0
        if DllCall("dwmapi\DwmRegisterThumbnail", "Ptr", bar.Hwnd, "Ptr", hwnd, "Ptr*", &thumb, "Int") != 0
            continue
        tile := barTiles[d + 1]
        NumPut("UInt", 0x1 | 0x2 | 0x8, props, 0)   ; destination, source, visible
        NumPut("Int", tile.x + Round((x1 - lay.ml) * sx), "Int", tile.y + Round((y1 - lay.mt) * sy)
            , "Int", tile.x + Round((x2 - lay.ml) * sx), "Int", tile.y + Round((y2 - lay.mt) * sy), props, 4)
        NumPut("Int", x1 - wx, "Int", y1 - wy, "Int", x2 - wx, "Int", y2 - wy, props, 20)
        NumPut("Int", 1, props, 40)
        DllCall("dwmapi\DwmUpdateThumbnailProperties", "Ptr", thumb, "Ptr", props, "Int")
        barThumbs.Push(thumb)
    }
}

; Every 15 ms while the bar is up: frame the tile under the cursor. The bar stays until the drop.
; On a slow PC (or one that's busy for a while) a frame can take longer than the 15 ms between two:
; then the glows round the dock and the window are redrawn every other frame only (the bumps, which
; do the switching, still every frame), until frames are quick again. Decided from the average
; time of the last full frames.
BarTrack() {
    global barHover
    static freq := 0, avg := 0, odd := false, autoLite := false
    if !freq
        DllCall("QueryPerformanceFrequency", "Int64*", &freq)
    DllCall("QueryPerformanceCounter", "Int64*", &t0 := 0)
    odd := !odd
    autoLite := avg > 9 ? true : avg < 7 ? false : autoLite   ; on and off again as the PC chokes and recovers
    full := !(autoLite || perfMode) || odd          ; Performance mode (tray menu): always light
    DeckMotion()
    if full {
        DeckGlowFrame()
        WinGlowFrame()
    }
    BumpFrame()
    DllCall("QueryPerformanceCounter", "Int64*", &t1 := 0)
    ms := (t1 - t0) * 1000 / freq
    if (full && ms < 40)                            ; full frames only (else skipping would hide the load); one-offs don't count
        avg += (ms - avg) * 0.1
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    box := barBox, hit := 0, half := box.gap // 2
    for tile in barTiles
        if (x >= box.x + tile.x - half && x < box.x + tile.x + tile.w + half && y >= box.y - half)   ; anywhere below the bar's top
            hit := A_Index
    if (hit = barHover)
        return
    barHover := hit
    DeckDrawOverlay()                               ; the frame moves to the tile under the cursor
}

; The desktop under the cursor when the window is dropped: its index, -2 for "+", -1 for none.
BarTarget() => barShown && barHover ? barTiles[barHover].target : -1

BarHide() {
    global bar, barShown, barTiles, barThumbs, barHover
    if !barShown
        return
    for thumb in barThumbs
        DllCall("dwmapi\DwmUnregisterThumbnail", "Ptr", thumb)
    try bar.Destroy()
    deckGlow.Hide()
    BumpsHide()
    WinGlowHide()
    SetTimer(BarRefresh, 0)
    bar := 0, barShown := false, barTiles := [], barThumbs := [], barHover := 0
}

; The wallpaper of desktop i (Windows 11 gives each desktop its own), else the current one.
DesktopWallpaper(i) {
    static key := "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops"
    path := ""
    try {
        hex := SubStr(RegRead(key, "VirtualDesktopIDs"), i * 32 + 1, 32)   ; 16 bytes per desktop, in order
        if (StrLen(hex) = 32)
            path := RegRead(key "\Desktops\" GuidFromHex(hex), "Wallpaper", "")
    }
    if (path = "" || !FileExist(path))
        path := RegRead("HKCU\Control Panel\Desktop", "WallPaper", "")
    if (path = "" || !FileExist(path))
        path := A_AppData "\Microsoft\Windows\Themes\TranscodedWallpaper"
    return FileExist(path) ? path : ""
}

; A GUID as the registry stores it (16 bytes in hex) → "{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}".
GuidFromHex(hex) {
    by := []
    Loop 16
        by.Push(SubStr(hex, A_Index * 2 - 1, 2))
    return "{" by[4] by[3] by[2] by[1] "-" by[6] by[5] "-" by[8] by[7] "-" by[9] by[10] "-"
        . by[11] by[12] by[13] by[14] by[15] by[16] "}"
}

; A wallpaper scaled to a tile, kept for next time (until the file changes).
WallBitmap(path, w, h) {
    static cache := Map()
    if (path = "")
        return 0
    key := path "|" FileGetTime(path) "|" w "|" h
    if cache.Has(key)
        return cache[key]
    src := path
    SplitPath(path, , , &ext)
    if (ext = "") {                                 ; TranscodedWallpaper has no extension: give it one to be read
        src := A_Temp "\EzDesk-wallpaper.jpg"
        try FileCopy(path, src, true)
    }
    hbm := 0
    try hbm := LoadPicture(src, "w" w " h" h)
    cache[key] := hbm
    return hbm
}

; Scaling big wallpapers takes a moment: done ahead of time, so the bar shows at once.
WarmBar() {
    if !pWinDesk || (count := DesktopCount()) <= 0
        return
    lay := BarLayout(MonitorGetPrimary(), count + (pCreate ? 1 : 0))
    Loop count
        WallBitmap(DesktopWallpaper(A_Index - 1), lay.tw, lay.th)
    mg := Round(DECK_GLOW * lay.s), fw := lay.bw + 2 * mg, fh := lay.bh + 2 * mg
    DeckMasks(fw, fh, lay.bw, lay.bh, DeckRadius(lay), mg)
    DeckStrip(fw, fh, fw, 0), DeckStrip(fw, fh, GlowPeriod2(fw), GLOW_COLORS.Length // 2)
    Loop BUMP_LEVELS
        BumpMasks(A_Index - 1)
    bh := BumpGeom().bh
    BumpStrip(bh, 0), BumpStrip(GlowPeriod2(bh), GLOW_COLORS.Length // 2), BumpWhite()
}

; ---- The deck: squircle outline and its glow ----
global deckGlow := Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0")   ; LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW
global deckGeom := {x: 0, y: 0, fw: 0, fh: 0, bw: 0, bh: 0, rad: 0, mg: 0}, deckShownAt := 0
global DECK_N := 5                                  ; superellipse exponent: 2 = circle arcs, higher = squarer
global DECK_GLOW := 14                              ; how far the haze reaches out round the deck, in pixels at 100 %

DeckRadius(lay) => Min(Round(lay.bh * 0.28), lay.bh // 2)   ; grows with the dock

; The deck's outline as a window region: a rectangle whose corners are superellipse quarters
; (|x|^n + |y|^n = rad^n), going round clockwise from the top edge.
DeckRegion(bw, bh, rad) {
    e := 2 / DECK_N, steps := 16, quarter := 1.5707963
    corners := [[bw - rad, rad, 1, -1, true], [bw - rad, bh - rad, 1, 1, false]
        , [rad, bh - rad, -1, 1, true], [rad, rad, -1, -1, false]]   ; centre, direction, true = from 90° down to 0°
    pts := Buffer(8 * corners.Length * (steps + 1)), k := 0
    for c in corners {
        Loop steps + 1 {
            t := (A_Index - 1) / steps * quarter
            if c[5]
                t := quarter - t
            NumPut("Int", Round(c[1] + c[3] * rad * Abs(Cos(t)) ** e)
                , "Int", Round(c[2] + c[4] * rad * Abs(Sin(t)) ** e), pts, k * 8)
            k += 1
        }
    }
    return DllCall("CreatePolygonRgn", "Ptr", pts, "Int", k, "Int", 1, "Ptr")   ; ALTERNATE
}

; The glow's brightness round the deck as two masks, fw × fh (the deck plus a margin mg all round),
; built once per size: bright on the squircle's edge, a wide haze outside, a few pixels inside that
; cover the region's hard edge. "inv" is black with alpha 1 - m, "keep" white with alpha m.
; A frame is colours × m without touching a pixel from here: AlphaBlend inv over the opaque colours
; (each pixel becomes m × colour, alpha 255), then AND keep, which sets the alpha byte to m.
DeckMasks(fw, fh, bw, bh, rad, mg) {
    static cache := Map()
    key := fw "|" fh "|" rad "|" mg
    if cache.Has(key)
        return cache[key]
    inv := NewDib(fw, fh, &ib), keep := NewDib(fw, fh, &kb)
    Fill32(ib, fw * fh, 0xFF000000), Fill32(kb, fw * fh, 0x00FFFFFF)   ; m = 0 everywhere to start
    inner := Max(2, Round(5 * A_ScreenDPI / 96)), ex := DECK_N
    hx := bw / 2 - rad, hy := bh / 2 - rad, cx := fw / 2, cy := fh / 2
    MaskAt(px, py) {                                ; m (0-255) at the centre of pixel (px, py)
        qx := Abs(px + 0.5 - cx) - hx, qy := Abs(py + 0.5 - cy) - hy
        if (qx > 0 && qy > 0) {                     ; in a corner: the superellipse's own measure, divided by its
            lp := (qx ** ex + qy ** ex) ** (1 / ex) ; slope so it's a true distance (raw, it's ~20 % short at 45°
            gx := (qx / lp) ** (ex - 1), gy := (qy / lp) ** (ex - 1)   ; and the haze bulges out at the corners)
            d := (lp - rad) / Sqrt(gx * gx + gy * gy)
        } else {
            d := Max(qx, qy) - rad                  ; along a side: straight distance from the edge
        }
        v := d >= 0 ? GlowFall(d, mg) : d > -inner ? GlowFall(0, mg) * (1 + d / inner) : 0
        return Round(255 * Min(1, v))
    }
    MaskPut(px, py, m) {
        o := (py * fw + px) * 4
        NumPut("UInt", (255 - m) << 24, ib, o), NumPut("UInt", (m << 24) | 0xFFFFFF, kb, o)
    }
    cs := mg + rad                                  ; corner box: from the frame's edge to where the sides go straight
    Loop cs {                                       ; the four corners, mirrored from the top-left one
        j := A_Index - 1
        Loop cs {
            i := A_Index - 1, m := MaskAt(i, j)
            if m
                MaskPut(i, j, m), MaskPut(fw - 1 - i, j, m), MaskPut(i, fh - 1 - j, m), MaskPut(fw - 1 - i, fh - 1 - j, m)
        }
    }
    Loop cs {                                       ; top and bottom: the same all along a row
        j := A_Index - 1, m := MaskAt(fw // 2, j)
        if !m
            continue
        for row in [j, fh - 1 - j] {
            Fill32(ib + (row * fw + cs) * 4, fw - 2 * cs, (255 - m) << 24)
            Fill32(kb + (row * fw + cs) * 4, fw - 2 * cs, (m << 24) | 0xFFFFFF)
        }
    }
    if (fh > 2 * cs) {                              ; left and right: the same all down a column
        Loop cs {
            i := A_Index - 1, m := MaskAt(i, fh // 2)
            if m
                MaskPut(i, cs, m), MaskPut(fw - 1 - i, cs, m)
        }
        Loop fh - 2 * cs - 1 {
            row := cs + A_Index
            for side in [0, fw - cs]
                for base in [ib, kb]
                    DllCall("RtlMoveMemory", "Ptr", base + (row * fw + side) * 4, "Ptr", base + (cs * fw + side) * 4, "UPtr", cs * 4)
        }
    }
    cache[key] := {inv: inv, keep: keep}
    return cache[key]
}

; Sets count 32-bit pixels from ptr to value, by doubling copies.
Fill32(ptr, count, value) {
    if (count <= 0)
        return
    NumPut("UInt", value, ptr)
    done := 1
    while (done < count) {
        n := Min(done, count - done)
        DllCall("RtlMoveMemory", "Ptr", ptr + done * 4, "Ptr", ptr, "UPtr", n * 4)
        done += n
    }
}

; Opaque colour bands, fw + period wide and fh tall, the colours cycling once per period across:
; a frame takes an fw-wide slice at a moving offset, like the edge glow's layers but sideways.
DeckStrip(fw, fh, period, shift) {
    static cache := Map()
    key := fw "|" fh "|" period "|" shift
    if cache.Has(key)
        return cache[key]
    total := fw + period
    dc := NewDib(total, fh, &bits)
    Loop total
        NumPut("UInt", 0xFF000000 | GlowColorAt(A_Index - 1, period, shift), bits, (A_Index - 1) * 4)
    Loop fh - 1
        DllCall("RtlMoveMemory", "Ptr", bits + A_Index * total * 4, "Ptr", bits, "UPtr", total * 4)
    cache[key] := {dc: dc}
    return cache[key]
}

; One frame of the glow round the deck: two colour layers sliding in opposite directions, mixed,
; then shaped by the masks. Four GDI calls, whatever the deck's size.
DeckGlowFrame() {
    if !barShown
        return
    g := deckGeom, fw := g.fw, fh := g.fh, p2 := GlowPeriod2(fw)
    masks := DeckMasks(fw, fh, g.bw, g.bh, g.rad, g.mg)
    sa := DeckStrip(fw, fh, fw, 0), sb := DeckStrip(fw, fh, p2, GLOW_COLORS.Length // 2)
    xa := fw - Mod(A_TickCount * fw // GLOW_FLOW_MS, fw)        ; flows right
    xb := Mod(A_TickCount * p2 // GLOW_FLOW2_MS, p2)             ; flows left
    canvas := GlowCanvas(fw, fh)
    DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", fw, "Int", fh, "Ptr", sa.dc, "Int", xa, "Int", 0, "UInt", 0x00CC0020)
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", fw, "Int", fh
        , "Ptr", sb.dc, "Int", xb, "Int", 0, "Int", fw, "Int", fh, "UInt", 150 << 16)                 ; mixed in at 150/255
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", fw, "Int", fh
        , "Ptr", masks.inv, "Int", 0, "Int", 0, "Int", fw, "Int", fh, "UInt", (1 << 24) | (255 << 16)) ; × m
    DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", fw, "Int", fh, "Ptr", masks.keep, "Int", 0, "Int", 0, "UInt", 0x008800C6)   ; SRCAND: alpha = m
    if (deckOverlay && deckOverlay.fw = fw && deckOverlay.fh = fh)   ; the names and the frame, over it all
        DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", fw, "Int", fh
            , "Ptr", deckOverlay.dc, "Int", 0, "Int", 0, "Int", fw, "Int", fh, "UInt", (1 << 24) | (255 << 16))
    k := Min(1, (A_TickCount - deckShownAt) / 200)
    GlowPut(deckGlow, g.x, g.y, fw, fh, canvas, Round(240 * k * k * (3 - 2 * k)))
}

; The dock's frosted glass, made by hand: Windows' own acrylic ignores a window's shape (and makes
; its contents see-through), so instead the dock's background is a picture of what's behind it:
; shrunk 24 times (averaging, the blur), grown back with smooth interpolation, tinted dark.
; Taken when the dock opens, in memory only; after a desktop switch the same one is kept.
; Returns a bitmap handle the dock shows as its background (0 if it couldn't).
DeckGlass(x, y, w, h) {
    sw := Max(1, w // 24), sh := Max(1, h // 24)
    big := NewDib(w, h), small := NewDib(sw, sh, &sbits)
    sdc := DllCall("GetDC", "Ptr", 0, "Ptr")
    DllCall("BitBlt", "Ptr", big, "Int", 0, "Int", 0, "Int", w, "Int", h, "Ptr", sdc, "Int", x, "Int", y, "UInt", 0x40CC0020)   ; SRCCOPY | CAPTUREBLT
    DllCall("ReleaseDC", "Ptr", 0, "Ptr", sdc)
    DllCall("SetStretchBltMode", "Ptr", small, "Int", 4), DllCall("SetBrushOrgEx", "Ptr", small, "Int", 0, "Int", 0, "Ptr", 0)   ; HALFTONE: averages
    DllCall("StretchBlt", "Ptr", small, "Int", 0, "Int", 0, "Int", sw, "Int", sh, "Ptr", big, "Int", 0, "Int", 0, "Int", w, "Int", h, "UInt", 0x00CC0020)
    FreeDib(big)
    bi := Buffer(40, 0)                             ; the result: a bitmap of its own, for the dock to show
    NumPut("UInt", 40, bi, 0), NumPut("Int", w, bi, 4), NumPut("Int", -h, bi, 8), NumPut("UShort", 1, bi, 12), NumPut("UShort", 32, bi, 14)
    bits := 0
    hbm := DllCall("CreateDIBSection", "Ptr", 0, "Ptr", bi, "UInt", 0, "Ptr*", &bits, "Ptr", 0, "UInt", 0, "Ptr")
    if !hbm {
        FreeDib(small)
        return 0
    }
    GdipStart()
    src := 0, dst := 0, gfx := 0, attr := 0
    DllCall("gdiplus\GdipCreateBitmapFromScan0", "Int", sw, "Int", sh, "Int", sw * 4, "Int", 0x22009, "Ptr", sbits, "Ptr*", &src)   ; 32bpp RGB: the capture has no alpha
    DllCall("gdiplus\GdipCreateBitmapFromScan0", "Int", w, "Int", h, "Int", w * 4, "Int", 0xE200B, "Ptr", bits, "Ptr*", &dst)
    DllCall("gdiplus\GdipGetImageGraphicsContext", "Ptr", dst, "Ptr*", &gfx)
    DllCall("gdiplus\GdipSetInterpolationMode", "Ptr", gfx, "Int", 7)    ; high-quality bilinear: smooth
    DllCall("gdiplus\GdipSetPixelOffsetMode", "Ptr", gfx, "Int", 4)
    DllCall("gdiplus\GdipCreateImageAttributes", "Ptr*", &attr)
    DllCall("gdiplus\GdipSetImageAttributesWrapMode", "Ptr", attr, "Int", 3, "UInt", 0, "Int", 0)   ; mirrored at the edges: no dark rim
    DllCall("gdiplus\GdipDrawImageRectRectI", "Ptr", gfx, "Ptr", src, "Int", 0, "Int", 0, "Int", w, "Int", h
        , "Int", 0, "Int", 0, "Int", sw, "Int", sh, "Int", 2, "Ptr", attr, "Ptr", 0, "Ptr", 0)
    tint := 0
    DllCall("gdiplus\GdipCreateSolidFill", "UInt", 0xB81C1C22, "Ptr*", &tint)   ; dark, about 72 %
    DllCall("gdiplus\GdipFillRectangleI", "Ptr", gfx, "Ptr", tint, "Int", 0, "Int", 0, "Int", w, "Int", h)
    DllCall("gdiplus\GdipDeleteBrush", "Ptr", tint), DllCall("gdiplus\GdipDisposeImageAttributes", "Ptr", attr)
    DllCall("gdiplus\GdipDeleteGraphics", "Ptr", gfx), DllCall("gdiplus\GdipDisposeImage", "Ptr", dst), DllCall("gdiplus\GdipDisposeImage", "Ptr", src)
    FreeDib(small)
    return hbm
}

; Frees a memory DC made by NewDib, and its bitmap.
FreeDib(hdc) {
    hbm := DllCall("GetCurrentObject", "Ptr", hdc, "UInt", 7, "Ptr")   ; OBJ_BITMAP
    DllCall("DeleteDC", "Ptr", hdc), DllCall("DeleteObject", "Ptr", hbm)
}

GdipStart() {
    static token := 0
    if token
        return
    DllCall("LoadLibrary", "Str", "gdiplus", "Ptr")
    si := Buffer(24, 0)                             ; GdiplusStartupInput, version 1
    NumPut("UInt", 1, si, 0)
    DllCall("gdiplus\GdiplusStartup", "UPtr*", &token, "Ptr", si, "Ptr", 0)
}

; What's drawn over the frosted dock, crisp: each desktop's name under its tile (the current one
; in bold white), and a rounded frame round the tile under the cursor. GDI+ with antialiasing, into
; a premultiplied bitmap the glow's frame lays on top. Redrawn only when it changes.
global deckOverlay := 0
DeckDrawOverlay() {
    global deckOverlay
    static dibs := Map()
    g := deckGeom, fw := g.fw, fh := g.fh, key := fw "|" fh
    if !dibs.Has(key) {
        dc := NewDib(fw, fh, &bits)
        dibs[key] := {dc: dc, bits: bits, fw: fw, fh: fh}
    }
    o := dibs[key]
    Fill32(o.bits, fw * fh, 0)
    GdipStart()
    bmp := 0, gfx := 0, fam := 0, fmt := 0
    DllCall("gdiplus\GdipCreateBitmapFromScan0", "Int", fw, "Int", fh, "Int", fw * 4, "Int", 0xE200B, "Ptr", o.bits, "Ptr*", &bmp)   ; 32bpp PARGB, on our pixels
    DllCall("gdiplus\GdipGetImageGraphicsContext", "Ptr", bmp, "Ptr*", &gfx)
    DllCall("gdiplus\GdipSetSmoothingMode", "Ptr", gfx, "Int", 4)        ; antialias
    DllCall("gdiplus\GdipSetTextRenderingHint", "Ptr", gfx, "Int", 4)    ; antialias, grid fit
    DllCall("gdiplus\GdipCreateFontFamilyFromName", "WStr", "Segoe UI", "Ptr", 0, "Ptr*", &fam)
    DllCall("gdiplus\GdipCreateStringFormat", "Int", 0, "Int", 0, "Ptr*", &fmt)
    DllCall("gdiplus\GdipSetStringFormatAlign", "Ptr", fmt, "Int", 1)    ; centred
    DllCall("gdiplus\GdipSetStringFormatLineAlign", "Ptr", fmt, "Int", 1)
    s := A_ScreenDPI / 96, rect := Buffer(16)
    for tile in barTiles {
        font := 0, brush := 0
        DllCall("gdiplus\GdipCreateFont", "Ptr", fam, "Float", Max(12 * s, barBox.labelH * 0.5), "Int", tile.current ? 1 : 0, "Int", 2, "Ptr*", &font)   ; pixels; bold for the current one
        DllCall("gdiplus\GdipCreateSolidFill", "UInt", tile.current ? 0xFFFFFFFF : 0xFFB8B8C4, "Ptr*", &brush)
        NumPut("Float", g.mg + tile.x, "Float", g.mg + tile.y + tile.h + 3 * s, "Float", tile.w, "Float", barBox.labelH, rect)
        DllCall("gdiplus\GdipDrawString", "Ptr", gfx, "WStr", tile.name, "Int", -1, "Ptr", font, "Ptr", rect, "Ptr", fmt, "Ptr", brush)
        DllCall("gdiplus\GdipDeleteBrush", "Ptr", brush), DllCall("gdiplus\GdipDeleteFont", "Ptr", font)
    }
    if barHover {                                   ; the frame round the tile under the cursor
        tile := barTiles[barHover], e := 3 * s, d := 16 * s, pen := 0, path := 0
        x := g.mg + tile.x - e, y := g.mg + tile.y - e, w := tile.w + 2 * e, h := tile.h + 2 * e
        DllCall("gdiplus\GdipCreatePen1", "UInt", 0xFFBC82F3, "Float", 2.5 * s, "Int", 2, "Ptr*", &pen)
        DllCall("gdiplus\GdipCreatePath", "Int", 0, "Ptr*", &path)
        for arc in [[x, y, 180], [x + w - d, y, 270], [x + w - d, y + h - d, 0], [x, y + h - d, 90]]
            DllCall("gdiplus\GdipAddPathArc", "Ptr", path, "Float", arc[1], "Float", arc[2], "Float", d, "Float", d, "Float", arc[3], "Float", 90)
        DllCall("gdiplus\GdipClosePathFigure", "Ptr", path)
        DllCall("gdiplus\GdipDrawPath", "Ptr", gfx, "Ptr", pen, "Ptr", path)
        DllCall("gdiplus\GdipDeletePath", "Ptr", path), DllCall("gdiplus\GdipDeletePen", "Ptr", pen)
    }
    DllCall("gdiplus\GdipDeleteStringFormat", "Ptr", fmt), DllCall("gdiplus\GdipDeleteFontFamily", "Ptr", fam)
    DllCall("gdiplus\GdipDeleteGraphics", "Ptr", gfx), DllCall("gdiplus\GdipDisposeImage", "Ptr", bmp)
    deckOverlay := o
}
; ---- The side bumps: where you take the window to the previous / next desktop ----
; While the dock is open, a small glowing bump sits on each side of the screen, level with the
; cursor (it glides along the edge after it). Bring the window towards a side and that bump swells,
; smoothly with the distance, from 200 px away. Into the bump (about 20 px from the edge) and hold:
; it fills with light for 0.75 s, then
; you slide to that desktop, window in hand, and it gives a little bounce. Windows' Snap still works
; all along: let go against the edge before the 0.75 s and the window snaps there. On desktop 1 there's no
; left bump: nothing that way. Past the last desktop, the right one makes a new desktop.
; Shape: a bell, depth × (1 - u²)² across ±half (u = -1…1 along the edge), filled, with the glow's
; haze round it. Its size follows q = closeness (+ bounce): BUMP_LEVELS masks, made once.
global bumpWin := [Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0"), Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0")]
global bumpVis := [false, false], bumpShown := false, bumpOpenAt := 0
global bumpY := [0, 0], bumpSquash := [0, 0], bumpAvail := [0, 1]   ; [left, right]
global BUMP_LEVELS := 15, BUMP_QMAX := 1.4, BUMP_GLOW := 10, BUMP_REACH := 200

BumpDepth(q) => (5 + 15 * q * q) * A_ScreenDPI / 96                 ; how far it sticks out
BumpHalf(q) => (120 + 85 * Min(q, 1)) * A_ScreenDPI / 96            ; half its length along the edge
BumpSpring(t) {                                     ; 0 → 1, overshooting a little, like a spring
    t := Min(Max(t, 0), 1)
    return 1 - Exp(-6 * t) * Cos(9 * t)
}

; The bump windows' size: room for the biggest bump and its haze.
BumpGeom() {
    static g := 0
    if !g {
        s := A_ScreenDPI / 96, mg := Round(BUMP_GLOW * s)
        g := {s: s, mg: mg, bw: Ceil(BumpDepth(BUMP_QMAX)) + mg + 2, bh: 2 * (Ceil(BumpHalf(1)) + mg)}
    }
    return g
}

; The masks for size level k (0 … BUMP_LEVELS - 1), left and right (mirrored), as DeckMasks makes
; them: inv = black with alpha 1 - m, keep = white with alpha m.
BumpMasks(k) {
    static cache := Map()
    if cache.Has(k)
        return cache[k]
    g := BumpGeom(), bw := g.bw, bh := g.bh
    q := k / (BUMP_LEVELS - 1) * BUMP_QMAX, depth := BumpDepth(q), half := BumpHalf(q)
    linv := NewDib(bw, bh, &lib), lkeep := NewDib(bw, bh, &lkb), rinv := NewDib(bw, bh, &rib), rkeep := NewDib(bw, bh, &rkb)
    Fill32(lib, bw * bh, 0xFF000000), Fill32(rib, bw * bh, 0xFF000000)
    Fill32(lkb, bw * bh, 0x00FFFFFF), Fill32(rkb, bw * bh, 0x00FFFFFF)
    Loop bh // 2 {                                  ; the top half, mirrored to the bottom
        j := A_Index - 1, u := (bh / 2 - j - 0.5) / half
        if (u >= 1)
            continue
        e := 1 - u * u, xb := depth * e * e         ; the bump's outline at this row
        slope := 4 * depth * u * e / half, norm := Sqrt(1 + slope * slope)
        fade := Min(1, (1 - u) * 4), fade := fade * fade * (3 - 2 * fade)   ; its ends melt into the edge
        Loop bw {
            c := A_Index - 1, d := (c + 0.5 - xb) / norm   ; distance from the outline, outward
            m := Round(255 * fade * (d < 0 ? 1 : GlowFall(d, g.mg)))
            if !m {
                if (d > 0)
                    break                           ; only darker further out
                continue
            }
            for row in [j, bh - 1 - j] {
                ol := (row * bw + c) * 4, orr := (row * bw + bw - 1 - c) * 4
                NumPut("UInt", (255 - m) << 24, lib, ol), NumPut("UInt", (m << 24) | 0xFFFFFF, lkb, ol)
                NumPut("UInt", (255 - m) << 24, rib, orr), NumPut("UInt", (m << 24) | 0xFFFFFF, rkb, orr)
            }
        }
    }
    cache[k] := {l: {inv: linv, keep: lkeep}, r: {inv: rinv, keep: rkeep}}
    return cache[k]
}

; Opaque colour bands for the bumps, the colours changing down the rows, period rows a cycle.
BumpStrip(period, shift) {
    static cache := Map()
    key := period "|" shift
    if cache.Has(key)
        return cache[key]
    g := BumpGeom(), total := g.bh + period
    dc := NewDib(g.bw, total, &bits)
    Loop total
        Fill32(bits + (A_Index - 1) * g.bw * 4, g.bw, 0xFF000000 | GlowColorAt(A_Index - 1, period, shift))
    cache[key] := {dc: dc}
    return cache[key]
}

BumpWhite() {                                       ; for the light that fills a bump you hold
    static dc := 0
    if !dc {
        g := BumpGeom()
        dc := NewDib(g.bw, g.bh, &bits)
        Fill32(bits, g.bw * g.bh, 0xFFFFFFFF)
    }
    return dc
}

BumpsOpen() {
    global bumpY, bumpSquash, bumpAvail, bumpOpenAt, bumpShown, edgeSide
    CoordMode("Mouse", "Screen")
    MouseGetPos(, &y)
    bumpY := [y, y], bumpSquash := [0, 0], bumpAvail := [CurrentDesktop() > 0 ? 1 : 0, 1]
    bumpOpenAt := A_TickCount, edgeSide := 0, bumpShown := true
}

BumpsHide() {
    global bumpShown, bumpVis, edgeSide
    Loop 2
        bumpWin[A_Index].Hide()
    bumpVis := [false, false], bumpShown := false, edgeSide := 0
}

; One frame of both bumps, and the desktop switch when the cursor has held one long enough.
BumpFrame() {
    global bumpY, bumpSquash, bumpAvail, edgeSince
    if !bumpShown
        return
    g := BumpGeom(), now := A_TickCount
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    MonitorGetWorkArea(barBox.mon, &wl, &wt, &wr, &wb)
    cur := CurrentDesktop(), count := DesktopCount()
    yc := Min(Max(y, wt + g.bh // 2), wb - g.bh // 2)   ; level with the cursor, whole on the screen
    contact := 0
    Loop 2 {
        i := A_Index, side := i = 1 ? -1 : 1
        bumpY[i] += (yc - bumpY[i]) * 0.18          ; glides after the cursor
        bumpAvail[i] += ((side < 0 ? cur > 0 : 1) - bumpAvail[i]) * 0.12   ; desktop 1: the left one tucks away
        dist := side < 0 ? x - wl : wr - 1 - x
        near := Max(0, Min(1, 1 - dist / (BUMP_REACH * g.s))), p := 1 - (1 - near) ** 3
        grow := BumpSpring((now - bumpOpenAt - 120 - (i - 1) * 60) / 600) * bumpAvail[i]
        bumpSquash[i] *= 0.88
        q := Min(BUMP_QMAX, Max(0, (p + 0.4 * bumpSquash[i]) * grow))
        if (dragHwnd && bumpAvail[i] > 0.5 && dist >= -1 && dist <= BumpDepth(q) + 2 * g.s)
            contact := side
        alpha := Round(255 * (0.45 + 0.55 * p) * Min(1, Max(0, grow) * 1.5))
        if (alpha < 3) {
            if bumpVis[i]
                bumpWin[i].Hide(), bumpVis[i] := false
            continue
        }
        fill := (edgeSide = side && edgeSince) ? Min(1, (now - edgeSince) / EDGE_HOLD_MS) : 0
        BumpDraw(i, side, Round(q / BUMP_QMAX * (BUMP_LEVELS - 1)), fill, alpha
            , side < 0 ? wl : wr - g.bw, Round(bumpY[i] - g.bh / 2))
    }
    if EdgeHeld(contact) {                          ; held long enough: off to that desktop, window in hand
        Send(contact < 0 ? "^#{Left}" : cur + 1 < count ? "^#{Right}" : "^#d")
        bumpSquash[contact < 0 ? 1 : 2] := 1        ; the little bounce
        edgeSince := A_TickCount                    ; still held there: the next one 0.75 s later
        SetTimer(BarRefresh, -450)                  ; once the slide is done, the dock shows where you are
    }
}

BumpDraw(i, side, k, fill, alpha, x, y) {
    global bumpVis
    static pinned := false
    g := BumpGeom(), bw := g.bw, bh := g.bh, pa := bh, pb := GlowPeriod2(bh)   ; the whole palette along a bump
    sa := BumpStrip(pa, 0), sb := BumpStrip(pb, GLOW_COLORS.Length // 2)
    ya := Mod(A_TickCount * pa // (2 * GLOW_FLOW_MS), pa)            ; one layer flows up...
    yb := pb - Mod(A_TickCount * pb // (2 * GLOW_FLOW2_MS), pb)      ; ...the other down
    canvas := GlowCanvas(bw, bh), mk := side < 0 ? BumpMasks(k).l : BumpMasks(k).r
    DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", bw, "Int", bh, "Ptr", sa.dc, "Int", 0, "Int", ya, "UInt", 0x00CC0020)
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", bw, "Int", bh
        , "Ptr", sb.dc, "Int", 0, "Int", yb, "Int", bw, "Int", bh, "UInt", 150 << 16)
    if (fill > 0)                                   ; held: it fills with light
        DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", bw, "Int", bh
            , "Ptr", BumpWhite(), "Int", 0, "Int", 0, "Int", bw, "Int", bh, "UInt", Round(220 * fill) << 16)
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", bw, "Int", bh
        , "Ptr", mk.inv, "Int", 0, "Int", 0, "Int", bw, "Int", bh, "UInt", (1 << 24) | (255 << 16))
    DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", bw, "Int", bh, "Ptr", mk.keep, "Int", 0, "Int", 0, "UInt", 0x008800C6)
    GlowPut(bumpWin[i], x, y, bw, bh, canvas, Min(255, alpha))
    if !bumpVis[i]
        OnTop(bumpWin[i]), bumpVis[i] := true
    if !pinned
        pinned := VdaCall(pPin, "Ptr", bumpWin[1].Hwnd, "Int") != -1 && VdaCall(pPin, "Ptr", bumpWin[2].Hwnd, "Int") != -1
}

; ---- The glow round the window in your hand ----
; While the dock is open, the window you hold is ringed with the same light, hugging its rounded
; corners (square when maximized). A window can be screen-sized, so the ring isn't one big bitmap:
; it's four thin bands, top and bottom (with the corners) and left and right, each its own layered
; window. The colours are one diagonal flow, C(x + y + time) over the whole ring, which every band
; samples at its own offset: they meet without a seam. Straight stretches use a 1-pixel-long mask,
; stretched by GDI.
; Nothing touches the window while it moves: Windows tells us it moved only after it has, so a ring
; that follows lags a frame behind, and reacting to every move made the whole drag stutter. So the
; ring steps aside as soon as the window moves (checked once a frame), and fades back in where it
; has settled.
global winGlow := [], winGlowOn := false, winGlowHwnd := 0, winGlowAt := 0
global winGlowGeo := 0, winGlowStillAt := 0, winGlowShown := false
; On Windows 11 the window's own border (DWMWA_BORDER_COLOR, the one the accent colour uses) also
; takes the glow's colours: Windows draws it, glued to the window. Changed only while the window is
; still, a few times a second: each change makes Windows redraw the window's frame.
global WIN_BORDER := WIN_BUILD >= 22000, winBorderColor := -1
Loop 4                                              ; top, bottom, left, right
    winGlow.Push(Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0"))
global WIN_GLOW := 12, WIN_GLOW_PERIOD := 900       ; haze reach, and one colour cycle, in pixels at 100 %

WinGlowShow(hwnd) {
    global winGlowOn, winGlowHwnd, winGlowAt, winGlowGeo, winGlowStillAt, winGlowShown
    static pinned := false
    if (!hwnd || winGlowOn)
        return
    winGlowHwnd := hwnd, winGlowAt := A_TickCount, winGlowOn := true
    winGlowGeo := 0, winGlowStillAt := A_TickCount, winGlowShown := false
    if !pinned
        for g in winGlow
            pinned := VdaCall(pPin, "Ptr", g.Hwnd, "Int") != -1
    WinGlowFrame()
}

; Where the ring goes for the held window: the window as you see it (without the invisible borders),
; plus the haze all round. 0 if Windows can't say.
WinGlowRect() {
    rect := Buffer(16, 0)
    if DllCall("dwmapi\DwmGetWindowAttribute", "Ptr", winGlowHwnd, "UInt", 9, "Ptr", rect, "UInt", 16, "Int") != 0
        return 0
    s := A_ScreenDPI / 96, mg := Round(WIN_GLOW * s), rad := Round(8 * s)   ; Windows 11's corner radius
    try rad := WinGetMinMax(winGlowHwnd) = 1 ? 0 : rad                    ; square when maximized
    wl := NumGet(rect, 0, "Int"), wt := NumGet(rect, 4, "Int")
    return {x0: wl - mg, y0: wt - mg, fw: NumGet(rect, 8, "Int") - wl + 2 * mg, fh: NumGet(rect, 12, "Int") - wt + 2 * mg
        , mg: mg, rad: rad, cs: mg + rad}
}

WinGlowHide() {
    global winGlowOn, winGlowShown
    if !winGlowOn
        return
    for g in winGlow
        g.Hide()
    if WIN_BORDER
        WinBorder(winGlowHwnd, -1)                  ; its usual border again
    winGlowOn := false, winGlowShown := false
}

; Sets a window's border colour (0xRRGGBB), or -1 for Windows' default. Windows 11 only.
WinBorder(hwnd, rgb) {
    global winBorderColor
    if (rgb = winBorderColor && rgb != -1)
        return
    winBorderColor := rgb
    ref := rgb = -1 ? 0xFFFFFFFF : ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)   ; DWMWA_COLOR_DEFAULT, or COLORREF (BGR)
    try DllCall("dwmapi\DwmSetWindowAttribute", "Ptr", hwnd, "UInt", 34, "UInt*", ref, "UInt", 4, "Int")   ; DWMWA_BORDER_COLOR
}
OnExit((*) => WinGlowHide())

WinGlowFrame() {
    global winGlowGeo, winGlowStillAt, winGlowShown
    static borderAt := 0
    if !winGlowOn || !(r := WinGlowRect())
        return
    g := winGlowGeo, now := A_TickCount
    if (g && (r.x0 != g.x0 || r.y0 != g.y0 || r.fw != g.fw || r.fh != g.fh)) {   ; moving: out of the way
        winGlowStillAt := now
        if winGlowShown {
            for b in winGlow
                b.Hide()
            winGlowShown := false
        }
    }
    winGlowGeo := r
    if (now - winGlowStillAt < 120)                 ; until it has settled
        return
    if (WIN_BORDER && now - borderAt >= 150) {      ; the border: one colour at a time, round the palette
        borderAt := now
        WinBorder(winGlowHwnd, GlowColorAt(Mod(now, 4000), 4000, 0))
    }
    fw := r.fw, fh := r.fh, mg := r.mg, rad := r.rad, cs := r.cs, sh := fh - 2 * cs
    if (sh < 1 || fw < 2 * cs + 1)
        return
    per := Round(WIN_GLOW_PERIOD * A_ScreenDPI / 96), per2 := Round(per * 0.62), shift := GLOW_COLORS.Length // 2
    o1 := Mod(A_TickCount * per // GLOW_FLOW_MS, per)                ; one flow towards the top left...
    o2 := per2 - 1 - Mod(A_TickCount * per2 // GLOW_FLOW2_MS, per2)  ; ...the other towards the bottom right
    ms := WinGlowMasks(fw, rad, mg)
    ta := WinGlowStrip(fw, cs, per, 0, false), tb := WinGlowStrip(fw, cs, per2, shift, false)
    sa := WinGlowStrip(cs, sh, per, 0, true), sb := WinGlowStrip(cs, sh, per2, shift, true)
    k := Min(1, (now - Max(winGlowAt, winGlowStillAt + 120)) / 200)   ; fades in where it settled
    alpha := Round(230 * k * k * (3 - 2 * k))
    x0 := r.x0, y0 := r.y0
    WinGlowBand(winGlow[1], x0, y0, fw, cs, ta, o1, 0, tb, o2, 0, ms.top, fw, cs, alpha)
    WinGlowBand(winGlow[2], x0, y0 + fh - cs, fw, cs, ta, Mod(o1 + fh - cs, per), 0, tb, Mod(o2 + fh - cs, per2), 0, ms.bottom, fw, cs, alpha)
    WinGlowBand(winGlow[3], x0, y0 + cs, cs, sh, sa, 0, Mod(o1 + cs, per), sb, 0, Mod(o2 + cs, per2), ms.left, cs, 1, alpha)
    WinGlowBand(winGlow[4], x0 + fw - cs, y0 + cs, cs, sh, sa, 0, Mod(o1 + fw, per), sb, 0, Mod(o2 + fw, per2), ms.right, cs, 1, alpha)
    if !winGlowShown {                              ; just under the dock: the dock stays above everything
        for b in winGlow
            DllCall("SetWindowPos", "Ptr", b.Hwnd, "Ptr", bar ? bar.Hwnd : -1, "Int", 0, "Int", 0, "Int", 0, "Int", 0, "UInt", 0x53)
        winGlowShown := true
    }
}

; One band: colours (two layers mixed), times its mask (stretched along a straight side), on screen.
WinGlowBand(g, x, y, w, h, sa, ax, ay, sb, bx, by, mk, mw, mh, alpha) {
    canvas := GlowCanvas(w, h)
    DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h, "Ptr", sa.dc, "Int", ax, "Int", ay, "UInt", 0x00CC0020)
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
        , "Ptr", sb.dc, "Int", bx, "Int", by, "Int", w, "Int", h, "UInt", 150 << 16)
    DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
        , "Ptr", mk.inv, "Int", 0, "Int", 0, "Int", mw, "Int", mh, "UInt", (1 << 24) | (255 << 16))
    DllCall("StretchBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
        , "Ptr", mk.keep, "Int", 0, "Int", 0, "Int", mw, "Int", mh, "UInt", 0x008800C6)   ; SRCAND
    GlowPut(g, x, y, w, h, canvas, alpha)
}

; The bands' masks for a ring fw wide: top and bottom (fw × cs, corners included), left and right
; (cs × 1, stretched down the sides). Same light as round the dock: bright on the window's edge, a
; haze mg wide outside, a few pixels inside.
WinGlowMasks(fw, rad, mg) {
    static cache := Map()
    key := fw "|" rad "|" mg
    if cache.Has(key)
        return cache[key]
    if (cache.Count > 8)                            ; one per window size: keep a few
        cache.Clear()
    cs := mg + rad, inner := Max(2, Round(4 * A_ScreenDPI / 96))
    Edge(d) {                                       ; m (0-255) at distance d outside the window's edge
        v := d >= 0 ? GlowFall(d, mg) : d > -inner ? GlowFall(0, mg) * (1 + d / inner) : 0
        return Round(255 * Min(1, v))
    }
    Corner(px, py) {                                ; in the top-left corner box
        qx := cs - px - 0.5, qy := cs - py - 0.5
        return Edge((qx > 0 && qy > 0 ? Sqrt(qx * qx + qy * qy) : Max(qx, qy)) - rad)
    }
    Mask(w, h) {
        inv := NewDib(w, h, &ib), keep := NewDib(w, h, &kb)
        Fill32(ib, w * h, 0xFF000000), Fill32(kb, w * h, 0x00FFFFFF)
        return {inv: inv, keep: keep, ib: ib, kb: kb, w: w}
    }
    Put(mk, px, py, m) {
        o := (py * mk.w + px) * 4
        NumPut("UInt", (255 - m) << 24, mk.ib, o), NumPut("UInt", (m << 24) | 0xFFFFFF, mk.kb, o)
    }
    top := Mask(fw, cs), bottom := Mask(fw, cs), left := Mask(cs, 1), right := Mask(cs, 1)
    Loop cs {
        t := A_Index - 1, m := Edge(cs - t - 0.5 - rad)   ; a straight side, t pixels in from the frame's edge
        if m {
            Fill32(top.ib + (t * fw + cs) * 4, fw - 2 * cs, (255 - m) << 24), Fill32(top.kb + (t * fw + cs) * 4, fw - 2 * cs, (m << 24) | 0xFFFFFF)
            Fill32(bottom.ib + ((cs - 1 - t) * fw + cs) * 4, fw - 2 * cs, (255 - m) << 24), Fill32(bottom.kb + ((cs - 1 - t) * fw + cs) * 4, fw - 2 * cs, (m << 24) | 0xFFFFFF)
            Put(left, t, 0, m), Put(right, cs - 1 - t, 0, m)
        }
        Loop cs {                                   ; the corners, mirrored from the top-left one
            c := A_Index - 1, m := Corner(c, t)
            if m
                Put(top, c, t, m), Put(top, fw - 1 - c, t, m), Put(bottom, c, cs - 1 - t, m), Put(bottom, fw - 1 - c, cs - 1 - t, m)
        }
    }
    cache[key] := {top: top, bottom: bottom, left: left, right: right}
    return cache[key]
}

; Opaque colours for a band, C(x + y) with the colours cycling once per period: a horizontal band
; is (w + period) × h, a vertical one w × (h + period), so a slice at any offset fits.
WinGlowStrip(w, h, period, shift, vertical) {
    static cache := Map()
    key := w "|" h "|" period "|" shift "|" vertical
    if cache.Has(key)
        return cache[key]
    if (cache.Count > 16)
        cache.Clear()
    sw := vertical ? w : w + period, sh := vertical ? h + period : h
    line := Buffer((sw + sh) * 4)                   ; C(0), C(1), …: row r is this line from r on
    Loop sw + sh
        NumPut("UInt", 0xFF000000 | GlowColorAt(A_Index - 1, period, shift), line, (A_Index - 1) * 4)
    dc := NewDib(sw, sh, &bits)
    Loop sh
        DllCall("RtlMoveMemory", "Ptr", bits + (A_Index - 1) * sw * 4, "Ptr", line.Ptr + (A_Index - 1) * 4, "UPtr", sw * 4)
    cache[key] := {dc: dc}
    return cache[key]
}

; Shows a glow window on top of everything, without activating it.
OnTop(g) => DllCall("SetWindowPos", "Ptr", g.Hwnd, "Ptr", -1, "Int", 0, "Int", 0, "Int", 0, "Int", 0, "UInt", 0x53)   ; TOPMOST; NOSIZE|NOMOVE|NOACTIVATE|SHOWWINDOW

SetTimer(WarmBar, -5000)

; ---------------------------------------------------------------------
;  Taskbar: show apps from all desktops (Windows' own setting, Windows 10 and 11)
; ---------------------------------------------------------------------
; By default the taskbar only marks apps open on the current desktop. Windows' setting
; "Show all open windows on the taskbar: On all desktops" is VirtualDesktopTaskbarFilter = 0.
; The Store app can't change it: a packaged app's registry writes go to a private copy, so
; there the menu item opens the Settings page instead.
global EXPLORER_ADV := "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"

TaskbarAllDesktops() => RegRead(EXPLORER_ADV, "VirtualDesktopTaskbarFilter", 1) = 0

ToggleTaskbarAllDesktops(itemName, *) {
    on := !TaskbarAllDesktops()
    RegWrite(on ? 0 : 1, "REG_DWORD", EXPLORER_ADV, "VirtualDesktopTaskbarFilter")
    DllCall("SendMessageTimeout", "Ptr", 0xFFFF, "UInt", 0x1A, "Ptr", 0, "Str", "TraySettings"
        , "UInt", 2, "UInt", 1000, "Ptr*", 0)       ; WM_SETTINGCHANGE: the taskbar re-reads it
    TaskbarAllDesktops() ? A_TrayMenu.Check(itemName) : A_TrayMenu.Uncheck(itemName)
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
A_TrayMenu.Add("Hold a window to move it between desktops", ToggleHold)
if holdOn
    A_TrayMenu.Check("Hold a window to move it between desktops")
A_TrayMenu.Add("Performance mode (lighter animations)", TogglePerf)
if perfMode
    A_TrayMenu.Check("Performance mode (lighter animations)")
if PACKAGED {
    A_TrayMenu.Add("Taskbar: show apps from all desktops…", (*) => Run("ms-settings:multitasking"))
} else {
    A_TrayMenu.Add("Taskbar: show apps from all desktops", ToggleTaskbarAllDesktops)
    if TaskbarAllDesktops()
        A_TrayMenu.Check("Taskbar: show apps from all desktops")
}
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
;  Mouse: scroll on the taskbar (registered before any call into the DLL)
; ---------------------------------------------------------------------
; EzDesk has no keyboard shortcuts on purpose: everything is done with the mouse.
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
    SetTimer(Welcome, -3000)                        ; give Windows time to place the new tray icon

; Windows decides whether a new tray icon is shown or hidden under ^, and apps can't change it.
; If it's hidden, the welcome tip opens the Settings page where it's one switch to turn on.
; Plain words and one concrete example, no shortcut notation: people read a toast in two seconds.
Welcome() {
    shown := TrayIconShown()
    if (shown = 0) {                                ; the one thing that matters first: see the number
        OpenTrayIconSettings()
        TrayTip(WIN_BUILD >= 22000
            ? "Open Other system tray icons in the window that just opened, and turn on EzDesk. Your desktop number will show next to the clock."
            : "Turn on EzDesk in the window that just opened. Your desktop number will show next to the clock."
            , "One last step", "Mute")
        return
    }
    tip := "Hold a window by its title bar for a moment: drop it on the desktop you want."
    tip .= "`nScroll on the taskbar to go from one desktop to the next."
    if (shown = -1)
        tip .= "`nDon't see the number next to the clock? Click ^ and drag it out."
    TrayTip(tip, "EzDesk is ready", "Mute")
}

OpenTrayIconSettings() {
    if (WIN_BUILD >= 22000)
        Run("ms-settings:taskbar")                  ; Windows 11: Taskbar > Other system tray icons
    else
        Run("explorer.exe shell:::{05d7b0f4-2121-4eff-bf6b-ed3f69b894d9}")   ; Windows 10: Select which icons appear on the taskbar
}

; 1 = our icon is on the taskbar, 0 = hidden under ^, -1 = can't tell.
TrayIconShown() {
    p := TrayPromoted()
    if (p != -1)
        return p
    if (WIN_BUILD >= 22000)                         ; Windows 11 reports a hidden icon at the ^ button, inside the tray too
        return -1
    ; Windows 10: a hidden icon sits in the (closed) overflow window, outside the taskbar's tray area.
    nid := Buffer(A_PtrSize = 8 ? 40 : 28, 0)       ; NOTIFYICONIDENTIFIER: cbSize, hWnd, uID (AutoHotkey's is 1028)
    NumPut("UInt", nid.Size, nid, 0)
    NumPut("Ptr", A_ScriptHwnd, nid, A_PtrSize = 8 ? 8 : 4)
    NumPut("UInt", 1028, nid, A_PtrSize = 8 ? 16 : 8)
    r := Buffer(16, 0)
    if DllCall("Shell32\Shell_NotifyIconGetRect", "Ptr", nid, "Ptr", r, "Int")
        return -1
    taskbar := WinExist("ahk_class Shell_TrayWnd")
    if !taskbar
        return -1
    return OverTrayArea(taskbar, (NumGet(r, 0, "Int") + NumGet(r, 8, "Int")) // 2, (NumGet(r, 4, "Int") + NumGet(r, 12, "Int")) // 2) ? 1 : 0
}

; Windows 11 (22H2+) keeps one key per tray icon under NotifyIconSettings, found by exe path.
; IsPromoted = 1 means shown on the taskbar. Paths under known folders are stored as {folder GUID}\rest.
TrayPromoted() {
    try {
        Loop Reg "HKCU\Control Panel\NotifyIconSettings", "K" {
            key := A_LoopRegKey "\" A_LoopRegName
            try path := RegRead(key, "ExecutablePath")
            catch
                continue
            if (ExpandKnownFolder(path) = A_AhkPath)
                return RegRead(key, "IsPromoted", 0) = 1 ? 1 : 0
        }
    }
    return -1
}

ExpandKnownFolder(path) {
    if !RegExMatch(path, "^(\{[0-9A-Fa-f-]{36}\})(.*)$", &m)
        return path
    guid := Buffer(16)
    if DllCall("ole32\CLSIDFromString", "Str", m[1], "Ptr", guid)
        return path
    if DllCall("shell32\SHGetKnownFolderPath", "Ptr", guid, "UInt", 0, "Ptr", 0, "Ptr*", &p := 0)
        return path
    folder := StrGet(p, "UTF-16")
    DllCall("ole32\CoTaskMemFree", "Ptr", p)
    return folder m[2]
}

SetTimer(UpdateTray, -100)                          ; first contact with the DLL happens after start-up is complete
