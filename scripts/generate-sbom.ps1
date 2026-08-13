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
$workerRoot = Join-Path $repoRoot "compute-worker"
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

function Write-PythonWorkerSbom {
    param(
        [Parameter(Mandatory = $true)][string]$LockPath,
        [Parameter(Mandatory = $true)][string]$ProjectPath,
        [Parameter(Mandatory = $true)][string]$OutputPath
    )

    $projectText = Get-Content -LiteralPath $ProjectPath -Raw
    $projectMatch = [regex]::Match(
        $projectText,
        '(?ms)^\[project\]\s*(?<project>.*?)(?=^\[|\z)'
    )
    if (-not $projectMatch.Success) {
        throw "Worker pyproject.toml is missing a [project] table"
    }
    $versionMatch = [regex]::Match(
        $projectMatch.Groups["project"].Value,
        '(?m)^version\s*=\s*"(?<version>[^"]+)"\s*$'
    )
    if (-not $versionMatch.Success -or
        [string]::IsNullOrWhiteSpace($versionMatch.Groups["version"].Value)) {
        throw "Worker pyproject.toml is missing a project version"
    }
    $workerProjectVersion = $versionMatch.Groups["version"].Value

    $components = New-Object System.Collections.Generic.List[object]
    $current = $null
    function Add-CurrentPackage {
        if ($null -eq $script:currentPackage) {
            return
        }
        if ([string]::IsNullOrWhiteSpace($script:currentPackage.Name) -or
            [string]::IsNullOrWhiteSpace($script:currentPackage.Version)) {
            throw "uv.lock contains a package without a name or version"
        }
        $normalizedName = $script:currentPackage.Name.ToLowerInvariant().Replace("_", "-")
        $component = [ordered]@{
            type    = if ($script:currentPackage.Source -eq "editable") { "application" } else { "library" }
            name    = $script:currentPackage.Name
            version = $script:currentPackage.Version
            purl    = "pkg:pypi/$normalizedName@$($script:currentPackage.Version)"
        }
        $hashes = @($script:currentPackage.Hashes | Sort-Object -Unique)
        if ($hashes.Count -gt 0) {
            $component.hashes = @($hashes | ForEach-Object {
                [ordered]@{
                    alg     = "SHA-256"
                    content = $_.ToLowerInvariant()
                }
            })
        }
        if (-not [string]::IsNullOrWhiteSpace($script:currentPackage.Source)) {
            $component.properties = @(
                [ordered]@{
                    name  = "bloomery.uv.source"
                    value = $script:currentPackage.Source
                }
            )
        }
        $components.Add($component) | Out-Null
    }

    $script:currentPackage = $null
    foreach ($line in Get-Content -LiteralPath $LockPath) {
        if ($line -match '^\[\[package\]\]') {
            Add-CurrentPackage
            $script:currentPackage = [pscustomobject]@{
                Name    = $null
                Version = $null
                Source  = $null
                Hashes  = New-Object System.Collections.Generic.List[string]
            }
            continue
        }
        if ($null -eq $script:currentPackage) {
            continue
        }
        if ($line -match '^name\s*=\s*"([^"]+)"') {
            $script:currentPackage.Name = $Matches[1]
        }
        elseif ($line -match '^version\s*=\s*"([^"]+)"') {
            $script:currentPackage.Version = $Matches[1]
        }
        elseif ($line -match '^source\s*=\s*\{\s*registry\s*=\s*"([^"]+)"') {
            $script:currentPackage.Source = $Matches[1]
        }
        elseif ($line -match '^source\s*=\s*\{\s*editable\s*=') {
            $script:currentPackage.Source = "editable"
        }
        foreach ($match in [regex]::Matches($line, 'hash\s*=\s*"sha256:([0-9a-fA-F]{64})"')) {
            $script:currentPackage.Hashes.Add($match.Groups[1].Value) | Out-Null
        }
    }
    Add-CurrentPackage
    $script:currentPackage = $null

    if ($components.Count -eq 0) {
        throw "uv.lock did not contain any Python Worker packages"
    }
    $metadataComponent = [ordered]@{
        type    = "application"
        name    = "bloomery-compute-worker"
        version = $workerProjectVersion
        purl    = "pkg:pypi/bloomery-compute-worker@$workerProjectVersion"
    }
    $bom = [ordered]@{
        '$schema'    = "http://cyclonedx.org/schema/bom-1.5.schema.json"
        bomFormat    = "CycloneDX"
        specVersion  = "1.5"
        version      = 1
        metadata     = [ordered]@{
            tools     = @(
                [ordered]@{
                    vendor = "Bloomery"
                    name   = "generate-sbom.ps1"
                }
            )
            component = $metadataComponent
        }
        components   = $components.ToArray()
    }
    [System.IO.File]::WriteAllText(
        $OutputPath,
        ($bom | ConvertTo-Json -Depth 12),
        [System.Text.UTF8Encoding]::new($false)
    )
}

foreach ($requiredPath in @(
    (Join-Path $rustRoot "Cargo.toml"),
    (Join-Path $rustRoot "Cargo.lock"),
    (Join-Path $rustRoot "about.toml"),
    (Join-Path $rustRoot "THIRD_PARTY_NOTICES.hbs"),
    (Join-Path $frontendRoot "package.json"),
    (Join-Path $frontendRoot "package-lock.json"),
    (Join-Path $workerRoot "pyproject.toml"),
    (Join-Path $workerRoot "uv.lock")
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
$pythonWorkerCyclonePath = Join-Path $outputPath "bloomery-python-worker-sbom.cdx.json"
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

    Write-PythonWorkerSbom `
        -LockPath (Join-Path $workerRoot "uv.lock") `
        -ProjectPath (Join-Path $workerRoot "pyproject.toml") `
        -OutputPath $pythonWorkerCyclonePath

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

    foreach ($jsonPath in @($rustOutputPath, $frontendCyclonePath, $frontendSpdxPath, $pythonWorkerCyclonePath)) {
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
