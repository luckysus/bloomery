[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$SkipFrontendBuild
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

if (-not (Test-Path -LiteralPath (Join-Path $frontendRoot "package.json") -PathType Leaf)) {
    throw "frontend/package.json is missing"
}
if (-not (Test-Path -LiteralPath (Join-Path $rustRoot "Cargo.toml") -PathType Leaf)) {
    throw "src-tauri/Cargo.toml is missing"
}

Invoke-Checked "Frontend runtime boundaries" "npm" @("run", "test:boundaries") $frontendRoot
if (-not $SkipFrontendBuild) {
    Invoke-Checked "Frontend production build" "npm" @("run", "build") $frontendRoot
}
Invoke-Checked "Rust formatting" "cargo" @("fmt", "--all", "--", "--check") $rustRoot

$cargoCheckArguments = @("check")
if ($Offline) {
    $cargoCheckArguments += "--offline"
}
Invoke-Checked "Rust compile check" "cargo" $cargoCheckArguments $rustRoot

Write-Host "Quality checks passed."
