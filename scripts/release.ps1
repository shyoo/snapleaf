<#
.SYNOPSIS
    Build a release of Snipper into dist\.

.DESCRIPTION
    Runs the unit tests, builds the optimised executable, and writes
    dist\snipper.exe plus dist\snipper-<version>-x64.zip with a SHA-256 next to it.
    Works from any current directory, in Windows PowerShell 5.1 or PowerShell 7.

.PARAMETER SkipTests
    Skip `cargo test`.

.PARAMETER StopRunning
    Stop Snipper first if it is running from target\release or dist\
    (a running exe cannot be overwritten).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\release.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$StopRunning
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }

function Invoke-Cargo {
    & $script:cargo @args
    if ($LASTEXITCODE -ne 0) { throw "cargo $($args -join ' ') failed (exit $LASTEXITCODE)" }
}

# Find cargo, even in a shell started before rustup put it on PATH.
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $candidate = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (Test-Path $candidate) { $cargo = $candidate }
}
if (-not $cargo) {
    throw "cargo not found. Install Rust with: winget install Rustlang.Rustup  (then open a new terminal)"
}

$manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') { throw 'Could not read version from Cargo.toml' }
$version = $Matches[1]

$built = Join-Path $root 'target\release\snipper.exe'
$dist = Join-Path $root 'dist'

# A running copy locks its exe; the build or the copy would fail.
$locked = @(Get-Process snipper -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and ($_.Path -ieq $built -or $_.Path -like (Join-Path $dist '*'))
})
if ($locked.Count -gt 0) {
    if ($StopRunning) {
        Step 'Stopping running Snipper'
        $locked | Stop-Process -Force
        Start-Sleep -Milliseconds 300
    } else {
        throw "Snipper is running from $($locked[0].Path). Exit it from the tray, or re-run with -StopRunning."
    }
}

if (-not $SkipTests) {
    Step 'cargo test'
    Invoke-Cargo test --quiet
}

Step "cargo build --release (v$version)"
Invoke-Cargo build --release

Step 'Packaging'
New-Item -ItemType Directory -Force $dist | Out-Null
$exe = Join-Path $dist 'snipper.exe'
Copy-Item $built $exe -Force
$zip = Join-Path $dist "snipper-$version-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $exe, (Join-Path $root 'README.md') -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
Set-Content -Path "$zip.sha256" -Value "$hash  $(Split-Path -Leaf $zip)" -Encoding ascii

$kb = [math]::Round((Get-Item $exe).Length / 1KB)
Write-Host ''
Write-Host "Snipper $version built" -ForegroundColor Green
Write-Host "  exe : $exe ($kb KB)"
Write-Host "  zip : $zip"
Write-Host "  sha : $hash"
