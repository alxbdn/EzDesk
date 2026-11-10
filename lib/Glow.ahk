; =====================================================================
;  Glow: the flowing light round the dock and on the side bumps
; =====================================================================
; Windows blue (#0063B1) brightening to white and back, alive. Two layers of those colours slide at
; different speeds and in opposite directions; where they cross they mix, so the light keeps
; shifting instead of just sliding.
;
; A GlowShape is one glowing shape on screen: a layered window (per-pixel alpha) that clicks go
; through, never activated, not in the taskbar, pinned to every desktop so it stays through a
; switch. Its outline comes from a mask, made once per size and kept:
;   GlowRoundRect(...)   the haze round a rounded rectangle (the dock)
;   GlowBell(...)        a bell standing out of a screen edge (the bumps)
; A frame is a few GDI calls, whatever the size: colours, mixed, then shaped by the mask.
; Careful with names: AutoHotkey ignores case, so "L" would be the same variable as "l".
; Needs Dib.ahk.

global GLOW_COLORS := [0x0063B1, 0x2A82CC, 0x8CC0EC, 0xFFFFFF, 0x8CC0EC, 0x2A82CC]   ; blue to white and back
global GLOW_FLOW_MS := 2600, GLOW_FLOW2_MS := 3700  ; time for each layer to slide one period

class GlowShape {
    static pin := 0                                 ; set by the app: pins a window to every desktop, true if it worked

    ; axis: "x", the colours flow sideways (the dock), "y", along its height (the bumps). slow: how many
    ; times slower than GLOW_FLOW_MS. raiseEvery: put back on top every so many ms (0: when shown only).
    __New(axis, slow := 1, raiseEvery := 0) {
        this.axis := axis, this.slow := slow, this.raiseEvery := raiseEvery
        this.win := Gui("-Caption +AlwaysOnTop +ToolWindow +E0x080800A0")   ; LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW
        this.visible := false, this.pinned := false, this.raisedAt := 0
    }

    Hwnd => this.win.Hwnd

    ; One frame: the shape's mask at (x, y), × alpha (0-255). extras, all optional:
    ;   white   0-1, light filling it (over the colours, under the mask)
    ;   over    a memory DC the mask's size, laid on top, premultiplied (the dock's names)
    ;   stamp   {dc, size, x, y, alpha}: a small square picture laid on top (the bump's +)
    Draw(mask, x, y, alpha, extras := 0) {
        w := mask.w, h := mask.h, t := A_TickCount, horiz := this.axis = "x"
        pa := horiz ? w : h, pb := GlowPeriod2(pa)  ; the whole palette along the shape
        sa := GlowStrip(w, h, horiz, pa, 0), sb := GlowStrip(w, h, horiz, pb, GLOW_COLORS.Length // 2)
        ma := Mod(t * pa // (this.slow * GLOW_FLOW_MS), pa), mb := Mod(t * pb // (this.slow * GLOW_FLOW2_MS), pb)
        oa := horiz ? pa - ma : ma, ob := horiz ? mb : pb - mb   ; the two layers flow in opposite directions
        canvas := GlowCanvas(w, h)
        DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
            , "Ptr", sa.dc, "Int", horiz ? oa : 0, "Int", horiz ? 0 : oa, "UInt", 0x00CC0020)
        DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
            , "Ptr", sb.dc, "Int", horiz ? ob : 0, "Int", horiz ? 0 : ob, "Int", w, "Int", h, "UInt", 150 << 16)   ; mixed in at 150/255
        if (extras && extras.HasOwnProp("white") && extras.white > 0)
            DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
                , "Ptr", GlowWhite(w, h), "Int", 0, "Int", 0, "Int", w, "Int", h, "UInt", Round(220 * Min(1, extras.white)) << 16)
        ; Shaped without touching a pixel from here: AlphaBlend inv over the opaque colours (each pixel
        ; becomes m × colour, alpha 255), then AND keep, which sets the alpha byte to m.
        DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
            , "Ptr", mask.inv, "Int", 0, "Int", 0, "Int", w, "Int", h, "UInt", (1 << 24) | (255 << 16))
        DllCall("BitBlt", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h, "Ptr", mask.keep, "Int", 0, "Int", 0, "UInt", 0x008800C6)   ; SRCAND
        if (extras && extras.HasOwnProp("over") && extras.over)
            DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", 0, "Int", 0, "Int", w, "Int", h
                , "Ptr", extras.over, "Int", 0, "Int", 0, "Int", w, "Int", h, "UInt", (1 << 24) | (255 << 16))
        if (extras && extras.HasOwnProp("stamp") && extras.stamp) {
            s := extras.stamp
            DllCall("msimg32\AlphaBlend", "Ptr", canvas, "Int", s.x, "Int", s.y, "Int", s.size, "Int", s.size
                , "Ptr", s.dc, "Int", 0, "Int", 0, "Int", s.size, "Int", s.size, "UInt", (1 << 24) | (s.alpha << 16))
        }
        LayeredPut(this.win, x, y, w, h, canvas, Max(0, Min(255, alpha)))
        if (!this.visible || (this.raiseEvery && t - this.raisedAt > this.raiseEvery))
            this.Raise()                            ; back on top now and then: Snap's preview comes up over everything
        if (!this.pinned && GlowShape.pin)
            this.pinned := GlowShape.pin.Call(this.Hwnd)
    }

    Raise() {
        OnTop(this.win)
        this.visible := true, this.raisedAt := A_TickCount
    }

    Hide() {
        this.win.Hide()
        this.visible := false
    }

    ; Builds what a frame of this mask needs ahead of time, so the first one is quick.
    Warm(mask) {
        horiz := this.axis = "x", pa := horiz ? mask.w : mask.h
        GlowStrip(mask.w, mask.h, horiz, pa, 0), GlowStrip(mask.w, mask.h, horiz, GlowPeriod2(pa), GLOW_COLORS.Length // 2)
    }
}

; ---------------------------------------------------------------------
;  The light itself: pure numbers
; ---------------------------------------------------------------------
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

; Distance from a rounded rectangle's edge, outward (negative inside), for a point (qx, qy) away
; from where its straight sides end. Corners are superellipses |x|^n + |y|^n = rad^n (n = 2: circle
; arcs, like Windows' own). In a corner, the superellipse's own measure is divided by its slope so
; it's a true distance (raw, it's ~20 % short at 45° and the haze bulges out at the corners).
RoundRectDistance(qx, qy, rad, n := 2) {
    if (qx > 0 && qy > 0) {
        lp := (qx ** n + qy ** n) ** (1 / n)
        gx := (qx / lp) ** (n - 1), gy := (qy / lp) ** (n - 1)
        return (lp - rad) / Sqrt(gx * gx + gy * gy)
    }
    return Max(qx, qy) - rad                        ; along a side: straight distance from the edge
}

; A bell standing out of a screen edge, depth × (1 - u²)² for u = -1…1 along the edge. At u:
; x = how far its outline stands out, norm = how much a horizontal distance overstates the true one
; there (the slope), fade = its ends melting into the edge. 0 past its ends.
BellEdge(u, depth, half) {
    u := Abs(u)
    if (u >= 1)
        return 0
    e := 1 - u * u, slope := 4 * depth * u * e / half
    fade := Min(1, (1 - u) * 4), fade := fade * fade * (3 - 2 * fade)
    return {x: depth * e * e, norm: Sqrt(1 + slope * slope), fade: fade}
}

; ---------------------------------------------------------------------
;  Masks: made once per size, kept
; ---------------------------------------------------------------------
; A mask is {w, h, inv, keep}: inv is black with alpha 1 - m, keep white with alpha m, m being the
; glow's strength at each pixel. m = 0 everywhere to start.
GlowMaskNew(w, h, &ib, &kb) {
    inv := NewDib(w, h, &ib), keep := NewDib(w, h, &kb)
    Fill32(ib, w * h, 0xFF000000), Fill32(kb, w * h, 0x00FFFFFF)
    return {w: w, h: h, inv: inv, keep: keep}
}

; The haze round a rounded rectangle bw × bh (corner radius rad, corner shape n), mg wide all round:
; bright on the edge, a few pixels inside that cover a window region's hard edge.
GlowRoundRect(bw, bh, rad, mg, n := 2) {
    static cache := Map()
    fw := bw + 2 * mg, fh := bh + 2 * mg, key := fw "|" fh "|" rad "|" mg "|" n
    if cache.Has(key)
        return cache[key]
    mask := GlowMaskNew(fw, fh, &ib, &kb)
    inner := Max(2, Round(5 * A_ScreenDPI / 96))
    hx := bw / 2 - rad, hy := bh / 2 - rad, cx := fw / 2, cy := fh / 2
    MaskAt(px, py) {                                ; m (0-255) at the centre of pixel (px, py)
        d := RoundRectDistance(Abs(px + 0.5 - cx) - hx, Abs(py + 0.5 - cy) - hy, rad, n)
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
            for sideX in [0, fw - cs]
                for base in [ib, kb]
                    DllCall("RtlMoveMemory", "Ptr", base + (row * fw + sideX) * 4, "Ptr", base + (cs * fw + sideX) * 4, "UPtr", cs * 4)
        }
    }
    cache[key] := mask
    return mask
}

; A bell (BellEdge) depth deep and 2 × half long, filled, with a haze mg wide round it, centred in a
; w × h window. side -1: on the left edge of the screen, standing out to the right; +1: mirrored.
GlowBell(w, h, depth, half, mg, side) {
    static cache := Map()
    key := w "|" h "|" Round(depth, 2) "|" Round(half, 2) "|" mg "|" side
    if cache.Has(key)
        return cache[key]
    mask := GlowMaskNew(w, h, &ib, &kb)
    Loop h // 2 {                                   ; the top half, mirrored to the bottom
        j := A_Index - 1, b := BellEdge((h / 2 - j - 0.5) / half, depth, half)
        if !b
            continue
        Loop w {
            c := A_Index - 1, d := (c + 0.5 - b.x) / b.norm   ; distance from the outline, outward
            m := Round(255 * b.fade * (d < 0 ? 1 : GlowFall(d, mg)))
            if !m {
                if (d > 0)
                    break                           ; only darker further out
                continue
            }
            col := side < 0 ? c : w - 1 - c
            for row in [j, h - 1 - j] {
                o := (row * w + col) * 4
                NumPut("UInt", (255 - m) << 24, ib, o), NumPut("UInt", (m << 24) | 0xFFFFFF, kb, o)
            }
        }
    }
    cache[key] := mask
    return mask
}

; ---------------------------------------------------------------------
;  What a frame is made from (kept, per size)
; ---------------------------------------------------------------------
; Opaque colour bands, the colours cycling once per period: along x (w + period wide, a frame takes a
; w-wide slice at a moving offset) or along y (h + period tall).
GlowStrip(w, h, horiz, period, shift) {
    static cache := Map()
    key := w "|" h "|" horiz "|" period "|" shift
    if cache.Has(key)
        return cache[key]
    if horiz {
        total := w + period, dc := NewDib(total, h, &bits)
        Loop total
            NumPut("UInt", 0xFF000000 | GlowColorAt(A_Index - 1, period, shift), bits, (A_Index - 1) * 4)
        Loop h - 1
            DllCall("RtlMoveMemory", "Ptr", bits + A_Index * total * 4, "Ptr", bits, "UPtr", total * 4)
    } else {
        total := h + period, dc := NewDib(w, total, &bits)
        Loop total
            Fill32(bits + (A_Index - 1) * w * 4, w, 0xFF000000 | GlowColorAt(A_Index - 1, period, shift))
    }
    cache[key] := {dc: dc}
    return cache[key]
}

; Where each frame is put together before it goes on screen.
GlowCanvas(w, h) {
    static cache := Map()
    key := w "|" h
    if !cache.Has(key)
        cache[key] := NewDib(w, h)
    return cache[key]
}

GlowWhite(w, h) {                                   ; for the light that fills a shape
    static cache := Map()
    key := w "|" h
    if !cache.Has(key) {
        dc := NewDib(w, h, &bits)
        Fill32(bits, w * h, 0xFFFFFFFF)
        cache[key] := dc
    }
    return cache[key]
}
