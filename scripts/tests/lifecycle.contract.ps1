[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$scriptPath = Join-Path $repoRoot "scripts\lifecycle-check.ps1"
$configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
    throw "Lifecycle check script is missing"
}

$content = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
$unicodeInstallPath = -join ([char[]](0x5B89, 0x88C5, 0x8DEF, 0x5F84))
$unicodeDataPath = -join ([char[]](0x7528, 0x6237, 0x6570, 0x636E))
foreach ($requiredText in @(
    "Set-StrictMode -Version Latest",
    "ErrorActionPreference",
    "migrations",
    "InstallerPath",
    "Get-FileHash",
    "tauri.conf.json",
    "identifier",
    "applicationDataDirectory",
    "bloomery.sqlite3",
    "BLOOMERY_DATA_DIR",
    "function Wait-For-ApplicationReady",
    "Process.HasExited",
    "did not stay alive",
    "WaitForExit",
    $unicodeInstallPath,
    $unicodeDataPath
)) {
    if ($content -notmatch [regex]::Escape($requiredText)) {
        throw "Lifecycle check is missing required behavior: $requiredText"
    }
}

if ($content -match 'if \(-not \$AllowUnsigned\)\s*\{\s*throw "-RunInstallerSmoke requires -AllowUnsigned') {
    throw "lifecycle-check.ps1 must allow signed installer smoke tests without -AllowUnsigned"
}

$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
if ([string]$config.mainBinaryName -ne "bloomery") {
    throw "Tauri must bundle bloomery as the main binary"
}

Write-Output "Lifecycle contract passed."
