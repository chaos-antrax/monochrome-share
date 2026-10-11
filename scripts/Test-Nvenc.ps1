[CmdletBinding()]
param(
    [ValidateRange(1, 3600)][int]$Seconds = 30,
    [ValidateRange(-1, 32)][int]$Monitor = -1,
    [ValidateRange(-1, 32)][int]$Adapter = -1,
    [ValidateRange(1000, 50000)][int]$BitrateKbps = 10000
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Enter-DevEnvironment.ps1')
if (-not (Get-Command gst-launch-1.0 -ErrorAction SilentlyContinue)) {
    throw 'Install GStreamer MSVC x64 1.26+ runtime and development packages. See README.md.'
}
# Discard compressed output. The spike does not save desktop recordings.
# Observe CPU/GPU in Task Manager during this run; do not infer performance from caps.
$arguments = @(
    '-e', 'd3d11screencapturesrc', 'capture-api=wgc', "monitor-index=$Monitor", "adapter=$Adapter", 'show-cursor=true', "num-buffers=$($Seconds * 60)",
    '!', 'video/x-raw(memory:D3D11Memory),framerate=60/1',
    '!', 'd3d11convert', '!', 'video/x-raw(memory:D3D11Memory),format=NV12,width=1920,height=1080',
    '!', 'queue', 'max-size-buffers=2', 'max-size-bytes=0', 'max-size-time=0', 'leaky=downstream',
    '!', 'nvd3d11h264enc', "bitrate=$BitrateKbps", 'rc-mode=cbr', 'preset=p4', 'tune=ultra-low-latency',
    'bframes=0', 'rc-lookahead=0', 'zerolatency=true', 'gop-size=300', 'repeat-sequence-header=true',
    '!', 'video/x-h264,profile=main', '!', 'h264parse', '!', 'fakesink', 'sync=false'
)
Write-Host "Running a $Seconds-second 1080p60 NVENC spike (no recording)."
& gst-launch-1.0 @arguments
if ($LASTEXITCODE -ne 0) { throw "NVENC spike failed with exit code $LASTEXITCODE" }
Write-Host 'Pipeline completed. Record measured CPU, GPU, encoder utilization, and static-screen behavior in docs/validation.md.'
