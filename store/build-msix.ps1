# Builds the Microsoft Store package from the release folder made by build-release.ps1.
#   .\store\build-msix.ps1 -Version 1.0.2            layout in dist\msix-layout, + dist\DesktopSwitcher.msix if makeappx is available
#   .\store\build-msix.ps1 -Version 1.0.2 -LayoutOnly  just the layout (enough for a local test with Developer Mode)
# The package is unsigned: the Store signs it on submission.
param(
    [Parameter(Mandatory)] [string]$Version,
    [string]$AppDir,
    [switch]$LayoutOnly
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
if (-not $AppDir) { $AppDir = Join-Path $root 'dist\DesktopSwitcher' }
if (-not (Test-Path "$AppDir\DesktopSwitcher.exe")) { throw "Run build-release.ps1 first: $AppDir\DesktopSwitcher.exe not found" }

# Store versions are Major.Minor.Build.0 (the last number is reserved for the Store)
$parts = @($Version.Split('.') | ForEach-Object { [int]$_ }); while ($parts.Count -lt 3) { $parts += 0 }
$msixVersion = '{0}.{1}.{2}.0' -f $parts[0], $parts[1], $parts[2]

$id = Get-Content (Join-Path $PSScriptRoot 'identity.json') -Raw | ConvertFrom-Json
$layout = Join-Path $root 'dist\msix-layout'
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force "$layout\icons" | Out-Null

# App files: everything the simple mode needs. No admin\ folder: a Store app never runs elevated.
Copy-Item "$AppDir\DesktopSwitcher.exe", "$AppDir\DesktopSwitcher.ahk", "$AppDir\VirtualDesktopAccessor.dll", "$AppDir\VirtualDesktopAccessor-win10.dll" $layout
Copy-Item "$AppDir\icons\*.ico" "$layout\icons"
Copy-Item "$AppDir\LICENSE.txt", "$AppDir\LICENSE-AutoHotkey.txt", "$AppDir\THIRD-PARTY-NOTICES.txt" $layout
& (Join-Path $PSScriptRoot 'make-store-assets.ps1') -OutDir "$layout\Assets" | Out-Null

$xml = (Get-Content (Join-Path $PSScriptRoot 'AppxManifest.template.xml') -Raw).
    Replace('{IDENTITY_NAME}', $id.IdentityName).Replace('{PUBLISHER}', $id.Publisher).
    Replace('{PUBLISHER_DISPLAY_NAME}', $id.PublisherDisplayName).Replace('{VERSION}', $msixVersion)
[xml]$xml | Out-Null                                   # fail early on malformed XML
[IO.File]::WriteAllText("$layout\AppxManifest.xml", $xml, (New-Object Text.UTF8Encoding $false))
Write-Host "Layout ready: $layout  (version $msixVersion, identity $($id.IdentityName))"
if ($LayoutOnly) { return }

$makeappx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter makeappx.exe -ErrorAction SilentlyContinue |
    Where-Object FullName -like '*\x64\*' | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $makeappx) { throw 'makeappx.exe not found (Windows SDK). Use -LayoutOnly, or build on GitHub Actions.' }
$msix = Join-Path $root 'dist\DesktopSwitcher.msix'
& $makeappx pack /o /d $layout /p $msix | Out-Host           # makeappx also validates the manifest
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
Write-Host "Built $msix  ($([math]::Round((Get-Item $msix).Length / 1MB, 2)) MB)"
