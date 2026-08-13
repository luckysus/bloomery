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

$unicodeInstallPathExpression = '$unicodeInstallDirectoryName = -join ([char[]](0x5B89, 0x88C5, 0x8DEF, 0x5F84))'
$unicodeDataPathExpression = '$unicodeDataDirectoryName = -join ([char[]](0x7528, 0x6237, 0x6570, 0x636E))'
if ($content -notmatch [regex]::Escape($unicodeInstallPathExpression)) {
    throw "lifecycle-matrix.ps1 must construct the Unicode install path from code points"
}
if ($content -notmatch [regex]::Escape($unicodeDataPathExpression)) {
    throw "lifecycle-matrix.ps1 must construct the Unicode data path from code points"
}

Write-Output "Lifecycle matrix contract passed."
