# Generates the tray icons: a black rounded square with a blue number, 1-9 plus 'more' for 10+.
# Usage: .\make-icons.ps1 -OutDir <folder> [-Outline 0|1|2] [-Ink '#RRGGBB']
#   Outline: border width in px at 16px (0 = none); Ink: the number's (and border's) colour
# (The app's own icon is brand\ezdesk.ico; these are only the numbered tray badges.)
param([Parameter(Mandatory)] [string]$OutDir, [double]$Outline = 0, [string]$Ink = '#4FA6EC',
      [int[]]$Sizes = @(16, 20, 24, 32, 40, 48, 64),
      [string[]]$Labels = @('1','2','3','4','5','6','7','8','9','+'))
Add-Type -AssemblyName System.Drawing

$sizes = $Sizes
$labels = $Labels
$bg = [System.Drawing.Color]::Black
$inkColor = [System.Drawing.ColorTranslator]::FromHtml($Ink)    # the glow's blue: no white anywhere

function RoundRect([double]$x, [double]$y, [double]$w, [double]$h, [double]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, $r*2, $r*2, 180, 90)
    $p.AddArc($x + $w - $r*2, $y, $r*2, $r*2, 270, 90)
    $p.AddArc($x + $w - $r*2, $y + $h - $r*2, $r*2, $r*2, 0, 90)
    $p.AddArc($x, $y + $h - $r*2, $r*2, $r*2, 90, 90)
    $p.CloseFigure()
    return ,$p
}

function New-Png([string]$text, [int]$s) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.TextRenderingHint = 'AntiAliasGridFit'
    $g.Clear([System.Drawing.Color]::Transparent)

    # rounded square: small transparent margin, gently rounded corners
    $m = [math]::Max(1, [math]::Round($s * 0.04))
    $r = [math]::Max(2, [int]($s * 0.13))
    $g.PixelOffsetMode = 'Half'                     # pixel edges on whole coordinates: 1px lines stay crisp
    $side = $s - 2*$m
    $g.FillPath((New-Object System.Drawing.SolidBrush $bg), (RoundRect $m $m $side $side $r))

    # optional border, drawn inside the square so the icon doesn't grow
    if ($Outline -gt 0) {
        $pw = [math]::Max(1.0, [math]::Round($Outline * $s / 16))   # whole pixels only
        $h = $pw / 2
        $pen = [System.Drawing.Pen]::new($inkColor, [single]$pw)
        $g.DrawPath($pen, (RoundRect ($m + $h) ($m + $h) ($side - $pw) ($side - $pw) ([math]::Max(1, $r - $h))))
    }

    # centred number
    # number drawn as a path so it can be centred on its real ink, not the font's line box
    $fam = New-Object System.Drawing.FontFamily 'Segoe UI'
    $tp = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tp.AddString($text, $fam, [int][System.Drawing.FontStyle]::Bold, [float]($s * $(if ($Outline -gt 0) { 0.62 } else { 0.70 })), (New-Object System.Drawing.PointF 0, 0), [System.Drawing.StringFormat]::GenericTypographic)
    $b = $tp.GetBounds()
    $mx = New-Object System.Drawing.Drawing2D.Matrix
    $mx.Translate(($s - $b.Width) / 2 - $b.X, ($s - $b.Height) / 2 - $b.Y)
    $tp.Transform($mx)
    $g.FillPath([System.Drawing.SolidBrush]::new($inkColor), $tp)

    $g.Dispose()
    # Classic DIB icon frame: BITMAPINFOHEADER + bottom-up BGRA pixels + 1bpp AND mask
    $data = $bmp.LockBits((New-Object System.Drawing.Rectangle 0, 0, $s, $s), 'ReadOnly', ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb))
    $stride = $data.Stride
    $raw = New-Object byte[] ($stride * $s)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $raw, 0, $raw.Length)
    $bmp.UnlockBits($data); $bmp.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter $ms
    $w.Write([uint32]40); $w.Write([int32]$s); $w.Write([int32]($s * 2))       # size, width, height (XOR+AND)
    $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]0)              # planes, bpp, BI_RGB
    $w.Write([uint32]0); $w.Write([int32]0); $w.Write([int32]0); $w.Write([uint32]0); $w.Write([uint32]0)
    for ($y = $s - 1; $y -ge 0; $y--) { $w.Write($raw, $y * $stride, $s * 4) }  # bottom-up rows
    $maskRow = [int]([math]::Ceiling($s / 32) * 4)
    $w.Write((New-Object byte[] ($maskRow * $s)))                               # AND mask all zero: alpha does the work
    $w.Flush()
    return ,$ms.ToArray()
}

foreach ($label in $labels) {
    $pngs = foreach ($s in $sizes) { ,(New-Png $label $s) }
    $ms = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter $ms
    $w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)   # ICONDIR
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {                                    # ICONDIRENTRY
        $s = $sizes[$i]; $len = $pngs[$i].Length
        $w.Write([byte]($s % 256)); $w.Write([byte]($s % 256)); $w.Write([byte]0); $w.Write([byte]0)
        $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$len); $w.Write([uint32]$offset)
        $offset += $len
    }
    foreach ($png in $pngs) { $w.Write($png) }                                    # PNG payloads
    $name = if ($label -eq '+') { 'more' } else { $label }
    [System.IO.File]::WriteAllBytes((Join-Path $OutDir "$name.ico"), $ms.ToArray())
}
Get-ChildItem $OutDir -Filter *.ico | Select-Object Name, Length
