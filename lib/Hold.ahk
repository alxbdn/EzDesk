; =====================================================================
;  The hold gesture: what holding a window means, from the press to the drop
; =====================================================================
; It's told what happens (a title bar pressed, Windows starting or ending a drag, and every frame:
; the time, the mouse button, what the dock shows under the cursor) and answers with what to do,
; as a list of intents. It never reads the mouse, a window or the clock itself, and does nothing on
; screen, so a test can play a whole gesture with made-up times.
;
; Intents, {do: …}:
;   grab    {hwnd}           browsing the dock: the window just pressed glows at once, in front
;   open    {hwnd, x, y}     held long enough: open the dock (on the screen at x, y)
;   carry   {hwnd}           a drag started: pin it, so it comes along through a switch
;   close                    let go without ever moving: nothing to drop, the dock goes
;   letGo   {hwnd}           same while browsing: the dock stays, only that window lets go
;   slide   {to}             held over a desktop in the dock for a second: go there (-2: a new one)
;   step    {side}           held in a side bump: one desktop that way (past the last: a new one)
;   drop    {hwnd, target, carried, from}   the drag ended: target is what the dock showed under
;                            the cursor (desktop index, -2 for "+", -1 for none)
;
; Phases: idle → pressed (button down on a title bar, Windows hasn't seen it move) → dragging
; (Windows is moving it) → idle. A drag can also start without a press (the press wasn't seen).

class HoldGesture {
    static OPEN_MS := 300                           ; held this long, moving or not: the dock opens
    static EDGE_MS := 750                           ; in a side bump this long: switch (macOS's workspaces-edge-delay)
    static SPRING_MS := 1000                        ; over a desktop in the dock this long: go there

    __New() {
        this.Clear()
    }

    Clear() {
        this.phase := "idle", this.hwnd := 0, this.x := 0, this.y := 0, this.since := 0
        this.from := 0, this.carried := false, this.browsing := false, this.target := -1
        this.edgeSide := 0, this.edgeSince := 0, this.springOn := -1, this.springSince := 0
    }

    ; The window being dragged by Windows right now, or 0.
    Dragging => this.phase = "dragging" ? this.hwnd : 0

    ; A title bar pressed. from: where the window was ({max, x, y, w, h}), to put it back on a drop.
    ; browsing: the dock is open already, from the tray icon.
    Press(hwnd, x, y, now, from, browsing := false) {
        if (this.phase != "idle")
            return []
        this.Clear()
        this.phase := "pressed", this.hwnd := hwnd, this.x := x, this.y := y, this.since := now
        this.from := from, this.browsing := browsing
        return browsing ? [{do: "grab", hwnd: hwnd}] : []
    }

    ; Windows started moving a window (it only says so once the mouse has moved a few pixels).
    DragStart(hwnd, x, y, now, from) {
        if (this.phase = "dragging")
            return []
        if !(this.phase = "pressed" && hwnd = this.hwnd)    ; a press already counts from when it was picked up
            this.x := x, this.y := y, this.since := now, this.browsing := false
        this.phase := "dragging", this.hwnd := hwnd, this.from := from, this.carried := true
        this.edgeSide := 0, this.edgeSince := 0, this.springOn := -1
        return [{do: "carry", hwnd: hwnd}]
    }

    ; Windows says the drag of hwnd ended: the mouse was released.
    DragEnd(hwnd) {
        if (this.phase != "dragging" || hwnd != this.hwnd)
            return []
        out := [{do: "drop", hwnd: hwnd, target: this.target, carried: this.carried, from: this.from}]
        this.Clear()
        return out
    }

    ; Every frame. f: {now, down (the left button), dockOpen, target (what the dock shows under the
    ; cursor: desktop index, -2 for "+", -1 for none), current (the desktop you're on), edge (the
    ; side bump the cursor is in: -1, +1, or 0)}.
    Tick(f) {
        if (this.phase = "idle")
            return []
        this.target := f.dockOpen ? f.target : -1
        if (this.phase = "pressed" && !f.down) {    ; let go without ever moving
            out := [this.browsing ? {do: "letGo", hwnd: this.hwnd} : {do: "close"}]
            this.Clear()
            return out
        }
        if (this.phase = "dragging" && !f.down)     ; released: Windows' end of the drag is on its way
            return []
        out := []
        if (!f.dockOpen && f.now - this.since >= HoldGesture.OPEN_MS)
            out.Push({do: "open", hwnd: this.hwnd, x: this.x, y: this.y})
        if (this.phase != "dragging" || !f.dockOpen)
            return out
        ; Over a desktop in the dock (not the one you're on): a second there and you slide to it.
        if (this.target = -1 || this.target = f.current) {
            this.springOn := -1
        } else if (this.target != this.springOn) {
            this.springOn := this.target, this.springSince := f.now
        } else if (f.now - this.springSince >= HoldGesture.SPRING_MS) {
            out.Push({do: "slide", to: this.target})
            this.springOn := -1
        }
        ; In a side bump: the cursor has to stay there the whole time, leaving starts the count again.
        if (f.edge != this.edgeSide)
            this.edgeSide := f.edge, this.edgeSince := f.now
        if (f.edge != 0 && f.now - this.edgeSince >= HoldGesture.EDGE_MS) {
            out.Push({do: "step", side: f.edge})
            this.edgeSince := f.now                 ; still held there: the next one 0.75 s later
        }
        return out
    }

    ; How full of light the bump on that side is (0-1): how long the cursor has been in it.
    EdgeFill(side, now) {
        if (this.phase != "dragging" || side != this.edgeSide || !this.edgeSince)
            return 0
        return Min(1, (now - this.edgeSince) / HoldGesture.EDGE_MS)
    }
}
