# Draws the package logos in the tray-badge style: black rounded square, white outline, white "1".
# Usage: .\make-store-assets.ps1 -OutDir <folder>
param([Parameter(Mandatory)] [string]$OutDir)
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $OutDir | Out-Null

function RoundRect([double]$x, [double]$y, [double]$w, [double]$h, [double]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, $r*2, $r*2, 180, 90)
    $p.AddArc($x + $w - $r*2, $y, $r*2, $r*2, 270, 90)
    $p.AddArc($x + $w - $r*2, $y + $h - $r*2, $r*2, $r*2, 0, 90)
    $p.AddArc($x, $y + $h - $r*2, $r*2, $r*2, 90, 90)
    $p.CloseFigure()
    return ,$p
}

# $fill: how much of the canvas the badge covers (Store logos keep some breathing room)
function New-Logo([int]$s, [double]$fill, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'; $g.PixelOffsetMode = 'Half'
    $g.Clear([System.Drawing.Color]::Transparent)
    $side = [math]::Round($s * $fill); $o = [math]::Floor(($s - $side) / 2)
    $r = $side * 0.16
    $g.FillPath([System.Drawing.Brushes]::Black, (RoundRect $o $o $side $side $r))
    $pw = [math]::Max(1.0, [math]::Round($side / 16))
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), $pw
    $g.DrawPath($pen, (RoundRect ($o + $pw/2) ($o + $pw/2) ($side - $pw) ($side - $pw) ([math]::Max(1, $r - $pw/2))))
    $fam = New-Object System.Drawing.FontFamily 'Segoe UI'
    $tp = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tp.AddString('1', $fam, [int][System.Drawing.FontStyle]::Bold, [float]($side * 0.62), (New-Object System.Drawing.PointF 0, 0), [System.Drawing.StringFormat]::GenericTypographic)
    $b = $tp.GetBounds()
    $mx = New-Object System.Drawing.Drawing2D.Matrix
    $mx.Translate(($s - $b.Width) / 2 - $b.X, ($s - $b.Height) / 2 - $b.Y)
    $tp.Transform($mx)
    $g.FillPath([System.Drawing.Brushes]::White, $tp)
    $g.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

New-Logo 150 0.66 (Join-Path $OutDir 'Square150x150Logo.png')   # Start menu tile
New-Logo 44  0.92 (Join-Path $OutDir 'Square44x44Logo.png')     # Start menu list, taskbar
New-Logo 50  0.92 (Join-Path $OutDir 'StoreLogo.png')           # Store / installer
Get-ChildItem $OutDir -Filter *.png | ForEach-Object { "$($_.Name)  $($_.Length) bytes" }
