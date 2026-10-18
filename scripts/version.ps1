# Tag versions override Cargo's development baseline without editing tracked files.
function Get-SnapleafVersion {
    param([string]$Version = $env:SNAPLEAF_VERSION)
    if (-not $Version) {
        $manifest = Get-Content (Join-Path $PSScriptRoot '../Cargo.toml') -Raw
        if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') { throw 'Missing Cargo version' }
        $Version = $Matches[1]
    }
    $Version = $Version -creplace '^v', ''
    if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.([1-9][0-9]*))?$') {
        throw "Invalid release version: $Version (use X.Y.Z or X.Y.Z-rc.N)"
    }
    return $Version
}

# ConvertTo-ReleaseVersion / Test-NewerRelease come from update.ps1 -Library.
function Get-ReleasePlan {
    param([string[]]$TagNames, [string]$Baseline, [string]$Release = "rc", [string]$Bump = "patch")
    $tags = @($TagNames | ForEach-Object {
        if ($_ -cmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.([1-9][0-9]*))?$') {
            $v = ConvertTo-ReleaseVersion $_
            [pscustomobject]@{ Tag = $_; Core = $v.Core; Rc = $v.Rc }
        }
    } | Sort-Object Core,Rc)
    $highest = $tags | Select-Object -Last 1
    $final = $tags | Where-Object { $_.Rc -eq [int]::MaxValue } | Select-Object -Last 1
    $rc = $tags | Where-Object { $_.Rc -ne [int]::MaxValue -and (-not $final -or $_.Core -gt $final.Core) } | Select-Object -Last 1
    $candidateTag = $null
    if ($Release -eq 'promote') {
        if (-not $rc) { throw 'No open release candidate to promote' }
        $version = $rc.Core.ToString(3)
        $candidateTag = $rc.Tag
    } elseif ($Release -eq 'rc') {
        if ($rc) {
            $version = "$($rc.Core.ToString(3))-rc.$($rc.Rc + 1)"
        } elseif (-not $highest) {
            $version = "$Baseline-rc.1"
        } else {
            $base = $highest.Core
            $next = switch ($Bump) {
                major { "$($base.Major + 1).0.0" }
                minor { "$($base.Major).$($base.Minor + 1).0" }
                patch { "$($base.Major).$($base.Minor).$($base.Build + 1)" }
            }
            $version = "$next-rc.1"
        }
    } else {
        $version = Get-SnapleafVersion $Release
        if ($version -notlike '*-rc.*') { throw 'Use promote for final releases so the tested candidate commit ships' }
    }
    if ($highest -and -not (Test-NewerRelease $version $highest.Tag)) { throw 'Version must be higher than every existing release tag' }

    [pscustomobject]@{ Version=$version; Candidate=$candidateTag; Since=$final.Tag }
}
