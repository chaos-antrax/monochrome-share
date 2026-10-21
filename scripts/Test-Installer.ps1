[CmdletBinding()]
param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\artifacts\MonochromeShare-0.1.1-setup.exe'),
    [ValidatePattern('^[a-f0-9]{8}$')][string]$ShortcutTestId,
    [switch]$CreateShortcut
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class InstallerPreview {
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr hwnd);
    public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr hwnd, string text);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint msg, IntPtr wp, IntPtr lp);
    public struct Rect { public int Left, Top, Right, Bottom; }
    public static string Text(IntPtr hwnd) {
        var text = new StringBuilder(4096); GetWindowText(hwnd, text, text.Capacity); return text.ToString();
    }
    public static IntPtr Find(uint[] pids) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, unused) => {
            uint pid; GetWindowThreadProcessId(hwnd, out pid);
            var name = new StringBuilder(128); GetClassName(hwnd, name, name.Capacity);
            Rect rect; GetWindowRect(hwnd, out rect);
            if (Array.IndexOf(pids, pid) >= 0 && name.ToString() == "TWizardForm" && IsWindowVisible(hwnd) && rect.Right > rect.Left && rect.Bottom > rect.Top) { result = hwnd; return false; }
            return true;
        }, IntPtr.Zero); return result;
    }
    public static IntPtr[] Controls(IntPtr parent) {
        var result = new List<IntPtr>();
        EnumChildWindows(parent, (hwnd, unused) => { if (IsWindowVisible(hwnd)) result.Add(hwnd); return true; }, IntPtr.Zero);
        return result.ToArray();
    }
}
'@
function Save-Preview([IntPtr]$Window, [string]$Name) {
    $rect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect($Window, [ref]$rect) | Out-Null
    $bitmap = [System.Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        # Capture only setup, even if another application has the foreground.
        # Our custom navigation painting supports WM_PRINTCLIENT as well as WM_PAINT.
        if (-not [InstallerPreview]::PrintWindow($Window, $dc, 2)) { throw 'Could not capture setup preview' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try { $bitmap.Save((Join-Path $PSScriptRoot "..\artifacts\$Name"), [System.Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
}
function Assert-NeutralButton([IntPtr]$Window, [IntPtr]$Button, [string]$Name) {
    $windowRect = New-Object InstallerPreview+Rect
    $buttonRect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect($Window, [ref]$windowRect) | Out-Null
    [InstallerPreview]::GetWindowRect($Button, [ref]$buttonRect) | Out-Null
    $preview = [System.Drawing.Bitmap]::new((Join-Path $PSScriptRoot "..\artifacts\$Name"))
    try {
        $color = $preview.GetPixel($buttonRect.Left-$windowRect.Left+10, $buttonRect.Top-$windowRect.Top+10)
        if ([Math]::Abs([int]$color.R-$color.G) -gt 2 -or [Math]::Abs([int]$color.G-$color.B) -gt 2 -or $color.R -gt 100) {
            throw 'Navigation button is not neutral dark gray'
        }
    } finally { $preview.Dispose() }
}
$ownedIds = [System.Collections.Generic.HashSet[uint32]]::new()
$previousDpi = [InstallerPreview]::SetThreadDpiAwarenessContext([IntPtr](-4))
$started = [DateTime]::Now
$testInstallDirectory = $null
try {
    if ($ShortcutTestId) {
        $testRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\artifacts\shortcut-test-$ShortcutTestId"))
        $expectedSetup = Join-Path $testRoot 'MonochromeShare-0.1.1-setup.exe'
        if ((Resolve-Path -LiteralPath $Executable).Path -ne $expectedSetup) { throw 'Shortcut installation tests only accept their isolated fixture executable' }
    }
    $logPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\artifacts\installer-preview.log'))
    $process = Start-Process -FilePath (Resolve-Path -LiteralPath $Executable).Path -ArgumentList '/SP-', '/NORESTART', ('/LOG="' + $logPath + '"') -WindowStyle Hidden -PassThru
    $ownedIds.Add([uint32]$process.Id) | Out-Null
    $window = [IntPtr]::Zero
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while ($window -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) {
        foreach ($item in Get-CimInstance Win32_Process) {
            if ($ownedIds.Contains([uint32]$item.ParentProcessId) -and $item.CreationDate -ge $started) {
                $ownedIds.Add([uint32]$item.ProcessId) | Out-Null
            }
        }
        $window = [InstallerPreview]::Find([uint32[]]@($ownedIds))
        Start-Sleep -Milliseconds 100
    }
    if ($window -eq [IntPtr]::Zero) { throw 'Installer welcome window did not appear' }
    Start-Sleep -Milliseconds 700
    [InstallerPreview]::ShowWindow($window, 4) | Out-Null
    Start-Sleep -Milliseconds 200
    $controls = [InstallerPreview]::Controls($window)
    $captions = @($controls | ForEach-Object { [InstallerPreview]::Text($_) })
    Save-Preview $window 'installer-welcome-preview.png'
    if (-not ($captions | Where-Object { $_.Trim() -eq 'Monochrome - Share' })) { throw 'Missing installer brand heading' }
    if (-not ($captions | Where-Object { $_ -like '1-to-1 Private screen sharing*' })) { throw 'Missing installer description' }
    if ([InstallerPreview]::Text($window) -notlike '*Monochrome - Share*') { throw 'Missing installer brand title' }
    $next = @($controls | Where-Object { [InstallerPreview]::Text($_) -match '^&?Next' })
    if ($next.Count -ne 1) { throw 'Cannot identify welcome Next button safely' }
    Assert-NeutralButton $window $next[0] 'installer-welcome-preview.png'
    [InstallerPreview]::SendMessage($next[0], 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 250
    $captions = @([InstallerPreview]::Controls($window) | ForEach-Object { [InstallerPreview]::Text($_) })
    foreach ($caption in @('Create a desktop shortcut', 'Install')) {
        if ($captions -notcontains $caption) { throw "Missing installation option: $caption" }
    }
    Save-Preview $window 'installer-options-preview.png'
    $install = @([InstallerPreview]::Controls($window) | Where-Object { [InstallerPreview]::Text($_) -eq 'Install' })[0]
    Assert-NeutralButton $window $install 'installer-options-preview.png'
    $back = @([InstallerPreview]::Controls($window) | Where-Object { [InstallerPreview]::Text($_) -eq '&Back' })[0]
    $cancel = @([InstallerPreview]::Controls($window) | Where-Object { [InstallerPreview]::Text($_) -eq 'Cancel' })[0]
    $windowRect = New-Object InstallerPreview+Rect
    $installRect = New-Object InstallerPreview+Rect
    $backRect = New-Object InstallerPreview+Rect
    $cancelRect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect($window, [ref]$windowRect) | Out-Null
    [InstallerPreview]::GetWindowRect($install, [ref]$installRect) | Out-Null
    [InstallerPreview]::GetWindowRect($back, [ref]$backRect) | Out-Null
    [InstallerPreview]::GetWindowRect($cancel, [ref]$cancelRect) | Out-Null
    if ($cancelRect.Left -ge $backRect.Left -or $backRect.Right -ge $installRect.Left) { throw 'Incorrect Cancel / Back / Install ordering' }
    if ($backRect.Top -ne $installRect.Top -or $cancelRect.Top -ne $installRect.Top) { throw 'Navigation buttons are not aligned' }
    $leftMargin = $cancelRect.Left-$windowRect.Left
    $rightMargin = $windowRect.Right-$installRect.Right
    if ([Math]::Abs($leftMargin-$rightMargin) -gt 3) { throw 'Navigation corner margins differ' }
    if (($backRect.Right-$backRect.Left) -ne ($installRect.Right-$installRect.Left) -or ($cancelRect.Right-$cancelRect.Left) -ne ($installRect.Right-$installRect.Left)) { throw 'Navigation button widths differ' }
    $pathControl = @([InstallerPreview]::Controls($window) | Where-Object { [InstallerPreview]::Text($_) -match '^[A-Z]:\\' })[0]
    if (-not $pathControl) { throw 'Installation directory edit was not found' }
    $pathRect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect($pathControl, [ref]$pathRect) | Out-Null
    $pathSurface = [InstallerPreview]::GetParent($pathControl)
    $surfaceRect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect($pathSurface, [ref]$surfaceRect) | Out-Null
    $topPadding = $pathRect.Top-$surfaceRect.Top
    $bottomPadding = $surfaceRect.Bottom-$pathRect.Bottom
    if ($topPadding -lt 1 -or [Math]::Abs($topPadding-$bottomPadding) -gt 1) { throw "Directory text edit is not vertically centered within its field (top=$topPadding, bottom=$bottomPadding, height=$($pathRect.Bottom-$pathRect.Top))" }
    if ($pathRect.Left-$surfaceRect.Left -lt 6) { throw 'Directory text needs horizontal padding' }
    $originalDirectory = [InstallerPreview]::Text($pathControl)
    try {
        $editedDirectory = $originalDirectory + '-input-check'
        if (-not [InstallerPreview]::SetWindowText($pathControl, $editedDirectory) -or [InstallerPreview]::Text($pathControl) -ne $editedDirectory) { throw 'Installation directory field is not editable' }
    } finally { [InstallerPreview]::SetWindowText($pathControl, $originalDirectory) | Out-Null }
    $outlineRect = New-Object InstallerPreview+Rect
    [InstallerPreview]::GetWindowRect([InstallerPreview]::GetParent($pathSurface), [ref]$outlineRect) | Out-Null
    $preview = [System.Drawing.Bitmap]::new((Join-Path $PSScriptRoot '..\artifacts\installer-options-preview.png'))
    try {
        $borderColor = $preview.GetPixel($outlineRect.Left-$windowRect.Left, $outlineRect.Top-$windowRect.Top+5)
        if ($borderColor.R -gt 100 -or $borderColor.G -gt 100 -or $borderColor.B -gt 100) { throw 'Directory field border is too bright' }
    } finally { $preview.Dispose() }
    $shortcut = @([InstallerPreview]::Controls($window) | Where-Object { [InstallerPreview]::Text($_) -eq 'Create a desktop shortcut' })[0]
    $checkedBefore = [InstallerPreview]::SendMessage($shortcut, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero)
    [InstallerPreview]::SendMessage($shortcut, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $checkedAfter = [InstallerPreview]::SendMessage($shortcut, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($checkedBefore -eq $checkedAfter) { throw 'Desktop shortcut checkbox did not toggle' }
    [InstallerPreview]::SendMessage($shortcut, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    if ($ShortcutTestId) {
        # Require a compiler-injected marker before EVER clicking Install.
        if (-not ((Get-Content -Raw -LiteralPath $logPath).Contains("Shortcut regression test ID: $ShortcutTestId"))) { throw 'Setup is not a verified isolated shortcut-test build' }
        $testInstallDirectory = Join-Path $testRoot 'installed'
        if ($originalDirectory -ne $testInstallDirectory) { throw 'Shortcut-test destination is not the isolated fixture directory' }
        $isChecked = [InstallerPreview]::SendMessage($shortcut, 0xF0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 0
        if ($isChecked -ne $CreateShortcut.IsPresent) { [InstallerPreview]::SendMessage($shortcut, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
        [InstallerPreview]::SendMessage($install, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        $finish = @()
        while ($finish.Count -ne 1 -and [DateTime]::UtcNow -lt $deadline) {
            $finish = @([InstallerPreview]::Controls($window) | Where-Object { ([InstallerPreview]::Text($_) -replace '&', '') -eq 'Finish' })
            Start-Sleep -Milliseconds 100
        }
        if ($finish.Count -ne 1) { throw 'Isolated shortcut installation did not finish' }
        $shortcutPath = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) "Monochrome - Share - Shortcut test $ShortcutTestId.lnk"
        if ((Test-Path -LiteralPath $shortcutPath) -ne $CreateShortcut.IsPresent) { throw "Desktop shortcut existence does not match checkbox ($CreateShortcut)" }
        if ($CreateShortcut) {
            $shell = New-Object -ComObject WScript.Shell
            $link = $shell.CreateShortcut($shortcutPath)
            if ($link.TargetPath -ne (Join-Path $testInstallDirectory 'desktop.exe')) { throw 'Desktop shortcut target is incorrect' }
            [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($link) | Out-Null
            [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null
        }
        [InstallerPreview]::SendMessage($finish[0], 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        Write-Host "PASS: isolated installation, desktop shortcut checked=$($CreateShortcut.IsPresent), actual shortcut existence/target verified."
    } else {
        Write-Host 'PASS: branding, navigation layout/colors, centered editable directory field, shortcut toggle; previews saved. No installation performed.'
    }
} finally {
    # Production preview never installs; installation requires the verified fixture.
    foreach ($ownedId in $ownedIds) {
        $ownedProcess = Get-Process -Id $ownedId -ErrorAction SilentlyContinue
        if ($ownedProcess -and $ownedProcess.StartTime -ge $started) { Stop-Process -Id $ownedId -ErrorAction SilentlyContinue }
    }
    [InstallerPreview]::SetThreadDpiAwarenessContext($previousDpi) | Out-Null
    if ($testInstallDirectory) {
        $uninstaller = Join-Path $testInstallDirectory 'unins000.exe'
        if (Test-Path -LiteralPath $uninstaller) {
            $cleanup = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -WindowStyle Hidden -PassThru -Wait
            if ($cleanup.ExitCode -ne 0) { throw 'Isolated shortcut-test uninstall failed' }
            $testShortcut = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) "Monochrome - Share - Shortcut test $ShortcutTestId.lnk"
            if (Test-Path -LiteralPath $testShortcut) { throw 'Temporary shortcut was not removed by uninstall' }
            Write-Host 'Cleaned up isolated test installation and temporary desktop shortcut.'
        }
    }
}
