# Integration smoke test on a machine without an existing Snapleaf installation.
[CmdletBinding()]
param([string]$Version)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'version.ps1')
$Version = Get-SnapleafVersion $Version
$root = Split-Path -Parent $PSScriptRoot
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{694F8F31-7809-479B-B5AB-982152DAFE67}_is1'
if (Test-Path $key) { throw 'An existing Snapleaf installation is registered; smoke test will not replace it' }
$destination = Join-Path $root "target/installer-smoke/$([guid]::NewGuid())"
[IO.Directory]::CreateDirectory($destination) | Out-Null
$setup = Join-Path $root "dist/snapleaf-$Version-x64-setup.exe"
function Invoke-Setup {
    $process = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-','/NOICONS',('/DIR="' + $destination + '"')) -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(30000)) { throw 'Installer smoke test timed out' }
    return $process.ExitCode
}
$mutex = [Threading.Mutex]::new($false, 'Local\Snapleaf.SingleInstance')
try {
    if ((Invoke-Setup) -eq 0) { throw 'Installer failed to refuse a running application' }
    if (Test-Path $key) { throw 'Blocked install registered an uninstaller' }
} finally { $mutex.Dispose() }
try {
    if ((Invoke-Setup) -ne 0) { throw 'Installer failed' }
    if ((Get-ItemProperty $key).DisplayVersion -cne $Version) { throw 'Installed version mismatch' }
    $installed = Join-Path $destination 'snapleaf.exe'
    if ((Get-FileHash $installed).Hash -cne (Get-FileHash (Join-Path $root 'dist/snapleaf.exe')).Hash) { throw 'Installed EXE differs from package' }
    if (-not (Test-Path (Join-Path $destination 'LICENSE'))) { throw 'Installed LICENSE missing' }
} finally {
    $uninstaller = Join-Path $destination 'unins000.exe'
    if (Test-Path $uninstaller) {
        $process = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(30000) -or $process.ExitCode -ne 0) { throw 'Uninstaller failed' }
    }
}
if (Test-Path $key) { throw 'Uninstall registration remained after uninstall' }
if (Test-Path (Join-Path $destination 'snapleaf.exe')) { throw 'Installed executable remained after uninstall' }
Write-Host 'Installer install, running-app guard, version, payload and uninstall smoke tests passed'
