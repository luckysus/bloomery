[CmdletBinding()]
param(
    [switch]$Offline,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$rustRoot = Join-Path $repoRoot "src-tauri"
$frontendRoot = Join-Path $repoRoot "frontend"
$outputPath = [System.IO.Path]::GetFullPath($OutputDirectory)

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

foreach ($requiredPath in @(
    (Join-Path $rustRoot "Cargo.toml"),
    (Join-Path $rustRoot "Cargo.lock"),
    (Join-Path $rustRoot "about.toml"),
    (Join-Path $rustRoot "THIRD_PARTY_NOTICES.hbs"),
    (Join-Path $frontendRoot "package.json"),
    (Join-Path $frontendRoot "package-lock.json")
)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "SBOM input is missing: $requiredPath"
    }
}

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null

$rustTempName = ".bloomery-rust-sbom-" + [Guid]::NewGuid().ToString("N")
$rustTempPath = Join-Path $rustRoot ($rustTempName + ".json")
$rustOutputPath = Join-Path $outputPath "bloomery-rust-sbom.cdx.json"
$frontendCyclonePath = Join-Path $outputPath "bloomery-frontend-sbom.cdx.json"
$frontendSpdxPath = Join-Path $outputPath "bloomery-frontend-sbom.spdx.json"
$noticesPath = Join-Path $outputPath "THIRD_PARTY_NOTICES.txt"
$npmErrorPath = Join-Path $env:TEMP ("bloomery-npm-sbom-" + [Guid]::NewGuid().ToString("N") + ".log")

try {
    $cargoArguments = @(
        "cyclonedx",
        "--quiet",
        "--format", "json",
        "--spec-version", "1.5",
        "--override-filename", $rustTempName
    )
    Invoke-Checked "Rust CycloneDX SBOM" "cargo" $cargoArguments $rustRoot
    if (-not (Test-Path -LiteralPath $rustTempPath -PathType Leaf)) {
        throw "cargo-cyclonedx did not create $rustTempPath"
    }
    Copy-Item -LiteralPath $rustTempPath -Destination $rustOutputPath -Force

    Push-Location $frontendRoot
    try {
        $npmCycloneOutput = @(& npm sbom --package-lock-only --sbom-format cyclonedx --sbom-type application 2> $npmErrorPath)
        $npmExitCode = $LASTEXITCODE
        if ($npmExitCode -ne 0) {
            $details = if (Test-Path -LiteralPath $npmErrorPath) { Get-Content -Raw $npmErrorPath } else { "" }
            throw ("Frontend CycloneDX SBOM failed with exit code {0}. {1}" -f $npmExitCode, $details.Trim())
        }
        [System.IO.File]::WriteAllText($frontendCyclonePath, ($npmCycloneOutput -join [Environment]::NewLine), [Text.UTF8Encoding]::new($false))

        $npmSpdxOutput = @(& npm sbom --package-lock-only --sbom-format spdx --sbom-type application 2> $npmErrorPath)
        $npmExitCode = $LASTEXITCODE
        if ($npmExitCode -ne 0) {
            $details = if (Test-Path -LiteralPath $npmErrorPath) { Get-Content -Raw $npmErrorPath } else { "" }
            throw ("Frontend SPDX SBOM failed with exit code {0}. {1}" -f $npmExitCode, $details.Trim())
        }
        [System.IO.File]::WriteAllText($frontendSpdxPath, ($npmSpdxOutput -join [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    }
    finally {
        Pop-Location
    }

    $aboutArguments = @(
        "about",
        "generate",
        "THIRD_PARTY_NOTICES.hbs",
        "--output-file", $noticesPath,
        "--locked",
        "--fail"
    )
    if ($Offline) {
        $aboutArguments += "--offline"
    }
    Invoke-Checked "Third-party license notices" "cargo" $aboutArguments $rustRoot

    foreach ($jsonPath in @($rustOutputPath, $frontendCyclonePath, $frontendSpdxPath)) {
        $null = Get-Content -LiteralPath $jsonPath -Raw | ConvertFrom-Json
    }
    if ((Get-Item -LiteralPath $noticesPath).Length -eq 0) {
        throw "Generated third-party notices are empty"
    }

    Write-Host ("SBOM and third-party notices written to " + $outputPath)
}
finally {
    Remove-Item -LiteralPath $rustTempPath, $npmErrorPath -Force -ErrorAction SilentlyContinue
}
