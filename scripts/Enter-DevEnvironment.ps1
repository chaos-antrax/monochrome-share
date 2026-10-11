# Dot-source: . .\scripts\Enter-DevEnvironment.ps1
$projectRoot = Split-Path -Parent $PSScriptRoot
$localCargo = Join-Path $projectRoot '.cargo-local'
$localRustup = Join-Path $projectRoot '.rustup-local'
if (Test-Path -LiteralPath (Join-Path $localCargo 'bin\cargo.exe')) {
    $env:CARGO_HOME = $localCargo
    $env:RUSTUP_HOME = $localRustup
    $env:PATH = (Join-Path $localCargo 'bin') + ';' + $env:PATH
}
# Override a machine-wide offline flag for this shell only.
$env:CARGO_NET_OFFLINE = 'false'
if (-not $env:GSTREAMER_1_0_ROOT_MSVC_X86_64) {
    $installedGstreamer = 'C:\Program Files\gstreamer\1.0\msvc_x86_64'
    if (Test-Path -LiteralPath (Join-Path $installedGstreamer 'bin\gst-inspect-1.0.exe')) {
        $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = $installedGstreamer
    }
}
if ($env:GSTREAMER_1_0_ROOT_MSVC_X86_64) {
    $env:PATH = (Join-Path $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 'bin') + ';' + $env:PATH
    $env:PKG_CONFIG_PATH = (Join-Path $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 'lib\pkgconfig')
}
