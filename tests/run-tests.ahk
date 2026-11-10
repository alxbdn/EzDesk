#Requires AutoHotkey v2.0
#Warn All, Off
; Tests for EzDesk's modules (lib\), through their interfaces only. Run:
;   EzDesk.exe /ErrorStdOut tests\run-tests.ahk
; It prints one line per failure and a summary, and exits 1 if anything failed. Nothing is shown on
; screen and Windows' desktops are never touched: the Desktops module runs on FakeDesktops.

#Include %A_ScriptDir%\..\lib\Dib.ahk
#Include %A_ScriptDir%\..\lib\Glow.ahk
#Include %A_ScriptDir%\..\lib\Desktops.ahk
#Include %A_ScriptDir%\..\lib\Hold.ahk
#Include %A_ScriptDir%\FakeDesktops.ahk

global passed := 0, failures := []

Eq(name, got, want) {
    global passed
    if (got = want)
        passed += 1
    else
        failures.Push(name ": got " got ", want " want)
}
Near(name, got, want, tol := 0.001) => Eq(name, Abs(got - want) <= tol ? "~" want : got, "~" want)

; Intents as one line, e.g. "open, carry": what a test compares.
Says(intents) {
    s := ""
    for it in intents {
        s .= (A_Index > 1 ? ", " : "") it.do
        if it.HasOwnProp("to")
            s .= " " it.to
        if it.HasOwnProp("side")
            s .= " " it.side
        if (it.do = "drop")
            s .= " " it.target (it.carried ? " carried" : "")
    }
    return s
}

; A frame as HoldTick makes it.
Fr(now, down := true, dockOpen := false, target := -1, current := 0, edge := 0)
    => {now: now, down: down, dockOpen: dockOpen, target: target, current: current, edge: edge}

PLACE := {max: false, x: 10, y: 20, w: 800, h: 600}

; ---------------------------------------------------------------------
;  Hold gesture
; ---------------------------------------------------------------------
TestHold() {
    ; A quick click on a title bar: no dock.
    h := HoldGesture()
    Eq("click: press", Says(h.Press(1, 0, 0, 0, PLACE)), "")
    Eq("click: still down", Says(h.Tick(Fr(100))), "")
    Eq("click: released", Says(h.Tick(Fr(150, false))), "close")
    Eq("click: idle after", h.phase, "idle")

    ; Held still: the dock opens at 300 ms, not before, where it was pressed.
    h := HoldGesture()
    h.Press(1, 500, 700, 0, PLACE)
    Eq("hold: 299 ms", Says(h.Tick(Fr(299))), "")
    out := h.Tick(Fr(300))
    Eq("hold: 300 ms", Says(out), "open")
    Eq("hold: opens where pressed", out[1].x "," out[1].y, "500,700")
    Eq("hold: once open, no more", Says(h.Tick(Fr(320, true, true))), "")

    ; A press that becomes a drag keeps its clock; the window is carried.
    h := HoldGesture()
    h.Press(7, 0, 0, 0, PLACE)
    Eq("drag: carried", Says(h.DragStart(7, 30, 0, 200, PLACE)), "carry")
    Eq("drag: still counts from the press", Says(h.Tick(Fr(300))), "open")
    Eq("drag: Dragging", h.Dragging, 7)

    ; A drag nobody pressed counts from its own start.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 1000, PLACE)
    Eq("drag alone: 1299", Says(h.Tick(Fr(1299))), "")
    Eq("drag alone: 1300", Says(h.Tick(Fr(1300))), "open")

    ; Dropped on a desktop in the dock: that one, carried.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    h.Tick(Fr(400, true, true, 2))
    out := h.DragEnd(7)
    Eq("drop: on 2", Says(out), "drop 2 carried")
    Eq("drop: from", out[1].from.w, 800)
    Eq("drop: idle after", h.phase, "idle")

    ; Dropped elsewhere, or dock closed: none.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    h.Tick(Fr(100, true, false, 2))                  ; the dock isn't open: what it would show doesn't count
    Eq("drop: dock closed", Says(h.DragEnd(7)), "drop -1 carried")

    ; Another window's drag ending is not ours.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    Eq("drop: other window", Says(h.DragEnd(8)), "")

    ; Over a desktop in the dock for a second: slide there. Not over the current one.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    Eq("spring: starts", Says(h.Tick(Fr(1000, true, true, 2, 0))), "")
    Eq("spring: 999 ms", Says(h.Tick(Fr(1999, true, true, 2, 0))), "")
    Eq("spring: 1 s", Says(h.Tick(Fr(2000, true, true, 2, 0))), "slide 2")
    Eq("spring: current one", Says(h.Tick(Fr(4000, true, true, 0, 0))), "")
    h.Tick(Fr(5000, true, true, 1, 0))
    h.Tick(Fr(5500, true, true, 2, 0))               ; moved on: the count starts again
    Eq("spring: moved on", Says(h.Tick(Fr(6000, true, true, 2, 0))), "")
    Eq("spring: +", Says((h.Tick(Fr(6400, true, true, -2, 0)), h.Tick(Fr(7400, true, true, -2, 0)))), "slide -2")

    ; In a side bump: one desktop every 0.75 s; leaving starts again.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    h.Tick(Fr(1000, true, true, -1, 1, 1))
    Eq("edge: fill half", Round(h.EdgeFill(1, 1375), 2), 0.5)
    Eq("edge: other side empty", h.EdgeFill(-1, 1375), 0)
    Eq("edge: 749", Says(h.Tick(Fr(1749, true, true, -1, 1, 1))), "")
    Eq("edge: 750", Says(h.Tick(Fr(1750, true, true, -1, 1, 1))), "step 1")
    Eq("edge: again 750 later", Says(h.Tick(Fr(2500, true, true, -1, 1, 1))), "step 1")
    h.Tick(Fr(2600, true, true, -1, 1, 0))
    h.Tick(Fr(2700, true, true, -1, 1, -1))
    Eq("edge: left, restarted", Says(h.Tick(Fr(3449, true, true, -1, 1, -1))), "")
    Eq("edge: left, 750", Says(h.Tick(Fr(3450, true, true, -1, 1, -1))), "step -1")

    ; Only a dragged window switches: held still on a title bar, the bump does nothing.
    h := HoldGesture()
    h.Press(1, 0, 0, 0, PLACE)
    h.Tick(Fr(400, true, true, 2, 0, 1))
    Eq("pressed: no step", Says(h.Tick(Fr(2000, true, true, 2, 0, 1))), "")
    Eq("pressed: no fill", h.EdgeFill(1, 2000), 0)

    ; Released mid-drag: nothing until Windows says the drag ended.
    h := HoldGesture()
    h.DragStart(7, 0, 0, 0, PLACE)
    Eq("drag released", Says(h.Tick(Fr(2000, false))), "")

    ; Browsing: the window glows at once; let go, only it lets go.
    h := HoldGesture()
    Eq("browse: grab", Says(h.Press(3, 0, 0, 0, PLACE, true)), "grab")
    Eq("browse: letGo", Says(h.Tick(Fr(100, false, true))), "letGo")

    ; Busy: a second press is ignored.
    h := HoldGesture()
    h.Press(1, 0, 0, 0, PLACE)
    h.Press(2, 0, 0, 50, PLACE)
    Eq("second press ignored", h.hwnd, 1)
}

; ---------------------------------------------------------------------
;  Desktops, on the fake
; ---------------------------------------------------------------------
TestDesktops() {
    ; Dropped on another desktop: moved first, then unpinned, and it left.
    f := FakeDesktops(3, 0), d := Desktops(f)
    Eq("drop other: left", d.Drop(5, 2, true), true)
    Eq("drop other: order", f.Says(), "move 5 2, unpin 5")

    ; On the current one, carried: back here, then unpinned.
    f := FakeDesktops(3, 1), d := Desktops(f)
    Eq("drop here: stays", d.Drop(5, 1, true), false)
    Eq("drop here: order", f.Says(), "move 5 1, unpin 5")

    ; Nowhere, not carried: nothing at all.
    f := FakeDesktops(3, 1), d := Desktops(f)
    d.Drop(5, -1, false)
    Eq("drop nowhere", f.Says(), "")

    ; On "+": a new desktop at the end, without going there.
    f := FakeDesktops(3, 0), d := Desktops(f)
    Eq("drop new: left", d.Drop(5, -2, true), true)
    Eq("drop new: order", f.Says(), "create, move 5 3, unpin 5")
    Eq("drop new: still here", f.cur, 0)

    ; Where a desktop can't be made without going there (Windows 10): it stays.
    f := FakeDesktops(3, 0, false), d := Desktops(f)
    Eq("no create: -1", d.Create(), -1)
    Eq("drop new, Windows 10", d.Drop(5, -2, true), false)
    Eq("drop new, Windows 10: back here", f.Says(), "move 5 0, unpin 5")

    ; Sliding stays inside the desktops; past the last only when asked, and it makes one.
    f := FakeDesktops(3, 0), d := Desktops(f)
    Eq("slide left of 1", d.Slide(-1), false)
    f.cur := 2
    Eq("slide right of last", d.Slide(1), false)
    Eq("slide right of last, or new", d.Slide(1, true), true)
    Eq("slide: made one", f.n " " f.cur, "4 3")
    Eq("slide: log", f.Says(), "new")

    ; GoTo: one slide at a time; -2 straight to a new one.
    f := FakeDesktops(4, 0), d := Desktops(f)
    d.GoTo(3)
    Eq("goto 3", f.Says(), "right, right, right")
    d.GoTo(1)
    Eq("goto back to 1", f.cur, 1)
    f.log := [], d.GoTo(-2)
    Eq("goto new", f.Says() " " f.cur, "new 4")
    f.log := [], d.GoTo(9)
    Eq("goto past the end", f.Says(), "")

    ; Closing: never the last one; windows go to the one before (the next for desktop 1).
    f := FakeDesktops(3, 0), d := Desktops(f)
    d.Remove(0)
    Eq("remove 1st", f.Says(), "remove 0 1")
    f := FakeDesktops(1, 0), d := Desktops(f)
    Eq("remove the only one", d.Remove(0), false)
    f := FakeDesktops(3, 0, true, false), d := Desktops(f)
    Eq("remove, Windows 10", d.Remove(1), false)

    ; Names and capabilities.
    f := FakeDesktops(3, 0), d := Desktops(f)
    f.names[1] := "Mail"
    Eq("label named", d.Label(1), "Mail")
    Eq("label default", d.Label(2), "Desktop 3")
    Eq("can carry", d.CanCarry, true)
    Eq("on current", d.IsOnCurrent(42), false)
    d.Pin(42)
    Eq("pinned: on current", d.IsOnCurrent(42), true)
}

; ---------------------------------------------------------------------
;  Glow: the pure numbers
; ---------------------------------------------------------------------
TestGlow() {
    Eq("fall: far out", GlowFall(10, 10), 0)
    Eq("fall: well inside", GlowFall(-1, 10), 0)
    Eq("fall: edge brighter than haze", GlowFall(0, 10) > GlowFall(5, 10), true)
    Eq("fall: haze fades", GlowFall(5, 10) > GlowFall(9, 10), true)
    Eq("colour: start", Format("{:06X}", GlowColorAt(0, 60, 0)), "0063B1")
    Eq("colour: half-way is white", Format("{:06X}", GlowColorAt(30, 60, 0)), "FFFFFF")
    Eq("colour: wraps", GlowColorAt(60, 60, 0), GlowColorAt(0, 60, 0))
    Eq("colour: shift by 3 = white", Format("{:06X}", GlowColorAt(0, 60, 3)), "FFFFFF")
    Eq("period 2", GlowPeriod2(100), 62)
    ; Rounded rectangle, circle corners: true distances.
    Near("side: outside", RoundRectDistance(-5, 13, 10), 3)
    Near("side: inside", RoundRectDistance(-5, 3, 10), -7)
    Near("corner: on the arc", RoundRectDistance(3, 4, 5), 0)
    Near("corner: 5 out", RoundRectDistance(6, 8, 5), 5)
    Near("corner, superellipse, on the axis", RoundRectDistance(0.000001, 10, 10, 4), 0, 0.01)
    ; The bell: full depth in the middle, gone at its ends, fading there.
    b := BellEdge(0, 20, 100)
    Near("bell: middle", b.x, 20)
    Near("bell: flat in the middle", b.norm, 1)
    Eq("bell: end", BellEdge(1, 20, 100), 0)
    Eq("bell: symmetric", BellEdge(-0.3, 20, 100).x, BellEdge(0.3, 20, 100).x)
    Eq("bell: fades near its end", BellEdge(0.9, 20, 100).fade < 1, true)
}

TestHold()
TestDesktops()
TestGlow()

for msg in failures
    FileAppend("FAIL " msg "`n", "*")
FileAppend(passed " passed, " failures.Length " failed`n", "*")
ExitApp(failures.Length ? 1 : 0)
