# Builds the ready-to-run release zip: dist\EzDesk.zip
# Builds EzDesk.exe with cargo (or takes a prebuilt one), and downloads VirtualDesktopAccessor (pinned
# versions, SHA256-checked) unless local copies are given.
# Usage:  powershell -ExecutionPolicy Bypass -File .\build-release.ps1 -Version 1.4.0 [-Exe <path>] [-Dll <path>] [-Dll10 <path>]
param(
    [Parameter(Mandatory)] [string]$Version,
    [string]$Exe, [string]$Dll, [string]$Dll10,
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'
if (-not $OutDir) { $OutDir = Join-Path $PSScriptRoot 'dist' }   # PS 5.1: $PSScriptRoot is empty inside param defaults
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

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
$stage = Join-Path $OutDir 'EzDesk'
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Force "$stage\icons", "$stage\admin" | Out-Null

# --- EzDesk.exe ----------------------------------------------------------------------------------
if (-not $Exe) {
    $env:EZDESK_VERSION = $Version                                   # shown in the exe's properties
    Push-Location $root
    try { cargo build --release; if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" } } finally { Pop-Location }
    $Exe = Join-Path $root 'target\release\EzDesk.exe'
}
Copy-Item $Exe "$stage\EzDesk.exe"

# --- VirtualDesktopAccessor ----------------------------------------------------------------------
if ($Dll) {
    if ((Get-FileHash $Dll).Hash -ne $DllSha256) { throw "Local DLL $Dll does not match the pinned SHA256" }
    Copy-Item $Dll "$stage\VirtualDesktopAccessor.dll"
} else { Get-Verified $DllUrl $DllSha256 "$stage\VirtualDesktopAccessor.dll" }
if ($Dll10) {
    if ((Get-FileHash $Dll10).Hash -ne $Dll10Sha256) { throw "Local DLL $Dll10 does not match the pinned SHA256" }
    Copy-Item $Dll10 "$stage\VirtualDesktopAccessor-win10.dll"
} else { Get-Verified $Dll10Url $Dll10Sha256 "$stage\VirtualDesktopAccessor-win10.dll" }   # Windows 10 build

# --- App files -----------------------------------------------------------------------------------
Copy-Item "$root\admin\*" "$stage\admin"
Copy-Item "$root\LICENSE" "$stage\LICENSE.txt"
Copy-Item "$root\THIRD-PARTY-NOTICES.md" "$stage\THIRD-PARTY-NOTICES.txt"
& "$root\make-icons.ps1" -OutDir "$stage\icons" -Outline 0 | Out-Null

@"
EzDesk $Version  (formerly Desktop Switcher)
https://github.com/alxbdn/ezdesk

START
  1. Double-click EzDesk.exe. A numbered icon appears next to the clock.
     If Windows says "Windows protected your PC": click "More info", then "Run anyway".
  2. Right-click the icon > "Start with Windows" so it starts by itself.

HOW IT WORKS (mouse only)
  Hold a window         hold it by its title bar for a moment: a glowing bump rises on the
                        left and right of the screen; bring the window into one and hold it
                        there to slide to the previous / next desktop with it
  Scroll on taskbar     previous / next desktop
  Click the icon        Task View

STOP / REMOVE
  Right-click the icon > untick "Start with Windows" > Exit, then delete this folder.

ADMIN MODE (optional, for power users)
  Makes it work with admin windows too (Task Manager, installers...).
  Double-click admin\Install admin mode.cmd. Remove it with admin\Uninstall admin mode.cmd.
"@ | Set-Content "$stage\README.txt" -Encoding UTF8

# --- Zip (the folder itself is the top-level entry, so extracting gives one tidy folder) ---------
# Entries are written by hand with "/" separators: Windows PowerShell's ZipFile uses "\", which other tools mangle.
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$zip = Join-Path $OutDir 'EzDesk.zip'
$archive = [IO.Compression.ZipFile]::Open($zip, 'Create')
try {
    foreach ($f in Get-ChildItem $stage -Recurse -File) {
        $name = 'EzDesk/' + $f.FullName.Substring($stage.Length + 1).Replace('\', '/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $f.FullName, $name, 'Optimal') | Out-Null
    }
} finally { $archive.Dispose() }
$sha = (Get-FileHash $zip).Hash
Set-Content (Join-Path $OutDir 'EzDesk.zip.sha256') -Encoding ASCII -Value "$sha  EzDesk.zip"
Write-Host "Built $zip  ($([math]::Round((Get-Item $zip).Length / 1MB, 2)) MB)  SHA256 $sha"
