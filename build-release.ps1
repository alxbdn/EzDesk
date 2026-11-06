# Builds the ready-to-run release zip: dist\DesktopSwitcher.zip
# Downloads AutoHotkey and VirtualDesktopAccessor (pinned versions, SHA256-checked) unless local copies are given.
# Usage:  powershell -ExecutionPolicy Bypass -File .\build-release.ps1 -Version 1.0.0 [-AhkExe <path> -AhkLicense <path>] [-Dll <path>]
param(
    [Parameter(Mandatory)] [string]$Version,
    [string]$AhkExe, [string]$AhkLicense, [string]$Dll, [string]$Dll10,
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'
if (-not $OutDir) { $OutDir = Join-Path $PSScriptRoot 'dist' }   # PS 5.1: $PSScriptRoot is empty inside param defaults
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$AhkZipUrl    = 'https://github.com/AutoHotkey/AutoHotkey/releases/download/v2.0.31/AutoHotkey_2.0.31.zip'
$AhkZipSha256 = 'CA1A595ACA4528755D8D4CD7FED23D497A37BD9A0B6892072612D68DEE1BD00A'
$DllUrl       = 'https://github.com/Ciantic/VirtualDesktopAccessor/releases/download/2024-12-16-windows11/VirtualDesktopAccessor.dll'
$DllSha256    = '8740C572A1C000E3B87FFEB1E4C397EAE9AF3BD4A2ABDC3BCFFACAB4493F8FF5'
$Dll10Url     = 'https://github.com/Ciantic/VirtualDesktopAccessor/releases/download/2019-windows10/VirtualDesktopAccessor.dll'
$Dll10Sha256  = '145431BC2090281838DE31824BC829DD35688B13488F4850F4BC5F25F5E599DA'

function Get-Verified($url, $sha, $dest) {
    Invoke-WebRequest -UseBasicParsing $url -OutFile $dest
    $h = (Get-FileHash $dest).Hash
    if ($h -ne $sha) { throw "SHA256 mismatch for $url`n  expected $sha`n  got      $h" }
}

$root  = $PSScriptRoot
$stage = Join-Path $OutDir 'DesktopSwitcher'
$tmp   = Join-Path ([IO.Path]::GetTempPath()) ("dsw-build-" + [guid]::NewGuid().ToString('N'))
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Force "$stage\icons", "$stage\admin", $tmp | Out-Null

try {
    # --- AutoHotkey runtime, renamed so a double-click runs DesktopSwitcher.ahk ------------------
    if (-not $AhkExe) {
        Get-Verified $AhkZipUrl $AhkZipSha256 "$tmp\ahk.zip"
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::ExtractToDirectory("$tmp\ahk.zip", "$tmp\ahk")
        $AhkExe = "$tmp\ahk\AutoHotkey64.exe"
        $AhkLicense = Get-ChildItem "$tmp\ahk" -Filter 'license*.txt' | Select-Object -First 1 -ExpandProperty FullName
    }
    Copy-Item $AhkExe "$stage\DesktopSwitcher.exe"
    # Own icon and name, so Windows lists "Desktop Switcher" (tray settings, Task Manager), not "AutoHotkey 64-bit"
    New-Item -ItemType Directory -Force "$tmp\appicon" | Out-Null
    & "$root\make-icons.ps1" -OutDir "$tmp\appicon" -Outline 1 -Labels 1 -Sizes 16,20,24,32,40,48,64,128,256 | Out-Null
    & "$root\rebrand-exe.ps1" -Exe "$stage\DesktopSwitcher.exe" -Icon "$tmp\appicon\1.ico" -Version $Version
    if (-not $AhkLicense) { throw 'AutoHotkey license file not found' }
    Copy-Item $AhkLicense "$stage\LICENSE-AutoHotkey.txt"

    # --- VirtualDesktopAccessor --------------------------------------------------------------------
    if ($Dll) {
        if ((Get-FileHash $Dll).Hash -ne $DllSha256) { throw "Local DLL $Dll does not match the pinned SHA256" }
        Copy-Item $Dll "$stage\VirtualDesktopAccessor.dll"
    } else { Get-Verified $DllUrl $DllSha256 "$stage\VirtualDesktopAccessor.dll" }
    if ($Dll10) {
        if ((Get-FileHash $Dll10).Hash -ne $Dll10Sha256) { throw "Local DLL $Dll10 does not match the pinned SHA256" }
        Copy-Item $Dll10 "$stage\VirtualDesktopAccessor-win10.dll"
    } else { Get-Verified $Dll10Url $Dll10Sha256 "$stage\VirtualDesktopAccessor-win10.dll" }   # Windows 10 build

    # --- App files ---------------------------------------------------------------------------------
    Copy-Item "$root\DesktopSwitcher.ahk" $stage
    Copy-Item "$root\admin\*" "$stage\admin"
    Copy-Item "$root\LICENSE" "$stage\LICENSE.txt"
    Copy-Item "$root\THIRD-PARTY-NOTICES.md" "$stage\THIRD-PARTY-NOTICES.txt"
    & "$root\make-icons.ps1" -OutDir "$stage\icons" -Outline 1 | Out-Null

    @"
Desktop Switcher $Version
https://github.com/alxbdn/win11-desktop-switcher

START
  1. Double-click DesktopSwitcher.exe. A numbered icon appears next to the clock.
     If Windows says "Windows protected your PC": click "More info", then "Run anyway".
  2. Right-click the icon > "Start with Windows" so it starts by itself.

SHORTCUTS
  Alt + 1...9           go to desktop N (creates the next desktop if N does not exist yet)
  Alt + Shift + 1...9   move the active window to desktop N (creates it the same way if needed)
  Scroll on taskbar     previous / next desktop
  Click the icon        Task View

STOP / REMOVE
  Right-click the icon > untick "Start with Windows" > Exit, then delete this folder.

ADMIN MODE (optional, for power users)
  Makes the shortcuts work even over admin windows (Task Manager, installers...).
  Double-click admin\Install admin mode.cmd. Remove it with admin\Uninstall admin mode.cmd.
"@ | Set-Content "$stage\README.txt" -Encoding UTF8

    # --- Zip (the folder itself is the top-level entry, so extracting gives one tidy folder) ------
    # Entries are written by hand with "/" separators: Windows PowerShell's ZipFile uses "\", which other tools mangle.
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = Join-Path $OutDir 'DesktopSwitcher.zip'
    $archive = [IO.Compression.ZipFile]::Open($zip, 'Create')
    try {
        foreach ($f in Get-ChildItem $stage -Recurse -File) {
            $name = 'DesktopSwitcher/' + $f.FullName.Substring($stage.Length + 1).Replace('\', '/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $f.FullName, $name, 'Optimal') | Out-Null
        }
    } finally { $archive.Dispose() }
    $sha = (Get-FileHash $zip).Hash
    Set-Content (Join-Path $OutDir 'DesktopSwitcher.zip.sha256') -Encoding ASCII -Value "$sha  DesktopSwitcher.zip"
    Write-Host "Built $zip  ($([math]::Round((Get-Item $zip).Length / 1MB, 2)) MB)  SHA256 $sha"
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
