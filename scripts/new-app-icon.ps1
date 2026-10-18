# Recreate the Snapleaf app icon and web favicon assets.
# Run with Windows PowerShell 5.1 (System.Drawing), from any directory.
# Geometry matches assets/snapleaf.svg, in a 32 x 32 coordinate system.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$assetDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets'
New-Item -ItemType Directory -Force $assetDir | Out-Null
$frames = @{}
$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 180, 192, 256, 512)
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
    $green = New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#11735B'))
    $lime = New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#DDF7B0'))
    $whitePen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 2
    $veinPen = New-Object System.Drawing.Pen ($green.Color), 1.5
    $veinPen.StartCap = 'Round'
    $veinPen.EndCap = 'Round'
    $graphics.FillPath($green, $tile)
    foreach ($corner in @(@(7,7,1,1), @(25,25,-1,-1))) {
        $x, $y, $dx, $dy = $corner
        $graphics.DrawLine($whitePen, $x, $y, ($x + $dx * 6), $y)
        $graphics.DrawLine($whitePen, $x, $y, $x, ($y + $dy * 6))
    }
    $leaf = New-Object System.Drawing.Drawing2D.GraphicsPath
    $leaf.AddBezier(10,22, 6,13, 15,9, 24,8)
    $leaf.AddBezier(24,8, 25,17, 21,25, 10,22)
    $leaf.CloseFigure()
    $graphics.FillPath($lime, $leaf)
    $graphics.DrawLine($veinPen, 10,24, 20,14)
    $small = New-Object System.Drawing.Bitmap $size, $size
    $downsample = [System.Drawing.Graphics]::FromImage($small)
    $downsample.InterpolationMode = 'HighQualityBicubic'
    $downsample.DrawImage($bitmap, 0, 0, $size, $size)
    $stream = New-Object System.IO.MemoryStream
    $small.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $frames[$size] = $stream.ToArray()
    foreach ($object in @($stream, $downsample, $small, $leaf, $veinPen, $whitePen, $lime, $green, $tile, $graphics, $bitmap)) {
        $object.Dispose()
    }
}
function Write-Icon($name, $iconSizes) {
    $file = [System.IO.File]::Create((Join-Path $assetDir $name))
    $writer = New-Object System.IO.BinaryWriter $file
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$iconSizes.Count)
        $offset = 6 + 16 * $iconSizes.Count
        foreach ($size in $iconSizes) {
            $dimension = [byte]($size % 256)
            $writer.Write($dimension)
            $writer.Write($dimension)
            $writer.Write([uint16]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$frames[$size].Length)
            $writer.Write([uint32]$offset)
            $offset += $frames[$size].Length
        }
        foreach ($size in $iconSizes) { $writer.Write([byte[]]$frames[$size]) }
    } finally {
        $writer.Dispose()
    }
}
Write-Icon 'snapleaf.ico' @(16, 20, 24, 32, 40, 48, 64, 128, 256)
Write-Icon 'favicon.ico' @(16, 32, 48)
foreach ($size in @(16, 32)) {
    [IO.File]::WriteAllBytes((Join-Path $assetDir "favicon-$($size)x$($size).png"), $frames[$size])
}
[IO.File]::WriteAllBytes((Join-Path $assetDir 'apple-touch-icon.png'), $frames[180])
foreach ($size in @(192, 512)) {
    [IO.File]::WriteAllBytes((Join-Path $assetDir "snapleaf-$size.png"), $frames[$size])
}
