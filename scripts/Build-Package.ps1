[CmdletBinding()]
param(
    [string]$CloudflaredPath,
    [string]$InnoCompiler,
    [switch]$SkipBuild,
    [switch]$PortableOnly
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Enter-DevEnvironment.ps1')
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    if (-not $SkipBuild) {
        & cargo build --release --features media --bins --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    }
    $gstRoot = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64
    if (-not $gstRoot) { throw 'GStreamer runtime root was not found' }
    $artifacts = Join-Path $projectRoot 'artifacts'
    New-Item -ItemType Directory -Path $artifacts -Force | Out-Null
    # Fresh staging directory; never erase an existing user directory.
    $stage = Join-Path $artifacts ('MonochromeCapture-0.1.0-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    & (Join-Path $PSScriptRoot 'Build-BrandAssets.ps1') -OutputDirectory $stage -InstallerImages
    $runtime = Join-Path $stage 'runtime'
    foreach ($relative in @('bin', 'lib\gstreamer-1.0', 'libexec\gstreamer-1.0', 'share')) {
        New-Item -ItemType Directory -Path (Join-Path $runtime $relative) -Force | Out-Null
    }
    foreach ($binary in @('desktop.exe', 'monochrome-capture.exe')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "target\release\$binary") -Destination $stage
    }
    # Headers/import libraries and unrelated plugins are excluded. Keep native
    # dependency DLLs intact; prune further only after clean-machine validation.
    Get-ChildItem -LiteralPath (Join-Path $gstRoot 'bin') -Filter '*.dll' -File | Copy-Item -Destination (Join-Path $runtime 'bin')
    foreach ($tool in @('gst-inspect-1.0.exe', 'gst-launch-1.0.exe')) {
        Copy-Item -LiteralPath (Join-Path $gstRoot "bin\$tool") -Destination (Join-Path $runtime 'bin')
    }
    $plugins = @('coreelements', 'd3d11', 'nvcodec', 'videoparsersbad', 'rtp', 'rtpmanager', 'webrtc', 'nice', 'dtls', 'srtp', 'sctp', 'wasapi2', 'audioconvert', 'audioresample', 'opus', 'typefindfunctions', 'app')
    foreach ($plugin in $plugins) {
        Copy-Item -LiteralPath (Join-Path $gstRoot "lib\gstreamer-1.0\gst$plugin.dll") -Destination (Join-Path $runtime 'lib\gstreamer-1.0')
    }
    Copy-Item -LiteralPath (Join-Path $gstRoot 'libexec\gstreamer-1.0\gst-plugin-scanner.exe') -Destination (Join-Path $runtime 'libexec\gstreamer-1.0')
    Copy-Item -LiteralPath (Join-Path $gstRoot 'share\licenses') -Destination (Join-Path $runtime 'share') -Recurse
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md'), (Join-Path $projectRoot 'packaging\THIRD-PARTY-NOTICES.md') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\remote-sharing.md') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs') -Destination $stage -Recurse
    if (-not $CloudflaredPath) { $CloudflaredPath = Join-Path $projectRoot '.tools\cloudflared.exe' }
    if (Test-Path -LiteralPath $CloudflaredPath) {
        Copy-Item -LiteralPath $CloudflaredPath -Destination (Join-Path $stage 'cloudflared.exe')
        $license = Join-Path (Split-Path -Parent $CloudflaredPath) 'cloudflared-LICENSE'
        if (-not (Test-Path -LiteralPath $license)) { throw 'Put upstream cloudflared-LICENSE beside cloudflared.exe before bundling it' }
        Copy-Item -LiteralPath $license -Destination $stage
    } else { Write-Warning 'cloudflared is not bundled. Remote mode requires cloudflared.exe beside desktop.exe or on PATH.' }
    $zip = "$stage.zip"
    Compress-Archive -LiteralPath $stage -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "Portable package: $zip"
    Write-Host "Stage: $stage"
    if ($PortableOnly) { return }
    if (-not $InnoCompiler) {
        $candidate = Join-Path $projectRoot '.tools\inno\ISCC.exe'
        if (Test-Path -LiteralPath $candidate) { $InnoCompiler = $candidate }
        else {
            $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
            if ($command) { $InnoCompiler = $command.Source }
            elseif (Test-Path 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe') { $InnoCompiler = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' }
            else { throw 'Portable package built. Install Inno Setup or pass -InnoCompiler to produce the installer.' }
        }
    }
    & $InnoCompiler '/Qp' "/DSourceDir=$stage" "/DOutputDir=$artifacts" (Join-Path $projectRoot 'packaging\monochrome.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    Write-Host ('Installer: ' + (Join-Path $artifacts 'MonochromeCapture-0.1.0-setup.exe'))
} finally { Pop-Location }
