[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

function Assert-NotContains {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern
    )

    $content = Get-Content -LiteralPath $Path -Raw
    if ($content -match [regex]::Escape($Pattern)) {
        throw "$Path must not contain deferred public update configuration: $Pattern"
    }
}

$configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$capabilityPath = Join-Path $repoRoot "src-tauri\capabilities\default.json"
$cargoPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$appPath = Join-Path $repoRoot "src-tauri\src\app\mod.rs"
$bridgePath = Join-Path $repoRoot "frontend\src\bridge\desktop.ts"
$settingsPath = Join-Path $repoRoot "frontend\src\features\settings\SettingsPage.tsx"
$testScriptPath = Join-Path $repoRoot "scripts\test.ps1"

foreach ($requiredPath in @($configPath, $capabilityPath, $cargoPath, $appPath, $bridgePath, $settingsPath, $testScriptPath)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required current-version update boundary is missing: $requiredPath"
    }
}

Assert-NotContains $configPath "github.com/luckysus/bloomery/releases"
Assert-NotContains $configPath '"updater"'
Assert-NotContains $capabilityPath '"process:allow-restart"'
Assert-NotContains $capabilityPath '"updater:default"'
Assert-NotContains $cargoPath "tauri-plugin-updater"
Assert-NotContains $appPath "tauri_plugin_updater"
Assert-NotContains $bridgePath "@tauri-apps/plugin-updater"
Assert-NotContains $settingsPath "UpdatePanel"
Assert-NotContains $testScriptPath 'Invoke-Checked "Updater contract"'

$updatePanelPath = Join-Path $repoRoot "frontend\src\features\settings\UpdatePanel.tsx"
if (Test-Path -LiteralPath $updatePanelPath) {
    throw "The deferred update panel must not ship in the current no-release build"
}

Write-Output "Update-channel contract passed."
