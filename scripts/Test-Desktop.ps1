[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Executable)
$ErrorActionPreference = 'Stop'
# Launch hidden and exercise native controls without opening a browser or capturing.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class MonochromeUiProbe {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder name, int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr hwnd, string text);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SendText(IntPtr hwnd, uint message, IntPtr wparam, StringBuilder text);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr hwnd, uint message, IntPtr wparam, string text);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hwnd, int id);
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")] public static extern int GetWindowLong(IntPtr hwnd, int index);
    public struct Rect { public int Left, Top, Right, Bottom; }
    public static IntPtr Find(uint pid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, unused) => {
            uint owner; GetWindowThreadProcessId(hwnd, out owner);
            var name = new StringBuilder(128); GetClassName(hwnd, name, name.Capacity);
            if ((pid == 0 || owner == pid) && name.ToString() == "MonochromeDesktopWindow") { result = hwnd; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    public static IntPtr[] Edits(IntPtr parent) {
        var handles = new List<IntPtr>();
        EnumChildWindows(parent, (hwnd, unused) => {
            var name = new StringBuilder(64); GetClassName(hwnd, name, name.Capacity);
            if (name.ToString() == "Edit") handles.Add(hwnd);
            return true;
        }, IntPtr.Zero);
        handles.Sort((a, b) => { Rect ra, rb; GetWindowRect(a, out ra); GetWindowRect(b, out rb); return ra.Top.CompareTo(rb.Top); });
        return handles.ToArray();
    }
    public static string Text(IntPtr hwnd) { var text = new StringBuilder(16384); SendText(hwnd, 0xD, new IntPtr(text.Capacity), text); return text.ToString(); }
    public static string Status(IntPtr hwnd) {
        var result = new StringBuilder();
        EnumChildWindows(hwnd, (child, unused) => {
            var name = new StringBuilder(64); GetClassName(child, name, name.Capacity);
            if (name.ToString() == "Static") result.AppendLine(Text(child));
            return true;
        }, IntPtr.Zero);
        return result.ToString();
    }
}
'@
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
if ([MonochromeUiProbe]::Find(0) -ne [IntPtr]::Zero) {
    throw 'Exit existing Monochrome desktop instances before running this isolated UI test. They have not been modified.'
}
function Test-SecondLaunch([IntPtr]$ExistingWindow) {
    $second = Start-Process -FilePath $executablePath -WindowStyle Hidden -PassThru
    try {
        if (-not $second.WaitForExit(7000)) { throw 'Second app launch did not exit' }
        if ($second.ExitCode -ne 0) { throw 'Second app launch returned an error' }
        $deadline = [DateTime]::UtcNow.AddSeconds(3)
        while (-not [MonochromeUiProbe]::IsWindowVisible($ExistingWindow) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
        if (-not [MonochromeUiProbe]::IsWindowVisible($ExistingWindow)) { throw 'Second launch did not restore the existing window' }
    } finally {
        if (-not $second.HasExited) { Stop-Process -Id $second.Id }
    }
}
$portProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$portProbe.Start()
$port = $portProbe.LocalEndpoint.Port
$portProbe.Stop()
$desktopProcess = Start-Process -FilePath $executablePath -WindowStyle Hidden -PassThru
$window = [IntPtr]::Zero
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (($window -eq [IntPtr]::Zero -or [MonochromeUiProbe]::SendMessage($window, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 1) -and [DateTime]::UtcNow -lt $deadline) {
        $window = [MonochromeUiProbe]::Find($desktopProcess.Id)
        Start-Sleep -Milliseconds 100
    }
    if ($window -eq [IntPtr]::Zero) { throw 'Desktop window did not initialize' }
    if ([MonochromeUiProbe]::Text($window) -ne 'Monochrome - Share') { throw 'Branded window title is incorrect' }
    Test-SecondLaunch $window
    $labels = [MonochromeUiProbe]::Status($window)
    if (-not $labels.Contains('Monochrome - Share') -or -not $labels.Contains('1-to-1 Private screen sharing')) { throw 'Branded header text is incorrect' }
    $logoControl = [MonochromeUiProbe]::GetDlgItem($window, 208)
    if ([MonochromeUiProbe]::SendMessage($logoControl, 0x171, [IntPtr]::Zero, [IntPtr]::Zero) -eq [IntPtr]::Zero) { throw 'Header logo icon did not load' }
    if ([MonochromeUiProbe]::SendMessage($window, 0x7F, [IntPtr]0, [IntPtr]::Zero) -eq [IntPtr]::Zero) { throw 'Branded window icon did not load' }
    # IDs remain stable when the Material layout puts fields in two columns.
    $edits = @(201,202,203,204,205 | ForEach-Object { [MonochromeUiProbe]::GetDlgItem($window, $_) })
    if ($edits -contains [IntPtr]::Zero) { throw 'Expected bind, key, secret, link, and log controls' }
    $audioControl = [MonochromeUiProbe]::GetDlgItem($window, 206)
    if ([MonochromeUiProbe]::SendMessage($audioControl, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 1) { throw 'Audio should start checked' }
    [MonochromeUiProbe]::SendMessage($audioControl, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    if ([MonochromeUiProbe]::SendMessage($audioControl, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 0) { throw 'Checkbox click did not toggle audio' }
    [MonochromeUiProbe]::SendMessage($audioControl, 0x100, [IntPtr]32, [IntPtr]::Zero) | Out-Null
    [MonochromeUiProbe]::SendMessage($audioControl, 0x101, [IntPtr]32, [IntPtr]::Zero) | Out-Null
    if ([MonochromeUiProbe]::SendMessage($audioControl, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 1) { throw 'Space key did not toggle audio' }
    if ([MonochromeUiProbe]::GetWindowLong($edits[4], -16) -band 0x10000000) { throw 'Diagnostics should start collapsed' }
    [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]110, [IntPtr]::Zero) | Out-Null
    if (-not ([MonochromeUiProbe]::GetWindowLong($edits[4], -16) -band 0x10000000)) { throw 'Diagnostics did not expand' }
    [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]110, [IntPtr]::Zero) | Out-Null
    if ([MonochromeUiProbe]::GetWindowLong($edits[4], -16) -band 0x10000000) { throw 'Diagnostics did not collapse' }
    [MonochromeUiProbe]::SetText($edits[0], 0xC, [IntPtr]::Zero, "127.0.0.1:$port") | Out-Null
    if ([MonochromeUiProbe]::Text($edits[0]) -ne "127.0.0.1:$port") { throw 'Test could not set the isolated loopback bind address' }
    [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]101, [IntPtr]::Zero) | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    $link = ''
    while (-not $link.StartsWith('http://') -and [DateTime]::UtcNow -lt $deadline) {
        $link = [MonochromeUiProbe]::Text($edits[3])
        Start-Sleep -Milliseconds 100
    }
    if (-not $link.StartsWith("http://127.0.0.1:$port/#token=")) {
        throw ('Host did not publish a link. Status: ' + [MonochromeUiProbe]::Status($window) + ' Log: ' + [MonochromeUiProbe]::Text($edits[4]))
    }
    $response = Invoke-WebRequest -Uri "http://127.0.0.1:$port/health" -UseBasicParsing
    if ($response.Content -ne 'ok') { throw 'Host health failed' }
    [MonochromeUiProbe]::SendMessage($window, 5, [IntPtr]1, [IntPtr]::Zero) | Out-Null
    if ([MonochromeUiProbe]::IsWindowVisible($window)) { throw 'Minimize did not hide to tray' }
    Test-SecondLaunch $window
    if ([MonochromeUiProbe]::Text($edits[3]) -ne $link) { throw 'Second launch changed the active viewer link' }
    if ((Invoke-WebRequest -Uri "http://127.0.0.1:$port/health" -UseBasicParsing).Content -ne 'ok') { throw 'Second launch interrupted the existing host' }
    [MonochromeUiProbe]::SendMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $desktopProcess.Refresh()
    if ($desktopProcess.HasExited) { throw 'Close exited instead of hiding to tray' }
    [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]102, [IntPtr]::Zero) | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    while ([MonochromeUiProbe]::Text($edits[3]) -ne '' -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    Start-Sleep -Milliseconds 1000
    $stillServing = $false
    try { $null = Invoke-WebRequest -Uri "http://127.0.0.1:$port/health" -UseBasicParsing -TimeoutSec 1; $stillServing = $true } catch {}
    if ($stillServing) { throw 'Stop left the host listening' }
    [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]106, [IntPtr]::Zero) | Out-Null
    if (-not $desktopProcess.WaitForExit(5000)) { throw 'Exit did not close the app' }
    Write-Host 'PASS: single-instance relaunch (visible and tray-hidden), unchanged host/link, native controls, diagnostics, tray, stop, and exit'
} finally {
    $desktopProcess.Refresh()
    if (-not $desktopProcess.HasExited) {
        if ($window -ne [IntPtr]::Zero) { [MonochromeUiProbe]::SendMessage($window, 0x111, [IntPtr]106, [IntPtr]::Zero) | Out-Null }
        if (-not $desktopProcess.WaitForExit(5000)) { Stop-Process -Id $desktopProcess.Id }
    }
}
