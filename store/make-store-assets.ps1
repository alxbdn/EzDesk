# Makes the package logos from the EzDesk app icon (brand\ezdesk-1080.png, rendered from brand\ezdesk.svg).
# Usage: .\make-store-assets.ps1 -OutDir <folder>
param([Parameter(Mandatory)] [string]$OutDir)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $OutDir | Out-Null
$master = Join-Path (Split-Path $PSScriptRoot) 'brand\ezdesk-1080.png'

# $fill: how much of the canvas the icon covers. The icon already has a small margin of its own,
# so the small logos use the full canvas and the Start tile keeps some breathing room.
function New-Logo([int]$s, [double]$fill, [string]$path) {
    $src = [System.Drawing.Image]::FromFile($master)
    $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppPArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'; $g.PixelOffsetMode = 'HighQuality'; $g.CompositingQuality = 'HighQuality'
    $g.Clear([System.Drawing.Color]::Transparent)
    $side = [math]::Round($s * $fill); $o = [math]::Floor(($s - $side) / 2)
    $g.DrawImage($src, $o, $o, $side, $side)
    $g.Dispose(); $src.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

New-Logo 150 0.80 (Join-Path $OutDir 'Square150x150Logo.png')   # Start menu tile
New-Logo 44  1.00 (Join-Path $OutDir 'Square44x44Logo.png')     # Start menu list, taskbar
New-Logo 50  1.00 (Join-Path $OutDir 'StoreLogo.png')           # Store / installer
Get-ChildItem $OutDir -Filter *.png | ForEach-Object { "$($_.Name)  $($_.Length) bytes" }
