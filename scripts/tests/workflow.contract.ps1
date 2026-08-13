[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$workflows = @(
    ".github\workflows\quality.yml",
    ".github\workflows\release.yml"
)

foreach ($relativePath in $workflows) {
    $path = Join-Path $repoRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required workflow is missing: $relativePath"
    }
    $content = Get-Content -LiteralPath $path -Raw
    $normalizedContent = $content.Replace("\", "/")
    foreach ($requiredText in @(
        "windows-latest",
        "actions/checkout",
        "actions/setup-python",
        "astral-sh/setup-uv",
        "compute-worker",
        "uv sync --frozen",
        "npm ci"
    )) {
        if ($normalizedContent -notmatch [regex]::Escape($requiredText)) {
            throw "$relativePath is missing required release automation: $requiredText"
        }
    }
    if ($relativePath -eq ".github\workflows\release.yml" -and
        ($normalizedContent -notmatch "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026")) {
        throw "$relativePath must pass the official domain-package private seed to signed release builds"
    }
    if ($normalizedContent -notmatch "scripts/(test|release-check)\.ps1") {
        throw "$relativePath must invoke the deterministic release test entry point"
    }
}

$releaseWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github\workflows\release.yml") -Raw
if ($releaseWorkflow -notmatch 'release-check\.ps1[^\r\n]*-Signed[^\r\n]*-Package') {
    throw ".github\workflows\release.yml must run signed updater release checks"
}
foreach ($requiredSignedEnvironment in @(
    "TAURI_SIGNING_PRIVATE_KEY",
    "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026",
    "BLOOMERY_AUTHENTICODE_PFX_BASE64",
    "BLOOMERY_AUTHENTICODE_PFX_PASSWORD",
    "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL"
)) {
    if ($releaseWorkflow -notmatch [regex]::Escape($requiredSignedEnvironment)) {
        throw ".github\workflows\release.yml must pass $requiredSignedEnvironment to signed release checks"
    }
}
if ($releaseWorkflow -notmatch 'release-check\.ps1[^\r\n]*-Signed[^\r\n]*-RequireSigned[^\r\n]*-RequireTagVersion[^\r\n]*-Package') {
    throw ".github\workflows\release.yml must require signed artifacts and tag/version consistency"
}
if ($releaseWorkflow -notmatch 'github\.ref_type\s*==\s*''tag''|startsWith\(github\.ref,\s*''refs/tags/') {
    throw ".github\workflows\release.yml must restrict signed releases to tag refs"
}

$qualityWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github\workflows\quality.yml") -Raw
$normalizedQualityWorkflow = $qualityWorkflow.Replace("\", "/")
if ($qualityWorkflow -notmatch 'timeout-minutes:\s*(?:9[0-9]|[1-9][0-9]{2,})') {
    throw ".github/workflows/quality.yml must allow the full performance gate to finish within a 90-minute job timeout"
}
foreach ($requiredBenchmark in @(
    "benchmark-retrieval\.ps1",
    "benchmark-dataset-import\.ps1",
    "benchmark-agent-performance\.ps1",
    "benchmark-startup\.ps1"
)) {
    if ($qualityWorkflow -notmatch $requiredBenchmark) {
        throw ".github\workflows\quality.yml must run performance gate: $requiredBenchmark"
    }
}
if ($normalizedQualityWorkflow -notmatch 'scripts/case-study\.ps1' -or
    $normalizedQualityWorkflow -notmatch 'bloomery-steel-case-study-' -or
    $normalizedQualityWorkflow -notmatch 'artifacts/case-study/steel-case-study\.json') {
    throw ".github/workflows/quality.yml must run and retain the reproducible steel case-study report"
}
if ($releaseWorkflow -notmatch 'release-check\.ps1[^\r\n]*-WithE2E[^\r\n]*-Package[^\r\n]*-Performance') {
    throw ".github\workflows\release.yml must run performance gates for unsigned candidates"
}
if ($releaseWorkflow -notmatch 'release-check\.ps1[^\r\n]*-Signed[^\r\n]*-Package[^\r\n]*-Performance') {
    throw ".github\workflows\release.yml must run performance gates for signed candidates"
}
if ($releaseWorkflow -notmatch 'publish-release') {
    throw ".github\workflows\release.yml must define a protected publish job"
}
if ($releaseWorkflow -notmatch 'needs:\s*signed-windows-release') {
    throw "publish job must depend on the signed Windows release job"
}
if ($releaseWorkflow -notmatch 'actions/download-artifact') {
    throw "publish job must download the verified signed artifacts"
}
if ($releaseWorkflow -notmatch 'gh\s+release\s+(create|upload)') {
    throw "publish job must publish artifacts through GitHub Release tooling"
}
if ($releaseWorkflow -notmatch 'contents:\s*write') {
    throw "publish job must declare contents write permission explicitly"
}
if ($releaseWorkflow -notmatch 'verify-tag') {
    throw "publish job must verify the tag before publishing"
}
foreach ($requiredArtifactVerificationText in @(
    "Get-FileHash",
    "ConvertFrom-Json",
    "SHA256SUMS.txt",
    "release-manifest.json",
    "latest.json",
    "GITHUB_REF_NAME",
    "manifest.artifacts",
    "[regex]::Match",
    "Release manifest checksum mismatch"
)) {
    if ($releaseWorkflow -notmatch [regex]::Escape($requiredArtifactVerificationText)) {
        throw "publish job must verify signed artifact metadata and checksums: $requiredArtifactVerificationText"
    }
}

Write-Output "Workflow contract passed."
