[CmdletBinding()]
param(
    [switch]$Offline,
    [ValidateSet("all", "contracts", "frontend", "rust")]
    [string]$Stage = "all"
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
$lifecycleMatrixContract = Join-Path $repoRoot "scripts\tests\lifecycle-matrix.contract.ps1"
$updaterContract = Join-Path $repoRoot "scripts\tests\updater.contract.ps1"
$workflowContract = Join-Path $repoRoot "scripts\tests\workflow.contract.ps1"
$runContracts = $Stage -eq "all" -or $Stage -eq "contracts"
$runFrontend = $Stage -eq "all" -or $Stage -eq "frontend"
$runRust = $Stage -eq "all" -or $Stage -eq "rust"

if ($runContracts) {
    Invoke-Checked "Release script contracts" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $scriptContract) $repoRoot
    Invoke-Checked "Lifecycle script contracts" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $lifecycleContract) $repoRoot
    Invoke-Checked "Lifecycle matrix contract" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $lifecycleMatrixContract) $repoRoot
    Invoke-Checked "Updater contract" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $updaterContract) $repoRoot
    Invoke-Checked "Workflow contract" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $workflowContract) $repoRoot
}

if ($runFrontend) {
    Invoke-Checked "Frontend unit and integration tests" "npm" @("run", "test") $frontendRoot
    Invoke-Checked "Frontend runtime boundaries" "npm" @("run", "test:boundaries") $frontendRoot
    Invoke-Checked "Frontend production build" "npm" @("run", "build") $frontendRoot
}

if ($runRust) {
    Invoke-Checked "Rust formatting" "cargo" @("fmt", "--all", "--", "--check") $rustRoot

    $cargoTestArguments = @("test", "--jobs", "1")
    if ($Offline) {
        $cargoTestArguments += "--offline"
    }
    $cargoTestArguments += @("--", "--test-threads=1")
    Invoke-Checked "Rust test suite" "cargo" $cargoTestArguments $rustRoot
}

Write-Host ("Deterministic test suite passed (stage: " + $Stage + ").")
