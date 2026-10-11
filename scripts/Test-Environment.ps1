[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Enter-DevEnvironment.ps1')
function Test-GstElement([string]$Name) {
    # Windows PowerShell treats native stderr as errors even for plugin warnings.
    # Decide availability from gst-inspect's exit code, not diagnostic output.
    $ErrorActionPreference = 'Continue'
    & gst-inspect-1.0 $Name *> $null
    return ($LASTEXITCODE -eq 0)
}
$missing = [System.Collections.Generic.List[string]]::new()
$commands = @('rustc', 'cargo', 'gst-inspect-1.0', 'gst-launch-1.0', 'nvidia-smi')
foreach ($name in $commands) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($command) { Write-Host ('OK  {0}: {1}' -f $name, $command.Source) }
    else { $missing.Add($name); Write-Host ('MISSING  {0}' -f $name) }
}
$os = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
Write-Host ('Windows: {0}, build {1}' -f $os.ProductName, $os.CurrentBuildNumber)
if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) {
    & nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
    if ($LASTEXITCODE -ne 0) { $missing.Add('Working NVIDIA driver') }
}
if (Get-Command gst-inspect-1.0 -ErrorAction SilentlyContinue) {
    & gst-inspect-1.0 --version
    $elements = @('d3d11screencapturesrc', 'd3d11convert', 'nvd3d11h264enc', 'h264parse', 'rtph264pay', 'webrtcbin', 'nicesrc', 'dtlssrtpenc', 'wasapi2src', 'opusenc', 'rtpopuspay')
    foreach ($element in $elements) {
        if (Test-GstElement $element) { Write-Host ('OK  element {0}' -f $element) }
        else { $missing.Add($element); Write-Host ('MISSING  element {0}' -f $element) }
    }
}
if ($missing.Count -gt 0) {
    Write-Host ('Not ready for capture: {0}' -f ($missing -join ', '))
    exit 1
}
Write-Host 'Prerequisites found. Run Test-Nvenc.ps1 to verify actual GPU capture and encoding.'
