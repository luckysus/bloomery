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

$content = Get-Content -LiteralPath $scriptPath -Raw
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
    "Wait-ForPath",
    "WaitForExit",
    "安装路径",
    "用户数据"
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
