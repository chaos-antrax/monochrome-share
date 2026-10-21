[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\viewer')
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile((Join-Path $projectRoot 'logo.png'))
$bitmap = [System.Drawing.Bitmap]::new(512, 512, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$attributes = [System.Drawing.Imaging.ImageAttributes]::new()
try {
    # A separate preview-only derivative. Never modify the original brand asset.
    # Invert RGB, preserve the source alpha, and composite onto opaque white so
    # chat clients' light/dark backgrounds cannot hide the inverted artwork.
    $matrix = [System.Drawing.Imaging.ColorMatrix]::new()
    $matrix.Matrix00 = -1
    $matrix.Matrix11 = -1
    $matrix.Matrix22 = -1
    $matrix.Matrix40 = 1
    $matrix.Matrix41 = 1
    $matrix.Matrix42 = 1
    $attributes.SetColorMatrix($matrix)
    $graphics.Clear([System.Drawing.Color]::White)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBilinear
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $scale = [Math]::Min(512 / $source.Width, 512 / $source.Height)
    $width = [int][Math]::Round($source.Width * $scale)
    $height = [int][Math]::Round($source.Height * $scale)
    $rectangle = [System.Drawing.Rectangle]::new([int]((512-$width)/2), [int]((512-$height)/2), $width, $height)
    $graphics.DrawImage($source, $rectangle, 0, 0, $source.Width, $source.Height, [System.Drawing.GraphicsUnit]::Pixel, $attributes)
    $path = Join-Path $destination 'link-preview-inverted-v1.png'
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Host "Link preview: $path"
} finally {
    $attributes.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
    $source.Dispose()
}
