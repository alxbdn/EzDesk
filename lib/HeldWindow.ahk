; =====================================================================
;  The window in your hand: how it looks while it's held, and putting it back
; =====================================================================
; While a window is held, EzDesk changes it: always on top (in front of the dock), its border lit
; with the glow's blue (Windows 11), and over the dock it's swapped for a small see-through picture
; of itself, like on a Mac (or, when it can't be pictured, made half see-through). These are changes
; to another app's window, so they must all be undone, every time: Release() does, once, whatever
; was changed and only that. Grab a second window and the first one is released first.
;
;   Grab(hwnd, now)                           in front, lit; again for the same window: just back in front
;   Frame(x, y, now, overDock, dragging, full)   every frame (full: a full frame, the border may change)
;   Release()                                 everything back; safe any time
;
; The Windows side is an adapter: WinOps in the app, FakeWinOps in the tests (tests\FakeWinOps.ahk).
; Both answer: canBorder, scale, IsTopmost(hwnd), SetTopmost(hwnd, on), Border(hwnd, rgb or -1),
; Pos(hwnd) ("" when gone), Alpha(hwnd, 0-255 or "Off") → success, Picture(hwnd, x, y),
; ShowPicture(pic, x, y, w, h, alpha, first), HidePicture(), FreePicture(pic), Log(msg).
; Needs Glow.ahk (the border's colours).

class HeldWindow {
    static SHRINK_W := 260                          ; its picture's width over the dock, in pixels at 100 %
    static SEE := 115                               ; its opacity over the dock: 45 %, so the dock shows through
    static STILL_MS := 120, BORDER_MS := 150        ; the border changes only while it's still, a few times a second

    __New(ops, afterFront := 0) {                   ; afterFront: called once it's in front (the bumps go over it)
        this.ops := ops, this.afterFront := afterFront
        this.Forget()
    }

    Forget() {
        this.hwnd := 0, this.wasTop := false, this.lit := false, this.color := -1
        this.geo := "", this.stillAt := 0, this.borderAt := 0, this.shrink := 0
    }

    Grab(hwnd, now) {
        if !hwnd
            return
        if (hwnd != this.hwnd) {
            this.Release()
            this.hwnd := hwnd, this.wasTop := this.ops.IsTopmost(hwnd), this.lit := this.ops.canBorder
            this.stillAt := now
        }
        this.ops.SetTopmost(hwnd, true)             ; every time: back over a dock that was just rebuilt
        if this.afterFront
            this.afterFront.Call()
    }

    Frame(x, y, now, overDock, dragging, full) {
        if !this.hwnd
            return
        if (full && this.lit)
            this.BorderFrame(now)
        this.ShrinkFrame(x, y, overDock, dragging)
    }

    Release() {
        if !this.hwnd
            return
        this.ShrinkEnd()
        if this.lit
            this.ops.Border(this.hwnd, -1)          ; its usual border again
        if !this.wasTop
            this.ops.SetTopmost(this.hwnd, false)   ; it wasn't always on top before
        this.Forget()
    }

    ; ---- Its border: one colour at a time round the glow's palette, only while it's still ----
    ; Each change makes Windows redraw the window's frame, which made dragging lag.
    BorderFrame(now) {
        pos := this.ops.Pos(this.hwnd)
        if (pos = "")
            return
        if (pos != this.geo)                        ; moving: leave it alone
            this.geo := pos, this.stillAt := now
        if (now - this.stillAt < HeldWindow.STILL_MS || now - this.borderAt < HeldWindow.BORDER_MS)
            return
        this.borderAt := now
        rgb := GlowColorAt(Mod(now, 4000), 4000, 0)
        if (rgb != this.color)
            this.color := rgb, this.ops.Border(this.hwnd, rgb)
    }

    ; ---- Over the dock: see-through (45 %), and small while it's dragged ----
    ; Held still, the window itself goes see-through. Dragged, Windows itself places it, so it can't
    ; be resized mid-drag: the real one goes invisible, and its picture shrinks round the point you
    ; hold it by, fading to the same 45 %, follows the cursor, and grows back into the real window
    ; when it leaves the dock.
    ShrinkFrame(x, y, over, dragging) {
        if (over && !this.shrink)
            this.ShrinkStart(x, y, dragging)
        sh := this.shrink
        if !sh
            return
        if (sh.kind != "picture") {                 ; see-through only, or left alone: until it leaves the dock
            if !over
                this.ShrinkEnd()
            return
        }
        if !dragging {                              ; the drag ended over the dock: the real window, at once
            this.ShrinkEnd()
            return
        }
        sh.p += ((over ? 1 : 0) - sh.p) * 0.35      ; eases in and out
        if (!over && sh.p < 0.03) {
            this.ShrinkEnd()                        ; back to the real window
            return
        }
        pic := sh.pic, e := sh.p * sh.p * (3 - 2 * sh.p), small := Min(pic.w, HeldWindow.SHRINK_W * this.ops.scale)
        ew := Max(1, Round(pic.w + (small - pic.w) * e)), eh := Max(1, Round(ew * pic.h / pic.w))
        this.ops.ShowPicture(pic, Round(x - ew * pic.fx), Round(y - eh * pic.fy), ew, eh
            , Round(255 - (255 - HeldWindow.SEE) * e), !sh.shown)   ; see-through as it shrinks: the dock shows under it
        sh.shown := true
    }

    ShrinkStart(x, y, dragging) {
        if !dragging {                              ; held still: just see-through
            this.shrink := {kind: this.ops.Alpha(this.hwnd, HeldWindow.SEE) ? "faded" : "skip"}
            return
        }
        r := this.ops.Picture(this.hwnd, x, y)
        if (r.kind = "skip") {                      ; layered already (draws its own transparency), or too small
            if r.why
                this.ops.Log(r.why)
            this.shrink := {kind: "skip"}
        } else if (r.kind = "fallback") {           ; can't picture it: the window itself goes see-through
            this.ops.Log(r.why)
            this.shrink := {kind: this.ops.Alpha(this.hwnd, HeldWindow.SEE) ? "faded" : "skip"}
        } else {
            if !this.ops.Alpha(this.hwnd, 0)        ; the real one steps aside; its picture takes its place
                this.ops.Log("couldn't make it see-through")
            this.shrink := {kind: "picture", pic: r, p: 0, shown: false}
        }
    }

    ShrinkEnd() {
        sh := this.shrink
        if !sh
            return
        if (sh.kind = "picture") {
            this.ops.HidePicture()
            this.ops.FreePicture(sh.pic)
        }
        if (sh.kind != "skip")
            this.ops.Alpha(this.hwnd, "Off")
        this.shrink := 0
    }
}

; ---------------------------------------------------------------------
;  The app's adapter: the real windows
; ---------------------------------------------------------------------
class WinOps {
    __New(canBorder) {
        this.canBorder := canBorder                 ; DWMWA_BORDER_COLOR: Windows 11 only
        this.gui := Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0")   ; where the picture is shown
    }

    scale => A_ScreenDPI / 96

    IsTopmost(hwnd) {
        try return (WinGetExStyle(hwnd) & 0x8) != 0
        return false
    }

    SetTopmost(hwnd, on) => DllCall("SetWindowPos", "Ptr", hwnd, "Ptr", on ? -1 : -2, "Int", 0, "Int", 0
        , "Int", 0, "Int", 0, "UInt", 0x13)         ; TOPMOST / NOTOPMOST; NOSIZE|NOMOVE|NOACTIVATE

    Border(hwnd, rgb) {
        ref := rgb = -1 ? 0xFFFFFFFF : ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)   ; DWMWA_COLOR_DEFAULT, or COLORREF (BGR)
        try DllCall("dwmapi\DwmSetWindowAttribute", "Ptr", hwnd, "UInt", 34, "UInt*", ref, "UInt", 4, "Int")   ; DWMWA_BORDER_COLOR
    }

    Pos(hwnd) {
        try {
            WinGetPos(&x, &y, &w, &h, hwnd)
            return x "|" y "|" w "|" h
        }
        return ""
    }

    ; Its opacity, 0-255, or "Off" for how it was. Most windows aren't layered: Windows' plain
    ; transparency is put on, then taken off. Chromium's (Chrome, Edge, VS Code, Electron apps) are
    ; layered already, with an opacity of their own: that's changed, then put back exactly, never
    ; taken off (it would break them). Windows that draw their own per-pixel transparency can't be
    ; changed: false.
    Alpha(hwnd, a) {
        static saved := Map()
        if (a = "Off") {
            if !saved.Has(hwnd)
                return true
            s := saved[hwnd], saved.Delete(hwnd)
            if s.layered
                return DllCall("SetLayeredWindowAttributes", "Ptr", hwnd, "UInt", s.key, "UChar", s.alpha, "UInt", s.flags) != 0
            try WinSetTransparent("Off", hwnd)
            return true
        }
        if !saved.Has(hwnd) {
            try ex := WinGetExStyle(hwnd)
            catch
                return false
            if (ex & 0x80000) {                     ; WS_EX_LAYERED already
                if !this.LayeredAttributes(hwnd, &key, &alpha, &flags)
                    return false                    ; per-pixel: leave it alone
                saved[hwnd] := {layered: true, key: key, alpha: alpha, flags: flags}
            } else {
                saved[hwnd] := {layered: false}
            }
        }
        s := saved[hwnd]
        if s.layered
            return DllCall("SetLayeredWindowAttributes", "Ptr", hwnd, "UInt", s.key, "UChar", a, "UInt", s.flags | 0x2, "Int") != 0   ; LWA_ALPHA
        try {
            WinSetTransparent(a, hwnd)
            return true
        }
        return false
    }

    ; A layered window's own opacity settings; false when it draws per-pixel (UpdateLayeredWindow).
    LayeredAttributes(hwnd, &key, &alpha, &flags) {
        key := 0, alpha := 0, flags := 0
        return DllCall("GetLayeredWindowAttributes", "Ptr", hwnd, "UInt*", &key, "UChar*", &alpha, "UInt*", &flags, "Int") != 0
    }

    ; The window as you see it (PrintWindow), ready to be shrunk, with where (x, y) is on it.
    ; {kind: "picture", …}, or {kind: "fallback" / "skip", why}.
    Picture(hwnd, x, y) {
        cls := ""
        try {
            cls := WinGetClass(hwnd)
            if ((WinGetExStyle(hwnd) & 0x80000) && !this.LayeredAttributes(hwnd, &k, &a, &f))
                return {kind: "skip", why: "per-pixel layered window, left alone: " cls}
            WinGetPos(&wx, &wy, &ww, &wh, hwnd)
        } catch as err
            return {kind: "skip", why: "can't read the window: " err.Message}
        rect := Buffer(16, 0)                       ; without the invisible borders
        if DllCall("dwmapi\DwmGetWindowAttribute", "Ptr", hwnd, "UInt", 9, "Ptr", rect, "UInt", 16, "Int") != 0
            return {kind: "fallback", why: "no frame bounds: " cls}
        fl := NumGet(rect, 0, "Int"), ft := NumGet(rect, 4, "Int"), fw := NumGet(rect, 8, "Int") - fl, fh := NumGet(rect, 12, "Int") - ft
        if (fw < 8 || fh < 8)
            return {kind: "skip", why: ""}
        full := NewDib(ww, wh)
        if !DllCall("PrintWindow", "Ptr", hwnd, "Ptr", full, "UInt", 2) {   ; PW_RENDERFULLCONTENT
            FreeDib(full)
            return {kind: "fallback", why: "PrintWindow failed: " cls}
        }
        opaque := NewDib(fw, fh, &ob)
        Fill32(ob, fw * fh, 0xFF000000)
        return {kind: "picture", full: full, canvas: NewDib(fw, fh), opaque: opaque, w: fw, h: fh
            , ox: fl - wx, oy: ft - wy, fx: (x - fl) / fw, fy: (y - ft) / fh}
    }

    ShowPicture(pic, x, y, w, h, alpha, first) {
        DllCall("SetStretchBltMode", "Ptr", pic.canvas, "Int", 4)   ; HALFTONE: smooth
        DllCall("StretchBlt", "Ptr", pic.canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
            , "Ptr", pic.full, "Int", pic.ox, "Int", pic.oy, "Int", pic.w, "Int", pic.h, "UInt", 0x00CC0020)
        DllCall("BitBlt", "Ptr", pic.canvas, "Int", 0, "Int", 0, "Int", w, "Int", h, "Ptr", pic.opaque, "Int", 0, "Int", 0, "UInt", 0x00EE0086)   ; SRCPAINT: alpha 255
        LayeredPut(this.gui, x, y, w, h, pic.canvas, alpha)
        if first
            OnTop(this.gui)
    }

    HidePicture() => this.gui.Hide()
    FreePicture(pic) => (FreeDib(pic.full), FreeDib(pic.canvas), FreeDib(pic.opaque))

    ; Why a window didn't shrink, in %TEMP%\EzDesk-debug.log (kept short).
    Log(msg) {
        static last := ""
        if (msg = last)
            return
        last := msg, path := A_Temp "\EzDesk-debug.log"
        try {
            if (FileExist(path) && FileGetSize(path) > 20000)
                FileDelete(path)
            FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") "  " msg "`n", path)
        }
    }
}
