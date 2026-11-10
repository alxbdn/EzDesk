; An in-memory adapter for the HeldWindow module (lib\HeldWindow.ahk): windows that only exist as
; their state (on top, border, transparency), with every call written to calls. kind: what
; Picture() answers ("picture", "fallback" or "skip").
; Careful with names: AutoHotkey ignores case, and a method is a property, so a field can't be
; called like a method (border / Border).
class FakeWinOps {
    __New(canBorder := true, kind := "picture") {
        this.canBorder := canBorder, this.kind := kind, this.scale := 1
        this.tops := Map(), this.borders := Map(), this.alphas := Map(), this.places := Map()
        this.calls := [], this.shown := false, this.live := 0, this.lastW := 0
    }

    IsTopmost(hwnd) => this.tops.Has(hwnd) && this.tops[hwnd]
    SetTopmost(hwnd, on) => (this.calls.Push((on ? "top " : "untop ") hwnd), this.tops[hwnd] := on)
    Border(hwnd, rgb) => (this.calls.Push("border " hwnd " " (rgb = -1 ? "default" : Format("{:06X}", rgb))), this.borders[hwnd] := rgb)
    Pos(hwnd) => this.places.Has(hwnd) ? this.places[hwnd] : "0|0|800|600"
    Alpha(hwnd, a) {
        this.calls.Push("alpha " hwnd " " a), this.alphas[hwnd] := a
        return true
    }
    Picture(hwnd, x, y) {
        this.calls.Push("picture " hwnd)
        if (this.kind != "picture")
            return {kind: this.kind, why: "fake " this.kind}
        this.live += 1
        return {kind: "picture", w: 800, h: 600, fx: 0.5, fy: 0.05}
    }
    ShowPicture(pic, x, y, w, h, alpha, first) {
        this.shown := true, this.lastW := w
        if first
            this.calls.Push("show")
    }
    HidePicture() => (this.calls.Push("hide"), this.shown := false)
    FreePicture(pic) => (this.calls.Push("free"), this.live -= 1)
    Log(msg) => 0

    Says() {
        s := ""
        for line in this.calls
            s .= (A_Index > 1 ? ", " : "") line
        return s
    }

    ; Whatever was changed on hwnd is back to how it was (given it wasn't on top before).
    Clean(hwnd) => !this.IsTopmost(hwnd) && (!this.borders.Has(hwnd) || this.borders[hwnd] = -1)
        && (!this.alphas.Has(hwnd) || this.alphas[hwnd] = "Off") && !this.shown && this.live = 0
}
