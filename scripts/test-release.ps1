# Pure version/URL/checksum tests and an offline end-to-end updater fixture.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'version.ps1')
. (Join-Path $PSScriptRoot 'update.ps1') -Library
function Assert($Condition, $Message) { if (-not $Condition) { throw $Message } }
function Refuses([scriptblock]$Action) {
    $refused = $false
    try { & $Action | Out-Null } catch { $refused = $true }
    Assert $refused 'Expected refusal'
}
Assert ((Get-SnapleafVersion 'v1.2.3-rc.4') -ceq '1.2.3-rc.4') 'Tag version'
foreach ($invalid in @('1.2', '1.02.3', '1.2.3-rc.0', '1.2.3-beta', '1.2.3;evil')) { Refuses { Get-SnapleafVersion $invalid } }
Assert (Test-NewerRelease '1.10.0' '1.9.9') 'Numeric version comparison'
Assert (Test-NewerRelease '1.2.3' '1.2.3-rc.4') 'Final upgrades same-core RC'
Assert (-not (Test-NewerRelease '1.2.3-rc.4' '1.2.3')) 'RC does not replace final'
Assert (-not (Test-NewerRelease '1.2.2' '1.2.3')) 'No downgrade'
Assert ((Get-ReleasePlan -TagNames @() -Baseline '0.1.0').Version -ceq '0.1.0-rc.1') 'First candidate uses baseline'
Assert ((Get-ReleasePlan -TagNames @('v0.1.0') -Baseline '0.1.0').Version -ceq '0.1.1-rc.1') 'Default patch bump'
Assert ((Get-ReleasePlan -TagNames @('v0.1.0') -Baseline '0.1.0' -Bump minor).Version -ceq '0.2.0-rc.1') 'Minor bump'
Assert ((Get-ReleasePlan -TagNames @('v0.1.0') -Baseline '0.1.0' -Bump major).Version -ceq '1.0.0-rc.1') 'Major bump'
$tags = @('v0.1.0', 'v0.2.0-rc.2', 'v0.2.0-rc.10')
Assert ((Get-ReleasePlan -TagNames $tags -Baseline '0.1.0').Version -ceq '0.2.0-rc.11') 'Numeric RC increment'
$promotion = Get-ReleasePlan -TagNames $tags -Baseline '0.1.0' -Release promote
Assert ($promotion.Version -ceq '0.2.0' -and $promotion.Candidate -ceq 'v0.2.0-rc.10') 'Promote highest open candidate'
Refuses { Get-ReleasePlan -TagNames @('v0.1.0') -Baseline '0.1.0' -Release promote }
Refuses { Get-ReleasePlan -TagNames $tags -Baseline '0.1.0' -Release '0.2.0' }
Refuses { Get-ReleasePlan -TagNames $tags -Baseline '0.1.0' -Release '0.2.0-rc.1' }
$hash = 'a' * 64
Assert ((Get-AssetChecksum "$hash  a.exe" 'a.exe') -ceq $hash) 'Checksum lookup'
Assert ((Get-AssetChecksum "$hash *a.exe" 'a.exe') -ceq $hash) 'Binary checksum lookup'
Refuses { Get-AssetChecksum "$hash  other.exe" 'a.exe' }
Refuses { Get-AssetChecksum "$hash  a.exe`n$hash  a.exe" 'a.exe' }
foreach ($url in @('http://github.com/a','https://github.com.evil/a','https://evil.com/a','https://github.com:444/a', 'https://user@github.com/a')) {
    Assert (-not (Test-DownloadUri ([uri]$url))) "Reject $url"
}
Assert (Test-DownloadUri ([uri]'https://release-assets.githubusercontent.com/a')) 'GitHub asset CDN'

$temp = Join-Path (Split-Path -Parent $PSScriptRoot) "target/update-test/$([guid]::NewGuid())"
[IO.Directory]::CreateDirectory($temp) | Out-Null
$oldLocal = $env:LOCALAPPDATA
$oldCurrent = $env:SNAPLEAF_CURRENT_VERSION
$oldOpen = $env:SNAPLEAF_UPDATE_OPEN
$oldHash = $env:SNAPLEAF_UPDATE_HASH
try {
    $env:LOCALAPPDATA = $temp
    $env:SNAPLEAF_CURRENT_VERSION = '0.1.0'
    $env:SNAPLEAF_UPDATE_OPEN = ''
    $script:fixtureVersion = '0.2.0'
    $script:fixtureBadHash = $false
    $script:fixturePrerelease = $false
    $script:installerRequests = 0
    $payload = Join-Path $temp 'payload'
    [IO.File]::WriteAllText($payload, 'fixture installer bytes')
    $script:payloadHash = (Get-FileHash $payload -Algorithm SHA256).Hash.ToLowerInvariant()
    function Receive-ReleaseFile([string]$Url, [string]$Destination, [long]$Limit) {
        $name = "snapleaf-$script:fixtureVersion-x64-setup.exe"
        $prefix = "https://github.com/shyoo/snapleaf/releases/download/v$script:fixtureVersion/"
        if ($Url -like '*/releases/latest') {
            @{ tag_name="v$script:fixtureVersion"; prerelease=$script:fixturePrerelease; draft=$false; assets=@(
                @{name=$name;browser_download_url="$prefix$name"},
                @{name='SHA256SUMS.txt';browser_download_url="${prefix}SHA256SUMS.txt"}
            ) } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $Destination
        } elseif ($Url -like '*/SHA256SUMS.txt') {
            $expected = if ($script:fixtureBadHash) { 'b' * 64 } else { $script:payloadHash }
            "$expected  $name" | Set-Content -LiteralPath $Destination
        } else {
            $script:installerRequests++
            Copy-Item -LiteralPath $payload -Destination $Destination
        }
    }
    $result = Invoke-SnapleafUpdate
    Assert ($result -like 'ready*') 'Downloads and verifies update'
    $destination = Join-Path $temp 'Snapleaf/updates/snapleaf-0.2.0-x64-setup.exe'
    Assert ((Get-FileHash $destination).Hash -ieq $script:payloadHash) 'Verified bytes stored'
    Invoke-SnapleafUpdate | Out-Null
    Assert ($script:installerRequests -eq 1) 'Reuses verified download'
    $script:fixtureVersion = '0.3.0'
    $script:fixtureBadHash = $true
    Refuses { Invoke-SnapleafUpdate }
    Assert (-not (Test-Path (Join-Path $temp 'Snapleaf/updates/snapleaf-0.3.0-x64-setup.exe'))) 'Bad checksum never installed'
    $script:fixturePrerelease = $true
    Refuses { Invoke-SnapleafUpdate }
    $script:fixturePrerelease = $false
    $script:fixtureVersion = '0.1.0'
    Assert ((Invoke-SnapleafUpdate) -ceq 'current') 'Current release does not download'
    $env:SNAPLEAF_UPDATE_OPEN = $destination
    $env:SNAPLEAF_UPDATE_HASH = 'c' * 64
    Refuses { Invoke-SnapleafUpdate }
} finally {
    $env:LOCALAPPDATA = $oldLocal
    $env:SNAPLEAF_CURRENT_VERSION = $oldCurrent
    $env:SNAPLEAF_UPDATE_OPEN = $oldOpen
    $env:SNAPLEAF_UPDATE_HASH = $oldHash
    if ($script:client) { $script:client.Dispose() }
}
Write-Host 'Release and update tests passed'
