<# Plan or cut an annotated release tag. A promotion always uses the RC's commit. #>
[CmdletBinding()]
param(
    [ValidateSet('plan','cut')][string]$Action = 'plan',
    [string]$Release = 'rc',
    [ValidateSet('patch','minor','major')][string]$Bump = 'patch',
    [string]$NotesFile,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
. (Join-Path $PSScriptRoot 'version.ps1')
. (Join-Path $PSScriptRoot 'update.ps1') -Library

function Git {
    $result = & git.exe @args
    if ($LASTEXITCODE -ne 0) { throw "git $($args -join ' ') failed" }
    return $result
}
function GhJson {
    $result = & gh @args
    if ($LASTEXITCODE -ne 0) { throw "gh $($args -join ' ') failed" }
    return ($result | ConvertFrom-Json)
}

if ((Git remote get-url origin) -notmatch '^https://github.com/shyoo/snapleaf(\.git)?$|^git@github.com:shyoo/snapleaf(\.git)?$') { throw 'origin must be shyoo/snapleaf' }
if (Git status --porcelain) { throw 'Uncommitted changes: commit or set them aside before releasing' }
Git fetch origin main --tags | Out-Null
$plan = Get-ReleasePlan -TagNames @(Git tag --list 'v*') -Baseline (Get-SnapleafVersion) -Release $Release -Bump $Bump
$version = $plan.Version
$candidateTag = $plan.Candidate
$commit = if ($candidateTag) { Git rev-parse "$candidateTag^{}" } else { Git rev-parse origin/main }
$tag = "v$version"
Git merge-base --is-ancestor $commit origin/main | Out-Null
if ($Action -eq 'plan') {
    [pscustomobject]@{ Version=$version; Commit=$commit; Candidate=$candidateTag; Since=$plan.Since; Prerelease=($version -like '*-*') } | Format-List
    exit 0
}
$main = Git rev-parse --verify main
Git merge-base --is-ancestor $main origin/main | Out-Null
$runs = GhJson api "repos/shyoo/snapleaf/actions/workflows/ci.yml/runs?head_sha=$commit&event=push&per_page=100"
if (-not @($runs.workflow_runs | Where-Object { $_.head_branch -eq 'main' -and $_.conclusion -eq 'success' }).Count) { throw 'CI on origin/main must pass for this commit before cutting a release' }
if ($candidateTag) {
    $candidate = GhJson release view $candidateTag --repo shyoo/snapleaf --json isPrerelease,isDraft,assets
    if (-not $candidate.isPrerelease -or $candidate.isDraft) { throw 'Candidate must be a published prerelease' }
    $expected = "snapleaf-$($candidateTag.Substring(1))-x64-setup.exe"
    if ($expected -cnotin $candidate.assets.name -or 'SHA256SUMS.txt' -cnotin $candidate.assets.name) { throw 'Candidate installer and checksums must be published before promotion' }
}
if (-not $NotesFile -or -not (Test-Path -LiteralPath $NotesFile)) { throw 'Pass -NotesFile with the user-facing release notes' }
if (-not (Get-Content -LiteralPath $NotesFile -Raw).Trim()) { throw 'Release notes cannot be empty' }
Write-Host "$tag -> $commit (candidate: $candidateTag)"
Get-Content -LiteralPath $NotesFile
if ($DryRun) { Write-Host 'Dry run: all gates passed; no tag created'; exit 0 }
Git tag -a $tag $commit -F $NotesFile | Out-Null
Git push origin "refs/tags/$tag" | Out-Null
Write-Host "https://github.com/shyoo/snapleaf/releases/tag/$tag"
