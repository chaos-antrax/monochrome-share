[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$compiler = Join-Path $projectRoot '.tools\inno\ISCC.exe'
foreach ($createShortcut in @($true, $false)) {
    $testId = [guid]::NewGuid().ToString('N').Substring(0, 8)
    $testRoot = Join-Path $projectRoot "artifacts\shortcut-test-$testId"
    New-Item -ItemType Directory -Path $testRoot | Out-Null
    $testShortcut = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) "Monochrome - Share - Shortcut test $testId.lnk"
    if (Test-Path -LiteralPath $testShortcut) { throw 'Refusing to reuse an existing desktop shortcut path for the test' }
    $source = Join-Path $testRoot 'source'
    & (Join-Path $PSScriptRoot 'Build-BrandAssets.ps1') -OutputDirectory $source -InstallerImages
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\desktop.exe') -Destination $source
    & $compiler /Q "/DSourceDir=$source" "/DOutputDir=$testRoot" "/DShortcutTestId=$testId" (Join-Path $projectRoot 'packaging\monochrome.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Isolated shortcut installer compilation failed' }
    $arguments = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'Test-Installer.ps1'), '-Executable', (Join-Path $testRoot 'MonochromeShare-0.1.1-setup.exe'), '-ShortcutTestId', $testId)
    if ($createShortcut) { $arguments += '-CreateShortcut' }
    & (Join-Path $PSHOME 'powershell.exe') @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Shortcut installer regression test failed' }
}
