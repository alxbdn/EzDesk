; An in-memory adapter for the Desktops module (lib\Desktops.ahk): desktops, windows on them and
; pins, with every call written to log so a test can check what was done, and in what order.
; Switches happen at once. canCreate / canRemove: as on the Windows 11 build; false like Windows 10.
class FakeDesktops {
    __New(count := 3, current := 0, canCreate := true, canRemove := true) {
        this.n := count, this.cur := current, this.canCreate := canCreate, this.canRemove := canRemove
        this.on := Map(), this.pinned := Map(), this.names := Map(), this.log := [], this.broken := false
    }

    Has(op) => op = "Create" ? this.canCreate : op = "Remove" ? this.canRemove : true

    Count() => this.n
    Current() => this.cur
    Name(i) => this.names.Has(i) ? this.names[i] : ""
    WindowDesktop(hwnd) => this.pinned.Has(hwnd) ? this.cur : this.on.Has(hwnd) ? this.on[hwnd] : -1
    IsOnCurrent(hwnd) => this.WindowDesktop(hwnd) = this.cur ? 1 : 0
    Listen(hwnd, msg) => 1

    Move(hwnd, i) {
        this.log.Push("move " hwnd " " i), this.on[hwnd] := i
        return 1
    }
    Pin(hwnd) {
        this.log.Push("pin " hwnd), this.pinned[hwnd] := true
        return 1
    }
    Unpin(hwnd) {
        this.log.Push("unpin " hwnd)
        if this.pinned.Has(hwnd)
            this.pinned.Delete(hwnd)
        return 1
    }
    Create() {
        this.log.Push("create"), this.n += 1
        return this.n - 1
    }
    Remove(i, fallback) {
        this.log.Push("remove " i " " fallback), this.n -= 1
        if (this.cur >= this.n)
            this.cur := this.n - 1
        return 1
    }
    SlideLeft() => (this.log.Push("left"), this.cur -= 1)
    SlideRight() => (this.log.Push("right"), this.cur += 1)
    SlideNew() => (this.log.Push("new"), this.n += 1, this.cur := this.n - 1)
    Wait(ms) => 0

    Says() {                                        ; the log, as one line
        s := ""
        for line in this.log
            s .= (A_Index > 1 ? ", " : "") line
        return s
    }
}
