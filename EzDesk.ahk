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
;    Tray icon             shows the current desktop number; click = the dock, to browse desktops
; =====================================================================
;
; Start-up order matters: icon, menu and mouse hooks are set up first, and the
; virtual-desktop DLL is only called afterwards, from a timer. If Windows'
; virtual desktops aren't reachable yet (login, Explorer restart) the app
; still shows its own icon, its menu still works and Exit still exits.
;
; Four modules live in lib\, each with its own tests in tests\:
;   Desktops.ahk   Windows' virtual desktops, behind one seam (the DLL and Windows' shortcuts)
;   Hold.ahk       the hold gesture: from a press on a title bar to the drop, as intents
;   HeldWindow.ahk how the window in hand looks while it's held, and putting it all back
;   Glow.ahk       the flowing light round the dock and on the side bumps
; This file wires them to the mouse, the screen and the tray.

#Include %A_ScriptDir%\lib\Dib.ahk
#Include %A_ScriptDir%\lib\Glow.ahk
#Include %A_ScriptDir%\lib\Desktops.ahk
#Include %A_ScriptDir%\lib\Hold.ahk
#Include %A_ScriptDir%\lib\HeldWindow.ahk

; Our icon right away, never AutoHotkey's green "H".
TraySetIcon(A_ScriptDir "\icons\1.ico")
A_IconTip := "EzDesk (starting…)"

; VirtualDesktopAccessor: Windows 11 and Windows 10 need different builds.
global WIN_BUILD := Integer(StrSplit(A_OSVersion, ".")[3])
global desk := Desktops(VdaAdapter(A_ScriptDir (WIN_BUILD >= 22000 ? "\VirtualDesktopAccessor.dll" : "\VirtualDesktopAccessor-win10.dll"), WIN_BUILD))
GlowShape.pin := (hwnd) => desk.Pin(hwnd)           ; the glows stay on screen through a switch

; Focuses the top window of the current desktop (after the active one left for another desktop).
FocusTopWindow() {
    for hwnd in WinGetList() {                      ; top-most first
        if IsRealWindow(hwnd) && desk.IsOnCurrent(hwnd) {
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

; Scrolling on the taskbar.
Step(delta) {
    static last := 0
    if (A_TickCount - last < 250)                   ; touchpads send bursts of wheel events
        return
    last := A_TickCount
    desk.Slide(delta)
}

; ---------------------------------------------------------------------
;  Holding a window: take it to another desktop
; ---------------------------------------------------------------------
; Windows announces the start and end of every window drag (EVENT_SYSTEM_MOVESIZESTART/END), and a
; press on a title bar is seen as it happens (a drag is only announced once the mouse has moved a
; few pixels). Both go to the hold gesture (lib\Hold.ahk); while a window is in hand, HoldTick
; tells it what the mouse and the dock are doing every 15 ms, and does what it answers.
; The window is pinned (shown on every desktop) from the start of the drag, so switching neither
; hides it nor ends the drag; on release it's unpinned onto the desktop it's dropped on.
global SETTINGS := A_AppData "\EzDesk\settings.ini"         ; per-package private copy when packaged
global holdOn := IniRead(SETTINGS, "EzDesk", "HoldWindow", 1) = 1       ; the dock and its side bumps
global perfMode := IniRead(SETTINGS, "EzDesk", "Performance", 0) = 1    ; lighter animation, always
global trayReminder := IniRead(SETTINGS, "EzDesk", "TrayReminder", 1) = 1   ; say so when the tray icon is hidden
global hold := HoldGesture()
global inHand := HeldWindow(WinOps(WIN_BUILD >= 22000), RaiseBumps)   ; how it looks while held (lib\HeldWindow.ahk)
OnExit((*) => inHand.Release())                    ; never leave a window see-through or on top behind us

; Where a window is, to put it back there when it's dropped on the dock.
WindowPlace(hwnd) {
    try {
        WinGetPos(&x, &y, &w, &h, hwnd)
        return {max: WinGetMinMax(hwnd) = 1, x: x, y: y, w: w, h: h}
    }
    return 0
}

OnMoveSize(hook, event, hwnd, idObject, idChild, thread, time) {
    if (idObject != 0)                              ; OBJID_WINDOW only
        return
    if (event = 0x000A) {                           ; drag started
        if !holdOn || !desk.CanCarry || !DraggedByTitle(hwnd)
            return
        CoordMode("Mouse", "Screen")
        MouseGetPos(&x, &y)
        Perform(hold.DragStart(hwnd, x, y, A_TickCount, WindowPlace(hwnd)))
        SetTimer(HoldTick, 15)                      ; often enough for smooth pulses
    } else if (event = 0x000B) {                    ; drag ended: mouse released
        Perform(hold.DragEnd(hwnd))
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

; Every 15 ms while a window is in hand, or the dock is open from the tray: one frame of the dock,
; then the gesture is told what happened, then what it answers is done. Drawing comes first, so a
; desktop switch never happens in the middle of a frame.
HoldTick() {
    if (hold.phase = "idle" && !(browseOn && barShown)) {
        SetTimer(HoldTick, 0)
        return
    }
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    edge := barShown ? BarTrack(x, y) : 0
    Perform(hold.Tick({now: A_TickCount, down: GetKeyState("LButton", "P"), dockOpen: barShown
        , target: BarTarget(), current: barShown ? desk.Current() : -1, edge: edge}))
}

; Does what the hold gesture says (lib\Hold.ahk lists its intents).
Perform(intents) {
    global barHeld
    for it in intents {
        switch it.do {
            case "grab":
                barHeld := it.hwnd, inHand.Grab(it.hwnd, A_TickCount)
            case "open":
                BarShow(MonitorAt(it.x, it.y), it.hwnd)
            case "carry":
                desk.Pin(it.hwnd)                   ; at the drag's start, not at the switch: a window pinned just
                                                    ; before the slide goes off with the old desktop and pops back
            case "close":
                BarHide()
            case "letGo":
                inHand.Release()
            case "slide":
                desk.GoTo(it.to)
                SetTimer(BarRefresh, -450)          ; once the slide is done, the dock shows where you are
            case "step":
                if desk.Slide(it.side, true) {
                    bumpSquash[it.side < 0 ? 1 : 2] := 1   ; the little bounce
                    SetTimer(BarRefresh, -450)
                }
            case "drop":
                BarHide()
                hwnd := it.hwnd, target := it.target, carried := it.carried, from := it.from
                SetTimer(() => DropWindow(hwnd, target, carried, from), -1)   ; out of the event callback first
        }
    }
}

; A held window let go. Dropped on the dock (target: desktop index, -2 for "+"), it goes there,
; back where it was picked up, and you stay where you are, like on a Mac. Else it stays on the
; desktop you ended up on.
DropWindow(hwnd, target, carried, from) {
    if (target != -1 && from) {
        try {
            if from.max {
                WinMaximize(hwnd)
            } else {
                if (WinGetMinMax(hwnd) = 1)         ; Windows maximized it, dropped near the top
                    WinRestore(hwnd)
                WinMove(from.x, from.y, from.w, from.h, hwnd)
            }
        }
    }
    if desk.Drop(hwnd, target, carried)
        FocusTopWindow()                            ; it left: the next window here gets the focus
    else if carried
        try WinActivate(hwnd)
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

; A press on a title bar (HTCAPTION) goes to the hold gesture right away, so a window held without
; moving at all opens the dock too. Holding the button down in a page or a document stays what it is.
Hotkey("~*LButton", PressStart)                     ; ~: the click goes through untouched

PressStart(*) {
    if (!holdOn || !desk.CanCarry || hold.phase != "idle" || (barShown && !browseOn))
        return
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y, &under)
    if (browseOn && x >= barBox.x && x < barBox.x + barBox.w && y >= barBox.y && y < barBox.y + barBox.h)
        return                                      ; on the dock: its tiles take the click
    hwnd := under ? DllCall("GetAncestor", "Ptr", under, "UInt", 2, "Ptr") : 0   ; GA_ROOT: the top-level window
    hit := 0
    if (hwnd && IsRealWindow(hwnd))
        try hit := SendMessage(0x84, 0, ((y & 0xFFFF) << 16) | (x & 0xFFFF), , hwnd, , , , 200)   ; WM_NCHITTEST
    if (hit != 2) {                                 ; not a title bar (HTCAPTION)
        if browseOn && !OnTaskbar(hwnd)             ; browsing, clicked elsewhere: close (the tray icon toggles it itself)
            SetTimer(BarHide, -1)
        return
    }
    Perform(hold.Press(hwnd, x, y, A_TickCount, WindowPlace(hwnd), browseOn))
    SetTimer(HoldTick, 15)
}

; The taskbar and its tray (where the icon is), including the overflow under ^.
OnTaskbar(hwnd) {
    if !hwnd
        return false
    try cls := WinGetClass(hwnd)
    catch
        return false
    return cls = "Shell_TrayWnd" || cls = "Shell_SecondaryTrayWnd" || cls = "NotifyIconOverflowWindow"
        || cls = "TopLevelWindowForOverflowXamlIsland"
}

; ---------------------------------------------------------------------
;  Desktop bar: hold a window without moving, drop it on a desktop
; ---------------------------------------------------------------------
; Like the Spaces bar at the top of macOS Mission Control, without the overview: your windows stay
; where they are. Keep a window held (mouse button down on it) for a moment, and a
; bar of desktops appears at the bottom of that screen, above the taskbar (the top belongs to Windows'
; own snapping), each a live miniature: its wallpaper and its
; windows, drawn by Windows itself (DWM thumbnails). On Windows 11 a "+" makes a new desktop. Drop
; the window on one: it goes there, back where you picked it up, and you stay where you are.
; Dropped anywhere else, it's an ordinary drop and the bar goes away.
; The bar is a dark panel with well-rounded corners, the glow's light
; flowing all round it for as long as it's open.
global bar := 0, barShown := false, barTiles := [], barThumbs := [], barHover := 0, barHeld := 0, barCloseHot := false
global barBase := {x: 0, y: 0}

; ---- Browsing: the dock opened by clicking the tray number, with no window in hand ----
; Click a desktop to go there, its × (top right, on hover) to close it, + to make one. Grab a window
; by its title bar and it glows at once: drop it on a desktop to send it there. Clicking anywhere
; else, or the number again, closes the dock.
global browseOn := false

ToggleBrowse(*) {
    global browseOn
    if barShown {
        BarHide()
        return
    }
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    m := MonitorAt(x, y)
    BarShow(m ? m : MonitorGetPrimary(), 0)
    if barShown {
        browseOn := true
        DeckDrawOverlay()                           ; with the × now
        SetTimer(HoldTick, 15)
    }
}

; The close button of a tile: a small squircle in its top-right corner, in pixels from the tile's
; top-left. What's drawn (DeckDrawOverlay) and what takes the click (CloseHit) are this one square.
CloseBox(tile) {
    s := A_ScreenDPI / 96, size := Round(20 * s), inset := Round(4 * s)
    return {x: tile.w - size - inset, y: inset, size: size}
}

; Whether (x, y), on screen, is on a tile's close button (a little margin round it: easier to hit).
CloseHit(tile, x, y) {
    if (!desk.CanRemove || tile.target < 0 || desk.Count() < 2)
        return false
    b := CloseBox(tile), m := Round(3 * A_ScreenDPI / 96)
    lx := x - barBox.x - tile.x, ly := y - barBox.y - tile.y
    return lx >= b.x - m && lx < b.x + b.size + m && ly >= b.y - m && ly < b.y + b.size + m
}

; A GDI+ path round a rounded rectangle (corner arcs d across): the squircles and the hover frame.
RoundPath(x, y, w, h, d) {
    path := 0
    DllCall("gdiplus\GdipCreatePath", "Int", 0, "Ptr*", &path)
    for arc in [[x, y, 180], [x + w - d, y, 270], [x + w - d, y + h - d, 0], [x, y + h - d, 90]]
        DllCall("gdiplus\GdipAddPathArc", "Ptr", path, "Float", arc[1], "Float", arc[2], "Float", d, "Float", d, "Float", arc[3], "Float", 90)
    DllCall("gdiplus\GdipClosePathFigure", "Ptr", path)
    return path
}

BrowseClick(idx, *) {
    if !browseOn || idx > barTiles.Length
        return
    tile := barTiles[idx]
    CoordMode("Mouse", "Screen")
    MouseGetPos(&x, &y)
    if CloseHit(tile, x, y) {                       ; ×: close that desktop (its windows go to a neighbour)
        desk.Remove(tile.target)
        SetTimer(BarRefresh, -350)
        return
    }
    target := tile.target
    BarHide()
    desk.GoTo(target)                               ; +: a new desktop, then there
}

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
    th := Max(Round(90 * s), Round(mh * 0.085))     ; about a twelfth of the screen's height
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
    ; Uninterruptible: building the dock takes a moment (glass, wallpapers, miniatures), and a timer or
    ; a drag ending must not run in the middle of it (two docks stacked, or one torn down mid-build).
    Critical
    if (intro && barShown)
        return
    count := desk.Count(), cur := desk.Current()
    if (count <= 0 || cur < 0)
        return
    n := count + (desk.CanCreate ? 1 : 0)           ; "+" only where a desktop can be made without going there
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
    oldBar := bar, oldThumbs := barThumbs
    ; A plain dark panel with rounded corners (a window region); the names are drawn in the glow's
    ; layer above (DeckDrawOverlay).
    bar := Gui("-Caption +AlwaysOnTop +ToolWindow -DPIScale +E0x08000000", "EzDesk desktops")   ; NOACTIVATE: the drag goes on
    bar.BackColor := "2B2C34", bar.MarginX := 0, bar.MarginY := 0
    barTiles := [], barThumbs := [], barHover := 0
    Loop n {
        i := A_Index - 1, x := pad + i * (tw + lay.gap), isNew := i >= count
        at := "x" x " y" pad " w" tw " h" th
        if isNew {
            bar.SetFont("s20 norm cC8C8D2", "Segoe UI Light")
            ctl := bar.AddText(at " Center +0x200 Background3A3B45", "+")   ; SS_CENTERIMAGE: centred both ways
            name := "New desktop"
        } else {
            hbm := WallBitmap(DesktopWallpaper(i), tw, th)
            ctl := hbm ? bar.AddPicture(at, "HBITMAP:*" hbm) : bar.AddText(at " Background3A3B45")
            name := desk.Label(i)
        }
        barTiles.Push({x: x, y: pad, w: tw, h: th, target: isNew ? -2 : i, name: name, current: i = cur, ctl: ctl})   ; names: drawn by DeckDrawOverlay
        ctl.OnEvent("Click", BrowseClick.Bind(A_Index))   ; when browsing: go there, or close it
    }
    bar.Show("Hide x" sx " y" sy " w" lay.bw " h" lay.bh)    ; sized, shaped, then shown
    DllCall("SetWindowRgn", "Ptr", bar.Hwnd, "Ptr", DllCall("CreateRoundRectRgn", "Int", 0, "Int", 0, "Int", lay.bw + 1, "Int", lay.bh + 1
        , "Int", 2 * rad, "Int", 2 * rad, "Ptr"), "Int", false)   ; rounded corners; Windows owns the region now
    bar.Show("NA x" sx " y" sy " w" lay.bw " h" lay.bh)
    desk.Pin(bar.Hwnd)                              ; on every desktop: it stays put through a switch
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
    if intro
        BumpsOpen()
    DeckGlowFrame()
    deckGlow.Raise()                                ; the glow over the dock
    inHand.Grab(held, A_TickCount)                  ; the window in hand over both (lit), the bumps over it
}

; The cursor on the dock, or close to it (anywhere below its top, give or take 40 px).
DockNear(x, y) {
    near := 40 * A_ScreenDPI / 96
    return barShown && x >= barBox.x - near && x < barBox.x + barBox.w + near && y >= barBox.y - near
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
        d := desk.WindowDesktop(hwnd)
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

; One frame of the open dock, cursor at (x, y): its motion, its glow, the bumps, the shrinking
; window, and the frame round the tile under the cursor. Returns the side bump the cursor is in.
; On a slow PC (or one that's busy for a while) a frame can take longer than the 15 ms between two:
; then the glows round the dock and the window are redrawn every other frame only (the bumps, which
; do the switching, still every frame), until frames are quick again. Decided from the average
; time of the last full frames.
BarTrack(x, y) {
    global barHover, barCloseHot
    static freq := 0, avg := 0, odd := false, autoLite := false
    if !freq
        DllCall("QueryPerformanceFrequency", "Int64*", &freq)
    DllCall("QueryPerformanceCounter", "Int64*", &t0 := 0)
    odd := !odd
    autoLite := avg > 12 ? true : avg < 9 ? false : autoLite  ; on and off again as the PC chokes and recovers
    full := !(autoLite || perfMode) || odd          ; Performance mode (tray menu): always light
    DeckMotion()
    if full
        DeckGlowFrame()
    edge := BumpFrame(x, y)
    inHand.Frame(x, y, A_TickCount, DockNear(x, y), hold.Dragging != 0, full)   ; its border, its picture over the dock
    DllCall("QueryPerformanceCounter", "Int64*", &t1 := 0)
    ms := (t1 - t0) * 1000 / freq
    if (full && ms < 40)                            ; full frames only (else skipping would hide the load); one-offs don't count
        avg += (ms - avg) * 0.1
    box := barBox, hit := 0, half := box.gap // 2
    for tile in barTiles
        if (x >= box.x + tile.x - half && x < box.x + tile.x + tile.w + half && y >= box.y - half)   ; anywhere below the bar's top
            hit := A_Index
    hot := browseOn && hit && CloseHit(barTiles[hit], x, y)   ; on a close button: it lights up
    if (hit != barHover || hot != barCloseHot) {
        barHover := hit, barCloseHot := hot
        DeckDrawOverlay()                           ; the frame moves to the tile under the cursor
    }
    return edge
}

; The desktop under the cursor in the dock: its index, -2 for "+", -1 for none.
BarTarget() => barShown && barHover ? barTiles[barHover].target : -1

BarHide() {
    global bar, barShown, barTiles, barThumbs, barHover, browseOn
    Critical
    browseOn := false
    if !barShown
        return
    for thumb in barThumbs
        DllCall("dwmapi\DwmUnregisterThumbnail", "Ptr", thumb)
    try bar.Destroy()
    deckGlow.Hide()
    BumpsHide()
    inHand.Release()
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

; Scaling big wallpapers and making the glows' masks takes a moment: done ahead of time, so the
; dock shows at once.
WarmBar() {
    if !desk.CanCarry || (count := desk.Count()) <= 0
        return
    lay := BarLayout(MonitorGetPrimary(), count + (desk.CanCreate ? 1 : 0))
    Loop count
        WallBitmap(DesktopWallpaper(A_Index - 1), lay.tw, lay.th)
    deckGlow.Warm(DeckMask(lay.bw, lay.bh, DeckRadius(lay), Round(DECK_GLOW * lay.s)))
    Loop BUMP_LEVELS {
        BumpMask(A_Index - 1, -1)
        bumpGlow[2].Warm(BumpMask(A_Index - 1, 1))
    }
}

; ---- The deck: its outline and its glow ----
global deckGlow := GlowShape("x")                   ; the colours flow along the dock
global deckGeom := {x: 0, y: 0, fw: 0, fh: 0, bw: 0, bh: 0, rad: 0, mg: 0}, deckShownAt := 0
global DECK_N := 2                                  ; corner shape: 2 = circle arcs, like Windows' own
global DECK_GLOW := 7                               ; how far the haze reaches out round the deck, in pixels at 100 %

DeckRadius(lay) => Round(lay.bh * 0.24)             ; well rounded, growing with the dock
DeckMask(bw, bh, rad, mg) => GlowRoundRect(bw, bh, rad, mg, DECK_N)

; One frame of the glow round the deck, with the names and the hover frame over it.
DeckGlowFrame() {
    if !barShown
        return
    g := deckGeom, mask := DeckMask(g.bw, g.bh, g.rad, g.mg)
    over := (deckOverlay && deckOverlay.fw = mask.w && deckOverlay.fh = mask.h) ? deckOverlay.dc : 0
    k := Min(1, (A_TickCount - deckShownAt) / 200)  ; fades in as it rises
    deckGlow.Draw(mask, g.x, g.y, Round(240 * k * k * (3 - 2 * k)), {over: over})
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
        tile := barTiles[barHover], e := 3 * s, pen := 0
        DllCall("gdiplus\GdipCreatePen1", "UInt", 0xFF4A9BE0, "Float", 2.5 * s, "Int", 2, "Ptr*", &pen)
        path := RoundPath(g.mg + tile.x - e, g.mg + tile.y - e, tile.w + 2 * e, tile.h + 2 * e, 16 * s)
        DllCall("gdiplus\GdipDrawPath", "Ptr", gfx, "Ptr", pen, "Ptr", path)
        DllCall("gdiplus\GdipDeletePath", "Ptr", path), DllCall("gdiplus\GdipDeletePen", "Ptr", pen)
        if (browseOn && desk.CanRemove && tile.target >= 0 && barTiles.Length > (desk.CanCreate ? 2 : 1))   ; browsing: × to close it
            DrawCloseButton(gfx, tile, g.mg, s, barCloseHot)
    }
    DllCall("gdiplus\GdipDeleteStringFormat", "Ptr", fmt), DllCall("gdiplus\GdipDeleteFontFamily", "Ptr", fam)
    DllCall("gdiplus\GdipDeleteGraphics", "Ptr", gfx), DllCall("gdiplus\GdipDisposeImage", "Ptr", bmp)
    deckOverlay := o
}

; A tile's close button: a dark squircle with a white ×. Under the cursor it lights up like the
; desktop number does on a switch: the glow's blue round it, a soft haze outside, so you see
; exactly where the click goes.
DrawCloseButton(gfx, tile, mg, s, lit) {
    b := CloseBox(tile), x := mg + tile.x + b.x, y := mg + tile.y + b.y, sz := b.size, d := sz * 0.6
    path := RoundPath(x, y, sz, sz, d), brush := 0, pen := 0, xp := 0
    if lit {                                        ; the haze, then the bright rim
        DllCall("gdiplus\GdipCreatePen1", "UInt", 0x702A82CC, "Float", 5 * s, "Int", 2, "Ptr*", &pen)
        DllCall("gdiplus\GdipDrawPath", "Ptr", gfx, "Ptr", pen, "Ptr", path), DllCall("gdiplus\GdipDeletePen", "Ptr", pen)
    }
    DllCall("gdiplus\GdipCreateSolidFill", "UInt", lit ? 0xFF0063B1 : 0xE616171C, "Ptr*", &brush)
    DllCall("gdiplus\GdipFillPath", "Ptr", gfx, "Ptr", brush, "Ptr", path), DllCall("gdiplus\GdipDeleteBrush", "Ptr", brush)
    DllCall("gdiplus\GdipCreatePen1", "UInt", lit ? 0xFF8CC0EC : 0x50FFFFFF, "Float", (lit ? 1.8 : 1) * s, "Int", 2, "Ptr*", &pen)
    DllCall("gdiplus\GdipDrawPath", "Ptr", gfx, "Ptr", pen, "Ptr", path), DllCall("gdiplus\GdipDeletePen", "Ptr", pen)
    DllCall("gdiplus\GdipDeletePath", "Ptr", path)
    cx := x + sz / 2, cy := y + sz / 2, k := 3.2 * s
    DllCall("gdiplus\GdipCreatePen1", "UInt", 0xFFFFFFFF, "Float", 1.6 * s, "Int", 2, "Ptr*", &xp)
    DllCall("gdiplus\GdipDrawLine", "Ptr", gfx, "Ptr", xp, "Float", cx - k, "Float", cy - k, "Float", cx + k, "Float", cy + k)
    DllCall("gdiplus\GdipDrawLine", "Ptr", gfx, "Ptr", xp, "Float", cx + k, "Float", cy - k, "Float", cx - k, "Float", cy + k)
    DllCall("gdiplus\GdipDeletePen", "Ptr", xp)
}

; ---- The side bumps: where you take the window to the previous / next desktop ----
; While the dock is open, a small glowing bump sits on each side of the screen, half-way down.
; Bring the window towards a side and that bump swells,
; smoothly with the distance, from 200 px away. Into the bump (about 20 px from the edge) and hold:
; it fills with light for 0.75 s, then
; you slide to that desktop, window in hand, and it gives a little bounce. Windows' Snap still works
; all along: let go against the edge before the 0.75 s and the window snaps there. On desktop 1 there's no
; left bump: nothing that way. Past the last desktop, the right one makes a new desktop.
; Shape: a bell (GlowBell), filled, with the glow's haze round it. Its size follows q = closeness
; (+ bounce): BUMP_LEVELS masks, made once.
global bumpGlow := [GlowShape("y", 2, 100), GlowShape("y", 2, 100)]   ; [left, right]: slower light, back on top every 100 ms
global bumpShown := false, bumpOpenAt := 0
global bumpSquash := [0, 0], bumpAvail := [0, 1]    ; [left, right]
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

; The mask for size level k (0 … BUMP_LEVELS - 1), on the left (side -1) or right (+1) edge.
BumpMask(k, side) {
    g := BumpGeom(), q := k / (BUMP_LEVELS - 1) * BUMP_QMAX
    return GlowBell(g.bw, g.bh, BumpDepth(q), BumpHalf(q), g.mg, side)
}

BumpPlus() {                                        ; the "+" on the last desktop's bump: dark, opaque, made once
    static p := 0
    if !p {
        s := A_ScreenDPI / 96, size := Round(12 * s), t := Max(1, Round(1.6 * s)), o := (size - t) // 2   ; a thin +
        dc := NewDib(size, size, &bits)             ; starts transparent
        Loop t {
            r := o + A_Index - 1
            Fill32(bits + r * size * 4, size, 0xFF16171C)              ; the bar across
            Loop size
                NumPut("UInt", 0xFF16171C, bits, ((A_Index - 1) * size + r) * 4)   ; the bar down
        }
        p := {dc: dc, size: size}
    }
    return p
}

BumpsOpen() {
    global bumpSquash, bumpAvail, bumpOpenAt, bumpShown
    bumpSquash := [0, 0], bumpAvail := [desk.Current() > 0 ? 1 : 0, 1]
    bumpOpenAt := A_TickCount, bumpShown := true
}

RaiseBumps() {                                      ; back over everything, the window in hand included
    for b in bumpGlow
        if b.visible
            b.Raise()
}

BumpsHide() {
    global bumpShown
    for b in bumpGlow
        b.Hide()
    bumpShown := false
}

; One frame of both bumps, cursor at (x, y). Returns the side the cursor is in (-1 left, +1 right,
; 0 neither): held there with a window, the hold gesture switches desktop.
BumpFrame(x, y) {
    global bumpSquash, bumpAvail
    if !bumpShown
        return 0
    g := BumpGeom(), now := A_TickCount
    MonitorGetWorkArea(barBox.mon, &wl, &wt, &wr, &wb)
    cur := desk.Current(), count := desk.Count()
    yc := (wt + wb) / 2                             ; fixed, half-way down the screen
    contact := 0
    Loop 2 {
        i := A_Index, side := i = 1 ? -1 : 1
        bumpAvail[i] += ((side < 0 ? cur > 0 : 1) - bumpAvail[i]) * 0.12   ; desktop 1: the left one tucks away
        dist := side < 0 ? x - wl : wr - 1 - x
        near := Max(0, Min(1, 1 - dist / (BUMP_REACH * g.s))), p := 1 - (1 - near) ** 3
        grow := BumpSpring((now - bumpOpenAt - 120 - (i - 1) * 60) / 600) * bumpAvail[i]
        bumpSquash[i] *= 0.88
        q := Min(BUMP_QMAX, Max(0, (p + 0.4 * bumpSquash[i]) * grow))
        if (bumpAvail[i] > 0.5 && dist >= -1 && dist <= BumpDepth(q) + 2 * g.s)
            contact := side
        alpha := Round(255 * (0.45 + 0.55 * p) * Min(1, Max(0, grow) * 1.5))
        if (alpha < 3) {
            if bumpGlow[i].visible
                bumpGlow[i].Hide()
            continue
        }
        stamp := 0
        plus := (side > 0 && cur + 1 >= count) ? Min(1, Max(0, (q - 0.45) / 0.4)) : 0   ; last desktop: this one makes a new one
        if (plus > 0) {                             ; a black "+" in the bump's belly
            pl := BumpPlus(), cx := side > 0 ? g.bw - BumpDepth(q) * 0.55 : BumpDepth(q) * 0.55
            stamp := {dc: pl.dc, size: pl.size, x: Round(cx - pl.size / 2), y: (g.bh - pl.size) // 2, alpha: Round(255 * plus)}
        }
        bumpGlow[i].Draw(BumpMask(Round(q / BUMP_QMAX * (BUMP_LEVELS - 1)), side)
            , side < 0 ? wl : wr - g.bw, Round(yc - g.bh / 2), alpha
            , {white: hold.EdgeFill(side, now), stamp: stamp})   ; held: it fills with light
    }
    return contact
}

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

global TASKBAR_ITEM := "Taskbar: show apps from all desktops"

ToggleTaskbarAllDesktops(itemName, *) => SetTaskbarAllDesktops(!TaskbarAllDesktops())

SetTaskbarAllDesktops(on) {
    RegWrite(on ? 0 : 1, "REG_DWORD", EXPLORER_ADV, "VirtualDesktopTaskbarFilter")
    DllCall("SendMessageTimeout", "Ptr", 0xFFFF, "UInt", 0x1A, "Ptr", 0, "Str", "TraySettings"
        , "UInt", 2, "UInt", 1000, "Ptr*", 0)       ; WM_SETTINGCHANGE: the taskbar re-reads it
    try TaskbarAllDesktops() ? A_TrayMenu.Check(TASKBAR_ITEM) : A_TrayMenu.Uncheck(TASKBAR_ITEM)
}

; The first time you switch desktops while the taskbar only shows the current desktop's apps, a
; notification offers to show them all (once, ever). Clicked: the zip and admin versions turn
; Windows' setting on right away; the Store version opens its Settings page.
OfferTaskbarAllDesktops() {
    if TaskbarAllDesktops() || IniRead(SETTINGS, "EzDesk", "TaskbarOffered", 0)
        return
    try {
        DirCreate(A_AppData "\EzDesk")
        IniWrite(1, SETTINGS, "EzDesk", "TaskbarOffered")
    }
    Notify("Apps open on your other desktops don't show on the taskbar. Click here to see them all, wherever you are."
        , "See all your apps on the taskbar", ShowAllApps)
}

ShowAllApps() {
    if PACKAGED {
        Run("ms-settings:multitasking")
        TrayTip('Under Desktops, set "On the taskbar, show all the open windows" to "On all desktops".', "One switch to change", "Mute")
    } else {
        SetTaskbarAllDesktops(true)
        TrayTip("The taskbar now shows the apps of every desktop. Clicking one takes you to its desktop.", "Done", "Mute")
    }
}

; A notification that does something when clicked: Windows tells the tray icon (AHK_NOTIFYICON,
; 0x404) with NIN_BALLOONUSERCLICK (0x405); the action waits until then, and only for that one.
global noteAction := 0, noteIgnored := 0
Notify(text, title, action, ignored := 0) {
    global noteAction := action, noteIgnored := ignored
    TrayTip(text, title, "Mute")
}
NotifyClicked(wParam, lParam, *) {
    global noteAction, noteIgnored
    msg := lParam & 0xFFFF
    if (msg = 0x405 && noteAction) {                ; NIN_BALLOONUSERCLICK
        act := noteAction, noteAction := 0, noteIgnored := 0
        SetTimer(act, -1)                           ; out of the message handler first
    } else if (msg = 0x404 && noteAction) {         ; NIN_BALLOONTIMEOUT: closed or gone without a click
        ign := noteIgnored, noteAction := 0, noteIgnored := 0
        if ign
            SetTimer(ign, -1)
    }
}
OnMessage(0x404, NotifyClicked)

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
A_TrayMenu.Add("Show desktops", ToggleBrowse)
A_TrayMenu.Add("Task View", (*) => Send("#{Tab}"))
A_TrayMenu.Add()
A_TrayMenu.Add("Hold a window to move it between desktops", ToggleHold)
if holdOn
    A_TrayMenu.Check("Hold a window to move it between desktops")
A_TrayMenu.Add("Performance mode (lighter animations)", TogglePerf)
if perfMode
    A_TrayMenu.Check("Performance mode (lighter animations)")
A_TrayMenu.Add("Remind me when this icon is hidden", ToggleTrayReminder)
if trayReminder
    A_TrayMenu.Check("Remind me when this icon is hidden")
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
A_TrayMenu.Default := "Show desktops"
A_TrayMenu.ClickCount := 1                          ; single left-click on the number opens the dock

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
    i := desk.Current(), c := desk.Count()
    if desk.Broken {                                ; incompatible DLL: stop polling, the menu keeps working
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
        desk.Listen(A_ScriptHwnd, MSG_DESKTOP_CHANGED)
        OnMessage(MSG_DESKTOP_CHANGED, UpdateTray)
        SetTimer(UpdateTray, 1000)
    }
    if (i = shownIndex && c = shownCount)
        return
    switched := shownIndex >= 0 && i != shownIndex  ; a real switch, not the first reading
    if switched
        SetTimer(OfferTaskbarAllDesktops, -1500)    ; once the slide is over
    shownIndex := i, shownCount := c
    n := i + 1
    icon := A_ScriptDir "\icons\" (n <= 9 ? n : "more") ".ico"
    TraySetIcon(icon)
    if switched
        TrayFlash(icon)
    name := desk.Name(i)
    A_IconTip := "Desktop " n " of " c (name != "" ? " — " name : "")
}

; A desktop switch lights the number up for a moment: a blue rim, whitening at its peak, that pulses on and
; off in about 0.6 s. The frames are drawn once per number and size (GDI+ over the icon)
; and kept.
TrayFlash(path) {
    static frames := Map(), count := 10
    sz := DllCall("GetSystemMetrics", "Int", 49)     ; SM_CXSMICON: the tray's icon size
    key := path "|" sz
    if !frames.Has(key) {
        list := []
        Loop count
            list.Push(TrayFlashFrame(path, sz, Sin(3.14159 * A_Index / (count + 1))))   ; up, then down
        frames[key] := list
    }
    list := frames[key], step := 0
    Next() {
        step += 1
        if (step > list.Length || !list[step]) {
            SetTimer(Next, 0)
            TraySetIcon(path)                       ; back to the plain number
            return
        }
        TraySetIcon("HICON:*" list[step])
    }
    SetTimer(Next, 60)
}

; The number's icon with the glow's rim over it, k = 0…1 strong. Returns an icon handle (0 on failure).
TrayFlashFrame(path, sz, k) {
    hIcon := LoadPicture(path, "w" sz " h" sz, &kind)
    if !hIcon
        return 0
    bi := Buffer(40, 0)
    NumPut("UInt", 40, bi, 0), NumPut("Int", sz, bi, 4), NumPut("Int", -sz, bi, 8), NumPut("UShort", 1, bi, 12), NumPut("UShort", 32, bi, 14)
    bits := 0
    hbm := DllCall("CreateDIBSection", "Ptr", 0, "Ptr", bi, "UInt", 0, "Ptr*", &bits, "Ptr", 0, "UInt", 0, "Ptr")
    dc := DllCall("CreateCompatibleDC", "Ptr", 0, "Ptr"), old := DllCall("SelectObject", "Ptr", dc, "Ptr", hbm, "Ptr")
    DllCall("DrawIconEx", "Ptr", dc, "Int", 0, "Int", 0, "Ptr", hIcon, "Int", sz, "Int", sz, "UInt", 0, "Ptr", 0, "UInt", 3)
    DllCall("SelectObject", "Ptr", dc, "Ptr", old), DllCall("DeleteDC", "Ptr", dc), DllCall("DestroyIcon", "Ptr", hIcon)
    GdipStart()
    bmp := 0, gfx := 0, brush := 0, pen := 0, path2 := 0
    DllCall("gdiplus\GdipCreateBitmapFromScan0", "Int", sz, "Int", sz, "Int", sz * 4, "Int", 0xE200B, "Ptr", bits, "Ptr*", &bmp)
    DllCall("gdiplus\GdipGetImageGraphicsContext", "Ptr", bmp, "Ptr*", &gfx)
    DllCall("gdiplus\GdipSetSmoothingMode", "Ptr", gfx, "Int", 4)
    a := Round(255 * k) << 24, p1 := Buffer(8), p2 := Buffer(8)
    NumPut("Float", 0, "Float", 0, p1), NumPut("Float", sz, "Float", sz, p2)
    c := GlowColorAt(k * 1.2, 6, 0)                 ; the glow's blue, brightening a little at the peak (never white)
    DllCall("gdiplus\GdipCreateSolidFill", "UInt", a | c, "Ptr*", &brush)
    w := Max(1.5, sz / 9), r := sz / 4, x := w / 2, d := 2 * r, e := sz - w
    DllCall("gdiplus\GdipCreatePen2", "Ptr", brush, "Float", w, "Int", 2, "Ptr*", &pen)
    DllCall("gdiplus\GdipCreatePath", "Int", 0, "Ptr*", &path2)
    for arc in [[x, x, 180], [x + e - d, x, 270], [x + e - d, x + e - d, 0], [x, x + e - d, 90]]
        DllCall("gdiplus\GdipAddPathArc", "Ptr", path2, "Float", arc[1], "Float", arc[2], "Float", d, "Float", d, "Float", arc[3], "Float", 90)
    DllCall("gdiplus\GdipClosePathFigure", "Ptr", path2)
    DllCall("gdiplus\GdipDrawPath", "Ptr", gfx, "Ptr", pen, "Ptr", path2)
    DllCall("gdiplus\GdipDeletePath", "Ptr", path2), DllCall("gdiplus\GdipDeletePen", "Ptr", pen), DllCall("gdiplus\GdipDeleteBrush", "Ptr", brush)
    DllCall("gdiplus\GdipDeleteGraphics", "Ptr", gfx), DllCall("gdiplus\GdipDisposeImage", "Ptr", bmp)
    mask := DllCall("CreateBitmap", "Int", sz, "Int", sz, "UInt", 1, "UInt", 1, "Ptr", 0, "Ptr")   ; all-zero mask: the alpha decides
    ii := Buffer(8 + A_PtrSize * 3, 0)               ; ICONINFO
    NumPut("Int", 1, ii, 0), NumPut("Ptr", mask, ii, 8 + A_PtrSize), NumPut("Ptr", hbm, ii, 8 + A_PtrSize * 2)
    h := DllCall("CreateIconIndirect", "Ptr", ii, "Ptr")
    DllCall("DeleteObject", "Ptr", mask), DllCall("DeleteObject", "Ptr", hbm)
    return h
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
; Every start checks that the desktop number is on the taskbar (the very first one with the welcome
; tip); a few seconds in, so Windows has placed the icon.
if (!STARTED_AT_LOGIN && FirstRun())
    SetTimer(Welcome, -3000)
else
    SetTimer(CheckTrayIcon, -8000)

; Windows decides whether a tray icon is shown or hidden under ^, and apps can't change it. If it's
; hidden, a notification says so; clicked, it opens the Settings page where it's one switch to turn on.
; Never nagging: it stops for good when "Remind me when this icon is hidden" is unticked in the
; tray menu, or after three reminders in a row closed or left without a click.
CheckTrayIcon() {
    if (!trayReminder || IniRead(SETTINGS, "EzDesk", "TrayIgnored", 0) >= 3 || TrayIconShown() != 0)
        return
    Notify("Your desktop number is hidden under ^ next to the clock. Click here to keep it in view."
        , "Show EzDesk next to the clock", ShowTrayIcon, TrayReminderIgnored)
}

TrayReminderIgnored() {
    try {
        DirCreate(A_AppData "\EzDesk")
        IniWrite(IniRead(SETTINGS, "EzDesk", "TrayIgnored", 0) + 1, SETTINGS, "EzDesk", "TrayIgnored")
    }
}

ToggleTrayReminder(itemName, *) {
    global trayReminder := !trayReminder
    SaveSetting("TrayReminder", trayReminder, itemName)
    try IniWrite(0, SETTINGS, "EzDesk", "TrayIgnored")   ; ticked again: a fresh start
}

ShowTrayIcon() {
    try IniWrite(0, SETTINGS, "EzDesk", "TrayIgnored")
    OpenTrayIconSettings()
    TrayTip(WIN_BUILD >= 22000
        ? "Open Other system tray icons in the window that just opened, and turn on EzDesk."
        : "Turn on EzDesk in the window that just opened."
        , "One switch to turn on", "Mute")
}

; Plain words and one concrete example, no shortcut notation: people read a toast in two seconds.
Welcome() {
    shown := TrayIconShown()
    if (shown = 0) {                                ; the one thing that matters first: see the number
        CheckTrayIcon()
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
