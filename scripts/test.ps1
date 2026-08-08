[CmdletBinding()]
param(
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    Write-Host ("==> " + $Name)
    Push-Location $WorkingDirectory
    try {
        & $File @Arguments
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw ("{0} failed with exit code {1}" -f $Name, $exitCode)
        }
    }
    finally {
        Pop-Location
    }
}

$frontendRoot = Join-Path $repoRoot "frontend"
$rustRoot = Join-Path $repoRoot "src-tauri"
$scriptContract = Join-Path $repoRoot "scripts\tests\release-scripts.contract.ps1"
$lifecycleContract = Join-Path $repoRoot "scripts\tests\lifecycle.contract.ps1"

Invoke-Checked "Release script contracts" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $scriptContract) $repoRoot
Invoke-Checked "Lifecycle script contracts" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $lifecycleContract) $repoRoot

Invoke-Checked "Frontend unit and integration tests" "npm" @("run", "test") $frontendRoot
Invoke-Checked "Frontend runtime boundaries" "npm" @("run", "test:boundaries") $frontendRoot
Invoke-Checked "Frontend production build" "npm" @("run", "build") $frontendRoot
Invoke-Checked "Rust formatting" "cargo" @("fmt", "--all", "--", "--check") $rustRoot

$cargoTestArguments = @("test")
if ($Offline) {
    $cargoTestArguments += "--offline"
}
Invoke-Checked "Rust test suite" "cargo" $cargoTestArguments $rustRoot

Write-Host "Deterministic test suite passed."
