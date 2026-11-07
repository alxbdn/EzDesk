# Gives the renamed AutoHotkey runtime its own identity: replaces the exe's main icon and its
# version information (the name Windows shows in Settings > Personalization > Taskbar,
# Task Manager, file properties...). Uses the Win32 UpdateResource API, no external tool.
# Usage: .\rebrand-exe.ps1 -Exe EzDesk.exe -Icon app.ico -Version 1.1.1
param(
    [Parameter(Mandatory)] [string]$Exe,
    [Parameter(Mandatory)] [string]$Icon,
    [Parameter(Mandatory)] [string]$Version,
    [string]$AhkVersion = '2.0.31'
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class ExeRebrand
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern IntPtr BeginUpdateResource(string file, bool deleteExisting);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool UpdateResource(IntPtr h, IntPtr type, IntPtr name, ushort lang, byte[] data, uint size);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool EndUpdateResource(IntPtr h, bool discard);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern IntPtr LoadLibraryEx(string file, IntPtr h, uint flags);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr h);
    delegate bool EnumNameProc(IntPtr h, IntPtr type, IntPtr name, IntPtr param);
    delegate bool EnumLangProc(IntPtr h, IntPtr type, IntPtr name, ushort lang, IntPtr param);
    [DllImport("kernel32.dll")] static extern bool EnumResourceNames(IntPtr h, IntPtr type, EnumNameProc cb, IntPtr param);
    [DllImport("kernel32.dll")] static extern bool EnumResourceLanguages(IntPtr h, IntPtr type, IntPtr name, EnumLangProc cb, IntPtr param);

    const int RT_ICON = 3, RT_GROUP_ICON = 14, RT_VERSION = 16;
    const uint LOAD_AS_DATAFILE = 0x2;

    static bool IsInt(IntPtr p) { return ((ulong)p.ToInt64() >> 16) == 0; }

    // Resource names: integer IDs or strings. Kept as object (int or string) so they survive FreeLibrary.
    static object NameOf(IntPtr p) { return IsInt(p) ? (object)(int)p.ToInt64() : Marshal.PtrToStringUni(p); }

    static List<object> Names(IntPtr h, int type)
    {
        var list = new List<object>();
        EnumResourceNames(h, (IntPtr)type, (m, t, n, p) => { list.Add(NameOf(n)); return true; }, IntPtr.Zero);
        return list;
    }

    static ushort FirstLang(IntPtr h, int type, object name)
    {
        ushort found = 0; bool got = false;
        IntPtr np = name is int ? (IntPtr)(int)name : Marshal.StringToHGlobalUni((string)name);
        EnumResourceLanguages(h, (IntPtr)type, np, (m, t, n, l, p) => { if (!got) { found = l; got = true; } return true; }, IntPtr.Zero);
        if (!(name is int)) Marshal.FreeHGlobal(np);
        return got ? found : (ushort)0x0409;
    }

    static void Update(IntPtr h, int type, object name, ushort lang, byte[] data)
    {
        IntPtr np = name is int ? (IntPtr)(int)name : Marshal.StringToHGlobalUni((string)name);
        try
        {
            if (!UpdateResource(h, (IntPtr)type, np, lang, data, (uint)data.Length))
                throw new InvalidOperationException("UpdateResource failed: " + Marshal.GetLastWin32Error());
        }
        finally { if (!(name is int)) Marshal.FreeHGlobal(np); }
    }

    // ---- VS_VERSIONINFO: nested blocks of { wLength, wValueLength, wType, szKey, pad, Value, pad, Children }
    static void Pad(MemoryStream ms) { while (ms.Length % 4 != 0) ms.WriteByte(0); }

    static byte[] Block(string key, byte[] value, ushort valueLength, ushort type, List<byte[]> children)
    {
        var ms = new MemoryStream();
        var w = new BinaryWriter(ms);
        w.Write((ushort)0); w.Write(valueLength); w.Write(type);
        w.Write(Encoding.Unicode.GetBytes(key + "\0"));
        Pad(ms);
        if (value != null) { w.Write(value); }
        foreach (var c in children) { Pad(ms); w.Write(c); }
        var bytes = ms.ToArray();
        BitConverter.GetBytes((ushort)bytes.Length).CopyTo(bytes, 0);
        return bytes;
    }

    static byte[] Str(string key, string value)
    {
        return Block(key, Encoding.Unicode.GetBytes(value + "\0"), (ushort)(value.Length + 1), 1, new List<byte[]>());
    }

    static byte[] VersionInfo(int[] v, Dictionary<string, string> strings)
    {
        var fixedInfo = new MemoryStream();
        var f = new BinaryWriter(fixedInfo);
        uint ms = (uint)((v[0] << 16) | v[1]), ls = (uint)((v[2] << 16) | v[3]);
        f.Write(0xFEEF04BDu); f.Write(0x00010000u);
        f.Write(ms); f.Write(ls); f.Write(ms); f.Write(ls);          // file / product version
        f.Write(0x3Fu); f.Write(0u); f.Write(0x40004u); f.Write(1u); f.Write(0u); f.Write(0u); f.Write(0u);   // flags, VOS_NT_WINDOWS32, VFT_APP
        var strs = new List<byte[]>();
        foreach (var kv in strings) strs.Add(Str(kv.Key, kv.Value));
        var table = Block("040904B0", null, 0, 1, strs);
        var sfi = Block("StringFileInfo", null, 0, 1, new List<byte[]> { table });
        var translation = new byte[] { 0x09, 0x04, 0xB0, 0x04 };       // en-US, Unicode
        var vfi = Block("VarFileInfo", null, 0, 1, new List<byte[]> { Block("Translation", translation, 4, 0, new List<byte[]>()) });
        return Block("VS_VERSION_INFO", fixedInfo.ToArray(), 52, 0, new List<byte[]> { sfi, vfi });
    }

    public static string Apply(string exe, string ico, int[] version, string[] keys, string[] values)
    {
        // 1) read the current layout: main icon group, icon IDs in use, languages
        IntPtr lib = LoadLibraryEx(exe, IntPtr.Zero, LOAD_AS_DATAFILE);
        if (lib == IntPtr.Zero) throw new InvalidOperationException("cannot open " + exe);
        object group; ushort groupLang, verLang; object verName; int nextId = 1;
        try
        {
            var groups = Names(lib, RT_GROUP_ICON);
            if (groups.Count == 0) throw new InvalidOperationException("no icon group in " + exe);
            group = groups[0];                                       // the one Explorer and Windows show
            groupLang = FirstLang(lib, RT_GROUP_ICON, group);
            foreach (var n in Names(lib, RT_ICON)) if (n is int && (int)n >= nextId) nextId = (int)n + 1;
            var vers = Names(lib, RT_VERSION);
            verName = vers.Count > 0 ? vers[0] : (object)1;
            verLang = vers.Count > 0 ? FirstLang(lib, RT_VERSION, verName) : (ushort)0x0409;
        }
        finally { FreeLibrary(lib); }

        // 2) the new icon: one RT_ICON per image in the .ico, plus a group directory pointing at them
        var icoBytes = File.ReadAllBytes(ico);
        int count = BitConverter.ToUInt16(icoBytes, 4);
        var grp = new MemoryStream(); var gw = new BinaryWriter(grp);
        gw.Write((ushort)0); gw.Write((ushort)1); gw.Write((ushort)count);
        var images = new List<byte[]>();
        for (int i = 0; i < count; i++)
        {
            int e = 6 + i * 16;
            int len = BitConverter.ToInt32(icoBytes, e + 8), off = BitConverter.ToInt32(icoBytes, e + 12);
            var img = new byte[len]; Array.Copy(icoBytes, off, img, 0, len); images.Add(img);
            gw.Write(icoBytes, e, 8);                                // width, height, colors, reserved, planes, bitcount
            gw.Write(len); gw.Write((ushort)(nextId + i));
        }

        // 3) write everything in one update
        IntPtr h = BeginUpdateResource(exe, false);
        if (h == IntPtr.Zero) throw new InvalidOperationException("BeginUpdateResource failed: " + Marshal.GetLastWin32Error());
        bool ok = false;
        try
        {
            for (int i = 0; i < count; i++) Update(h, RT_ICON, nextId + i, groupLang, images[i]);
            Update(h, RT_GROUP_ICON, group, groupLang, grp.ToArray());
            var strings = new Dictionary<string, string>();
            for (int i = 0; i < keys.Length; i++) strings[keys[i]] = values[i];
            Update(h, RT_VERSION, verName, verLang, VersionInfo(version, strings));
            ok = true;
        }
        finally
        {
            if (!EndUpdateResource(h, !ok) && ok) throw new InvalidOperationException("EndUpdateResource failed: " + Marshal.GetLastWin32Error());
        }
        return "icon group " + group + " (" + count + " images, IDs from " + nextId + "), version resource " + verName;
    }
}
'@

# numeric part for the binary version ("1.1.1" -> 1.1.1.0); CI dev builds ("dev-abc1234") get 0.0.0.0
$parts = @(); if ($Version -match '^\d+(\.\d+){0,3}') { $parts = @($Matches[0].Split('.') | ForEach-Object { [int]$_ }) }
while ($parts.Count -lt 4) { $parts += 0 }
$strings = [ordered]@{
    CompanyName      = 'alxbdn'
    FileDescription  = 'EzDesk'
    FileVersion      = ($parts -join '.')
    InternalName     = 'EzDesk'
    LegalCopyright   = 'Copyright (c) 2026 Alexandre B. (MIT)'
    OriginalFilename = 'EzDesk.exe'
    ProductName      = 'EzDesk'
    ProductVersion   = $Version
    Comments         = "AutoHotkey v$AhkVersion runtime (GPLv2, https://www.autohotkey.com), renamed and rebranded to run EzDesk.ahk"
}
$r = [ExeRebrand]::Apply((Resolve-Path $Exe).Path, (Resolve-Path $Icon).Path, [int[]]$parts, [string[]]$strings.Keys, [string[]]$strings.Values)
"rebranded $(Split-Path $Exe -Leaf): $r"
