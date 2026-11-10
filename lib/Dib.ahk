; Memory bitmaps and layered windows: the GDI plumbing the glows and the shrinking window share.

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

; Frees a memory DC made by NewDib, and its bitmap.
FreeDib(hdc) {
    hbm := DllCall("GetCurrentObject", "Ptr", hdc, "UInt", 7, "Ptr")   ; OBJ_BITMAP
    DllCall("DeleteDC", "Ptr", hdc), DllCall("DeleteObject", "Ptr", hbm)
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

; Puts a w × h premultiplied picture on a layered window at (x, y), × alpha (0-255).
LayeredPut(g, x, y, w, h, src, alpha) {
    DllCall("UpdateLayeredWindow", "Ptr", g.Hwnd, "Ptr", 0, "Int64*", (y << 32) | (x & 0xFFFFFFFF)
        , "Int64*", (h << 32) | w, "Ptr", src, "Int64*", 0, "UInt", 0
        , "UInt*", (1 << 24) | (alpha << 16), "UInt", 2)   ; BLENDFUNCTION: per-pixel alpha × alpha; ULW_ALPHA
}

; Shows a window on top of everything, without activating it.
OnTop(g) => DllCall("SetWindowPos", "Ptr", g.Hwnd, "Ptr", -1, "Int", 0, "Int", 0, "Int", 0, "Int", 0, "UInt", 0x53)   ; TOPMOST; NOSIZE|NOMOVE|NOACTIVATE|SHOWWINDOW
