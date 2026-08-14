[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$lockPath = Join-Path $repoRoot "src-tauri\Cargo.lock"
if (-not (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
    throw "Required lockfile is missing: $lockPath"
}

$lockContent = Get-Content -LiteralPath $lockPath -Raw
$eventListenerMatch = [regex]::Match(
    $lockContent,
    '(?ms)\[\[package\]\]\s*name = "event-listener"\s*version = "([^"]+)"'
)
if (-not $eventListenerMatch.Success) {
    throw "Cargo.lock must contain event-listener"
}

$minimumVersion = [Version]"5.4.2"
$resolvedVersion = [Version]$eventListenerMatch.Groups[1].Value
if ($resolvedVersion -lt $minimumVersion) {
    throw "event-listener $resolvedVersion is below the security floor $minimumVersion (RUSTSEC-2026-0221)"
}

Write-Output "Dependency contract passed: event-listener $resolvedVersion."
