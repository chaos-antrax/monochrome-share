[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$sourcePath = Join-Path $projectRoot 'logo.png'
$previewPath = Join-Path $projectRoot 'viewer\link-preview-inverted-v1.png'
$sourceHash = (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash
$testRoot = Join-Path $projectRoot ('artifacts\link-preview-test-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $testRoot | Out-Null
& (Join-Path $PSScriptRoot 'Build-LinkPreview.ps1') -OutputDirectory $testRoot
if ((Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash -ne $sourceHash) { throw 'Original logo changed' }
if ((Get-FileHash -LiteralPath $previewPath -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $testRoot 'link-preview-inverted-v1.png') -Algorithm SHA256).Hash) {
    throw 'Checked-in link preview is stale; run scripts/Build-LinkPreview.ps1'
}
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile($sourcePath)
$preview = [System.Drawing.Bitmap]::FromFile($previewPath)
$baseline = [System.Drawing.Bitmap]::new(512, 512, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($baseline)
try {
    if ($preview.Width -ne 512 -or $preview.Height -ne 512) { throw 'Preview must be 512x512' }
    # Inverting the source over white equals inverting its composition over black.
    # Render independently without a colour matrix to verify the RGB transform.
    $graphics.Clear([System.Drawing.Color]::Black)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBilinear
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $scale = [Math]::Min(512 / $source.Width, 512 / $source.Height)
    $width = [int][Math]::Round($source.Width * $scale)
    $height = [int][Math]::Round($source.Height * $scale)
    $graphics.DrawImage($source, [System.Drawing.Rectangle]::new([int]((512-$width)/2), [int]((512-$height)/2), $width, $height), 0, 0, $source.Width, $source.Height, [System.Drawing.GraphicsUnit]::Pixel)
    $dark = 0
    for ($y = 0; $y -lt 512; $y += 8) {
        for ($x = 0; $x -lt 512; $x += 8) {
            $actual = $preview.GetPixel($x, $y)
            $original = $baseline.GetPixel($x, $y)
            if ($actual.A -ne 255) { throw 'Preview must be opaque for predictable chat backgrounds' }
            foreach ($channel in @('R', 'G', 'B')) {
                if ([Math]::Abs($actual.$channel - (255 - $original.$channel)) -gt 2) { throw "Incorrect inversion at $x,$y ($channel)" }
            }
            if ($actual.R -lt 64) { $dark++ }
        }
    }
    if ($dark -lt 100 -or $preview.GetPixel(0, 0).R -lt 250) { throw 'Expected dark artwork on a white background' }
    Write-Host 'PASS: deterministic 512x512 inverted preview, opaque white background, RGB inversion, original logo unchanged.'
} finally {
    $graphics.Dispose()
    $baseline.Dispose()
    $preview.Dispose()
    $source.Dispose()
}
