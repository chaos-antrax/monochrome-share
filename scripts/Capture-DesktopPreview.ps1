[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\desktop-preview.png')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class MonochromePreview {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder name, int length);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wp, IntPtr lp);
    public struct Rect { public int Left, Top, Right, Bottom; }
    public static IntPtr Find(uint pid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, unused) => {
            uint owner; GetWindowThreadProcessId(hwnd, out owner);
            var name = new StringBuilder(128); GetClassName(hwnd, name, name.Capacity);
            if (owner == pid && name.ToString() == "MonochromeDesktopWindow") { result = hwnd; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
'@
$oldKey = $env:SCREEN_SHARE_TURN_KEY_ID
$oldToken = $env:SCREEN_SHARE_TURN_API_TOKEN
$previewProcess = $null
$window = [IntPtr]::Zero
try {
    # Override both fields with synthetic values. Never screenshot actual credentials.
    $env:SCREEN_SHARE_TURN_KEY_ID = 'Your Cloudflare TURN key ID'
    $env:SCREEN_SHARE_TURN_API_TOKEN = 'synthetic-preview-token'
    $previewProcess = Start-Process -FilePath (Resolve-Path -LiteralPath $Executable).Path -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ($window -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) {
        $window = [MonochromePreview]::Find($previewProcess.Id)
        Start-Sleep -Milliseconds 100
    }
    if ($window -eq [IntPtr]::Zero) { throw 'Preview window did not initialize' }
    [MonochromePreview]::ShowWindow($window, 4) | Out-Null
    Start-Sleep -Milliseconds 300
    # GetWindowRect must use physical pixels, matching PrintWindow's bitmap.
    $previousDpiContext = [MonochromePreview]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $rect = New-Object MonochromePreview+Rect
    [MonochromePreview]::GetWindowRect($window, [ref]$rect) | Out-Null
    $bitmap = [System.Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [MonochromePreview]::PrintWindow($window, $dc, 2)) { throw 'Could not render desktop preview' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try { $bitmap.Save([System.IO.Path]::GetFullPath($OutputPath), [System.Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
    Write-Host "Preview: $OutputPath"
} finally {
    $env:SCREEN_SHARE_TURN_KEY_ID = $oldKey
    $env:SCREEN_SHARE_TURN_API_TOKEN = $oldToken
    if ($previousDpiContext -and $previousDpiContext -ne [IntPtr]::Zero) { [MonochromePreview]::SetThreadDpiAwarenessContext($previousDpiContext) | Out-Null }
    if ($previewProcess) {
        if ($window -ne [IntPtr]::Zero) { [MonochromePreview]::SendMessage($window, 0x111, [IntPtr]106, [IntPtr]::Zero) | Out-Null }
        if (-not $previewProcess.WaitForExit(5000)) { Stop-Process -Id $previewProcess.Id }
    }
}
