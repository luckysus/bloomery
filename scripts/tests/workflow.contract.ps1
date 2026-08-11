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
    if ($normalizedContent -notmatch "scripts/(test|release-check)\.ps1") {
        throw "$relativePath must invoke the deterministic release test entry point"
    }
}

Write-Output "Workflow contract passed."
