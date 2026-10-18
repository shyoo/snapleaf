# Recreate the Explorer icon from the same artwork as ui::make_icon.
# Run with Windows PowerShell 5.1 (System.Drawing), from any directory.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$frames = @()
$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
foreach ($size in $sizes) {
    $bitmap = New-Object System.Drawing.Bitmap ($size * 4), ($size * 4)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = 'AntiAlias'
    $graphics.ScaleTransform($size / 8.0, $size / 8.0)
    $tile = New-Object System.Drawing.Drawing2D.GraphicsPath
    foreach ($arc in @(@(1,1,180), @(17,1,270), @(17,17,0), @(1,17,90))) {
        $tile.AddArc($arc[0], $arc[1], 14, 14, $arc[2], 90)
    }
    $tile.CloseFigure()
    $blue = New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#0067C0'))
    $red = New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#FF5252'))
    $whitePen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 2.4
    $redPen = New-Object System.Drawing.Pen ($red.Color), 2.6
    $graphics.FillPath($blue, $tile)
    foreach ($corner in @(@(7,7,1,1), @(25,7,-1,1), @(7,25,1,-1), @(25,25,-1,-1))) {
        $x, $y, $dx, $dy = $corner
        $graphics.DrawLine($whitePen, $x, $y, ($x + $dx * 6), $y)
        $graphics.DrawLine($whitePen, $x, $y, $x, ($y + $dy * 6))
    }
    $graphics.DrawLine($redPen, 11.0, 21.0, 18.5, 13.5)
    $graphics.FillPolygon($red, [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF 22.5,9.5),
        (New-Object System.Drawing.PointF 14.5,12.0),
        (New-Object System.Drawing.PointF 20.0,17.5)))
    $small = New-Object System.Drawing.Bitmap $size, $size
    $downsample = [System.Drawing.Graphics]::FromImage($small)
    $downsample.InterpolationMode = 'HighQualityBicubic'
    $downsample.DrawImage($bitmap, 0, 0, $size, $size)
    $stream = New-Object System.IO.MemoryStream
    $small.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $frames += ,$stream.ToArray()
    foreach ($object in @($stream, $downsample, $small, $redPen, $whitePen, $red, $blue, $tile, $graphics, $bitmap)) {
        $object.Dispose()
    }
}
$assetDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets'
New-Item -ItemType Directory -Force $assetDir | Out-Null
$file = [System.IO.File]::Create((Join-Path $assetDir 'snipper.ico'))
$writer = New-Object System.IO.BinaryWriter $file
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        $dimension = [byte]($sizes[$i] % 256)
        $writer.Write($dimension)
        $writer.Write($dimension)
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$frames[$i].Length)
        $writer.Write([uint32]$offset)
        $offset += $frames[$i].Length
    }
    foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
} finally {
    $writer.Dispose()
}
