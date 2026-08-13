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
if ($releaseWorkflow -notmatch 'release-check\.ps1 -Signed -Package') {
    throw ".github\workflows\release.yml must run signed updater release checks"
}

$qualityWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github\workflows\quality.yml") -Raw
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
if ($releaseWorkflow -notmatch 'release-check\.ps1 -WithE2E -Package -Performance') {
    throw ".github\workflows\release.yml must run performance gates for unsigned candidates"
}
if ($releaseWorkflow -notmatch 'release-check\.ps1 -Signed -Package -Performance') {
    throw ".github\workflows\release.yml must run performance gates for signed candidates"
}

Write-Output "Workflow contract passed."
