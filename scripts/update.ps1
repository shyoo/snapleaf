# Embedded in snapleaf.exe; runs in a hidden worker process, never on the UI thread.
param([switch]$Library)
$ErrorActionPreference = 'Stop'

function ConvertTo-ReleaseVersion([string]$Text) {
    if ($Text -cnotmatch '^v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.([1-9][0-9]*))?$') { throw 'Invalid version' }
    return @{ Core = [version]("$($Matches[1]).$($Matches[2]).$($Matches[3])"); Rc = $(if ($Matches[4]) { [int]$Matches[5] } else { [int]::MaxValue }) }
}

function Test-NewerRelease([string]$Candidate, [string]$Current) {
    $a = ConvertTo-ReleaseVersion $Candidate
    $b = ConvertTo-ReleaseVersion $Current
    return ($a.Core -gt $b.Core -or ($a.Core -eq $b.Core -and $a.Rc -gt $b.Rc))
}

function Get-AssetChecksum([string]$Text, [string]$Name) {
    $found = @($Text -split '\r?\n' | Where-Object { $_ -match '^([a-fA-F0-9]{64})\s+\*?(.+)$' -and $Matches[2] -ceq $Name } | ForEach-Object { ($_ -split '\s+')[0].ToLowerInvariant() })
    if ($found.Count -ne 1) { throw "Missing or ambiguous checksum for $Name" }
    return $found[0]
}

function Test-DownloadUri([uri]$Uri) {
    return ($Uri.Scheme -eq 'https' -and $Uri.IsDefaultPort -and -not $Uri.UserInfo -and
        ($Uri.Host -eq 'api.github.com' -or $Uri.Host -eq 'github.com' -or $Uri.Host.EndsWith('.githubusercontent.com')))
}

function Receive-ReleaseFile([string]$Url, [string]$Destination, [long]$Limit) {
    $uri = [uri]$Url
    for ($redirect = 0; $redirect -lt 6; $redirect++) {
        if (-not (Test-DownloadUri $uri)) { throw 'Untrusted update URL' }
        $response = $script:client.GetAsync($uri, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        try {
            if ([int]$response.StatusCode -in 301,302,303,307,308) {
                $uri = [uri]::new($uri, $response.Headers.Location)
                continue
            }
            $response.EnsureSuccessStatusCode() | Out-Null
            if ($response.Content.Headers.ContentLength -gt $Limit) { throw 'Update response is too large' }
            $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $outputStream = [IO.File]::Create($Destination)
            try {
                $buffer = New-Object byte[] 65536
                $total = 0L
                $deadline = [DateTime]::UtcNow.AddMinutes(3)
                while ($true) {
                    $read = $inputStream.ReadAsync($buffer, 0, $buffer.Length)
                    if (-not $read.Wait(30000)) { throw 'Update download timed out' }
                    $count = $read.GetAwaiter().GetResult()
                    if ($count -eq 0) { break }
                    $total += $count
                    if ($total -gt $Limit -or [DateTime]::UtcNow -gt $deadline) { throw 'Update download exceeded its limit' }
                    $outputStream.Write($buffer, 0, $count)
                }
            } finally { $outputStream.Dispose(); $inputStream.Dispose() }
            return
        } finally { $response.Dispose() }
    }
    throw 'Too many update redirects'
}

function Invoke-SnapleafUpdate {
    if ($env:SNAPLEAF_UPDATE_OPEN) {
        $path = $env:SNAPLEAF_UPDATE_OPEN
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $env:SNAPLEAF_UPDATE_HASH) { throw 'Installer checksum changed; check for updates again' }
        Start-Process -FilePath $path | Out-Null
        Write-Output 'opened'
        return
    }
    Add-Type -AssemblyName System.Net.Http
    $handler = New-Object System.Net.Http.HttpClientHandler
    $handler.AllowAutoRedirect = $false
    $script:client = New-Object System.Net.Http.HttpClient($handler)
    $script:client.Timeout = [TimeSpan]::FromSeconds(30)
    $script:client.DefaultRequestHeaders.UserAgent.ParseAdd('Snapleaf')
    $dir = Join-Path $env:LOCALAPPDATA 'Snapleaf/updates'
    [IO.Directory]::CreateDirectory($dir) | Out-Null
    $metadata = Join-Path $dir 'release.json.part'
    $checksums = Join-Path $dir 'SHA256SUMS.txt.part'
    try {
        Receive-ReleaseFile 'https://api.github.com/repos/shyoo/snapleaf/releases/latest' $metadata 1048576
    } catch {
        if ($_.Exception.ToString() -match '\b404\b') { Write-Output 'unpublished'; return }
        throw
    }
    $release = Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json
    if ($release.draft -or $release.prerelease -or $release.tag_name -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Expected a stable release' }
    if (-not (Test-NewerRelease $release.tag_name $env:SNAPLEAF_CURRENT_VERSION)) { Write-Output 'current'; return }
    $name = "snapleaf-$($release.tag_name.Substring(1))-x64-setup.exe"
    $asset = @($release.assets | Where-Object { $_.name -ceq $name })
    $sums = @($release.assets | Where-Object { $_.name -ceq 'SHA256SUMS.txt' })
    if ($asset.Count -ne 1 -or $sums.Count -ne 1) { throw 'Release installer or checksums missing' }
    $prefix = "https://github.com/shyoo/snapleaf/releases/download/$($release.tag_name)/"
    if ($asset[0].browser_download_url -cne "$prefix$name" -or $sums[0].browser_download_url -cne "${prefix}SHA256SUMS.txt") { throw 'Unexpected release asset URL' }
    Receive-ReleaseFile $sums[0].browser_download_url $checksums 65536
    $hash = Get-AssetChecksum (Get-Content -LiteralPath $checksums -Raw) $name
    $destination = Join-Path $dir $name
    if (-not (Test-Path -LiteralPath $destination) -or (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ine $hash) {
        $partial = "$destination.part"
        Receive-ReleaseFile $asset[0].browser_download_url $partial 104857600
        if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ine $hash) { Remove-Item -LiteralPath $partial; throw 'Installer checksum mismatch' }
        Move-Item -LiteralPath $partial -Destination $destination -Force
    }
    Write-Output "ready`t$destination`t$hash"
}
if ($Library) { return }
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
    Invoke-SnapleafUpdate
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if ($script:client) { $script:client.Dispose() }
}
