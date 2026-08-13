$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$scriptPath = Join-Path $repoRoot "scripts\lifecycle-matrix.ps1"
if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
    throw "lifecycle-matrix.ps1 is missing"
}

$content = Get-Content -LiteralPath $scriptPath -Raw
foreach ($requiredText in @(
    "[switch]`$RunInstallerSmoke",
    "[switch]`$RunUpgradeDowngrade",
    "BLOOMERY_DATA_DIR",
    "upgrade",
    "downgrade",
    "results.Add((Install-And-Launch",
    "retention-sentinel.txt",
    "function Wait-For-ApplicationReady",
    "Process.HasExited",
    "did not stay alive",
    "requires distinct product versions",
    "Unicode",
    "data-preservation"
)) {
    if ($content -notmatch [regex]::Escape($requiredText)) {
        throw "lifecycle-matrix.ps1 is missing required behavior: $requiredText"
    }
}

Write-Output "Lifecycle matrix contract passed."
