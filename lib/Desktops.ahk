; =====================================================================
;  Desktops: everything EzDesk asks of Windows' virtual desktops
; =====================================================================
; Callers never call the DLL or send Windows' shortcuts themselves: they ask this module, which
; picks the way that works on this Windows. Desktops count from 0 here; the user sees them from 1.
;
; The raw work is done by an adapter:
;   VdaAdapter     the app: the VirtualDesktopAccessor DLL (its Windows 11 or Windows 10 build) and
;                  Windows' own Ctrl+Win shortcuts, which slide where the DLL's switch is instant
;   FakeDesktops   the tests (tests\FakeDesktops.ahk): desktops in memory
; Both answer: broken, Has(op), Count(), Current(), Name(i), WindowDesktop(hwnd), IsOnCurrent(hwnd),
; Move(hwnd, i), Pin(hwnd), Unpin(hwnd), Create(), Remove(i, fallback), SlideLeft(), SlideRight(),
; SlideNew(), Wait(ms), Listen(hwnd, msg). A failed call returns -1.

class Desktops {
    __New(adapter) {
        this.a := adapter
    }

    ; ---- What this Windows allows ----
    CanCreate => this.a.Has("Create")               ; make a desktop without going there (Windows 11 DLL)
    CanRemove => this.a.Has("Remove")               ; close one (Windows 11 DLL)
    CanCarry => this.a.Has("Pin") && this.a.Has("Unpin") && this.a.Has("WindowDesktop")   ; take a window along a switch
    Broken => this.a.broken                         ; the DLL crashed on this Windows build: everything is a no-op

    ; ---- Reading ----
    Count() => this.a.Count()                       ; -1 when Windows doesn't answer
    Current() => this.a.Current()
    WindowDesktop(hwnd) => this.a.WindowDesktop(hwnd)
    IsOnCurrent(hwnd) => this.a.IsOnCurrent(hwnd) = 1

    Name(i) {                                       ; "" when it has no name of its own
        if !this.a.Has("Name")
            return ""
        name := this.a.Name(i)
        return name = -1 ? "" : name
    }

    Label(i) {                                      ; what the dock writes under it
        name := this.Name(i)
        return name != "" ? name : "Desktop " (i + 1)
    }

    Listen(hwnd, msg) => this.a.Listen(hwnd, msg)   ; Windows posts msg to hwnd on every switch

    ; ---- Windows on desktops ----
    Pin(hwnd) => this.a.Has("Pin") && this.a.Pin(hwnd) != -1   ; on every desktop; true if it worked
    Unpin(hwnd) => this.a.Has("Unpin") && this.a.Unpin(hwnd)

    ; A window let go after it was held (target: desktop index, -2 for a new one, -1 for none).
    ; carried: it was pinned for the drag. It goes to target, or stays where you are; returns true
    ; when it left the current desktop, so the next window here can take the focus.
    ; Always moved first, then unpinned, so it never vanishes in between.
    Drop(hwnd, target, carried) {
        if (target = -2)
            target := this.Create()                 ; -1 if it can't be made without going there
        cur := this.Current()
        if (target >= 0 && target != cur) {
            this.a.Move(hwnd, target)
            if carried
                this.a.Unpin(hwnd)
            return true
        }
        if carried {                                ; back on the desktop you're on
            if (cur >= 0)
                this.a.Move(hwnd, cur)
            this.a.Unpin(hwnd)
        }
        return false
    }

    ; ---- Making and closing desktops ----
    ; One more at the end, without going there; returns its index, or -1 (can't, or it didn't appear).
    ; Success is read from the count, not from the call's return value.
    Create() {
        if !this.CanCreate
            return -1
        count := this.Count()
        this.a.Create()
        Loop 30 {                                   ; up to 1.5 s
            if (this.Count() > count)
                return count
            this.a.Wait(50)
        }
        return -1
    }

    ; Closes desktop i; its windows go to the one before it (the next one for desktop 1).
    Remove(i) {
        if (!this.CanRemove || i < 0 || this.Count() < 2)
            return false
        this.a.Remove(i, i > 0 ? i - 1 : 1)
        return true
    }

    ; ---- Switching, always with Windows' own slide ----
    ; One desktop left (side -1) or right (+1). orNew: past the last one, make a new one and go there.
    ; Returns true if it switched.
    Slide(side, orNew := false) {
        cur := this.Current(), count := this.Count()
        if (cur < 0 || count <= 0)
            return false
        i := cur + side
        if (i >= 0 && i < count) {
            if (side < 0)
                this.a.SlideLeft()
            else
                this.a.SlideRight()
            return true
        }
        if (orNew && side > 0) {
            this.a.SlideNew()
            return true
        }
        return false
    }

    ; Goes to desktop target (-2: a new one), one slide at a time.
    GoTo(target) {
        if (target = -2) {
            this.a.SlideNew()                       ; Windows makes it at the end and slides there
            return
        }
        cur := this.Current()
        if (target < 0 || cur < 0 || target = cur || target >= this.Count())
            return
        Loop Abs(target - cur) {
            if (target > cur)
                this.a.SlideRight()
            else
                this.a.SlideLeft()
            this.a.Wait(180)
        }
    }
}

; ---------------------------------------------------------------------
;  The app's adapter: VirtualDesktopAccessor and Windows' shortcuts
; ---------------------------------------------------------------------
; A DLL that doesn't match this Windows build crashes (access violation 0xC0000005) instead of
; returning an error. Without the try, AutoHotkey would show an error dialog every second and the
; app would look frozen. Three crashes and it stops calling the DLL at all.
class VdaAdapter {
    __New(dll, build) {
        this.build := build, this.errors := 0, this.broken := false
        this.h := DllCall("LoadLibrary", "Str", dll, "Ptr")
        if !this.h {
            MsgBox("Could not load " dll, "EzDesk", "Iconx")
            ExitApp(1)
        }
        this.p := Map()
        for name in ["GetDesktopCount", "GetCurrentDesktopNumber", "MoveWindowToDesktopNumber"
            , "IsWindowOnCurrentVirtualDesktop", "RegisterPostMessageHook"]
            this.Proc(name, true)
        for name in ["GetDesktopName", "CreateDesktop", "RemoveDesktop", "PinWindow", "UnPinWindow"
            , "GetWindowDesktopNumber"]             ; not in every build: Windows 10's lacks the first three
            this.Proc(name, false)
    }

    Proc(name, required) {
        p := DllCall("GetProcAddress", "Ptr", this.h, "AStr", name, "Ptr")
        if (!p && required) {
            MsgBox("VirtualDesktopAccessor.dll has no " name "().`nIt may not match this Windows build (" this.build ").", "EzDesk", "Iconx")
            ExitApp(1)
        }
        this.p[name] := p
    }

    Call(name, args*) {
        p := this.p[name]
        if (this.broken || !p)
            return -1
        try return DllCall(p, args*)
        catch {
            this.errors += 1
            if (this.errors >= 3 && !this.broken) {
                this.broken := true
                A_IconTip := "EzDesk: not compatible with this Windows (build " this.build ")"
                TrayTip("The virtual-desktop library doesn't work on this Windows version (build " this.build ").`nMoving windows between desktops is off. See Help in this icon's menu.", "EzDesk", "Iconx Mute")
            }
            return -1
        }
    }

    static OPS := Map("Name", "GetDesktopName", "Create", "CreateDesktop", "Remove", "RemoveDesktop"
        , "Pin", "PinWindow", "Unpin", "UnPinWindow", "WindowDesktop", "GetWindowDesktopNumber")
    Has(op) => VdaAdapter.OPS.Has(op) && this.p[VdaAdapter.OPS[op]] != 0

    Count() => this.Call("GetDesktopCount", "Int")
    Current() => this.Call("GetCurrentDesktopNumber", "Int")
    WindowDesktop(hwnd) => this.Call("GetWindowDesktopNumber", "Ptr", hwnd, "Int")
    IsOnCurrent(hwnd) => this.Call("IsWindowOnCurrentVirtualDesktop", "Ptr", hwnd, "Int")
    Move(hwnd, i) => this.Call("MoveWindowToDesktopNumber", "Ptr", hwnd, "Int", i, "Int")
    Pin(hwnd) => this.Call("PinWindow", "Ptr", hwnd, "Int")
    Unpin(hwnd) => this.Call("UnPinWindow", "Ptr", hwnd, "Int")
    Create() => this.Call("CreateDesktop", "Int")
    Remove(i, fallback) => this.Call("RemoveDesktop", "Int", i, "Int", fallback, "Int")
    Listen(hwnd, msg) => this.Call("RegisterPostMessageHook", "Ptr", hwnd, "Int", msg, "Int")

    Name(i) {
        buf := Buffer(1024, 0)
        if (this.Call("GetDesktopName", "Int", i, "Ptr", buf, "UPtr", buf.Size, "Int") = -1 && this.broken)
            return -1
        return StrGet(buf, "UTF-8")                 ; "" when the desktop has no custom name
    }

    SlideLeft() => Send("^#{Left}")
    SlideRight() => Send("^#{Right}")
    SlideNew() => Send("^#d")                       ; makes one at the end and slides there, on Windows 10 too
    Wait(ms) => Sleep(ms)
}
