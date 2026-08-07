[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$SkipTests,
    [ValidateSet("msi", "nsis", "all")][string]$Bundles = "all",
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$rustRoot = Join-Path $repoRoot "src-tauri"
$tauriConfigPath = Join-Path $rustRoot "tauri.conf.json"

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

if (-not (Test-Path -LiteralPath $tauriConfigPath -PathType Leaf)) {
    throw "src-tauri/tauri.conf.json is missing"
}

$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw | ConvertFrom-Json
$version = [string]$tauriConfig.version
if (-not $version) {
    throw "Tauri version is missing"
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot ("artifacts\\Bloomery-" + $version)
}
$outputPath = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $outputPath) {
    throw "Release output already exists: $outputPath. Choose a new -OutputDirectory."
}

if (-not $SkipTests) {
    $testScript = Join-Path $PSScriptRoot "test.ps1"
    $testArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $testScript)
    if ($Offline) {
        $testArguments += "-Offline"
    }
    Invoke-Checked "Deterministic test suite" "powershell" $testArguments $repoRoot
}

$bundleValue = if ($Bundles -eq "all") { "msi,nsis" } else { $Bundles }
$buildArguments = @("tauri", "build", "--ci", "--no-sign", "--bundles", $bundleValue)
if ($Offline) {
    $buildArguments = @("--offline") + $buildArguments
}
Invoke-Checked "Unsigned Windows package build" "cargo" $buildArguments $rustRoot

$bundleRoot = Join-Path $rustRoot "target\\release\\bundle"
$releaseArtifacts = Get-ChildItem -LiteralPath $bundleRoot -Recurse -File | Where-Object {
    $_.Extension -in @(".exe", ".msi")
} | Sort-Object Name
if ($releaseArtifacts.Count -eq 0) {
    throw "Tauri completed without producing an MSI or NSIS artifact"
}

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
foreach ($artifact in $releaseArtifacts) {
    Copy-Item -LiteralPath $artifact.FullName -Destination (Join-Path $outputPath $artifact.Name) -ErrorAction Stop
}

$commit = git -C $repoRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0) {
    throw "Unable to determine the release commit"
}
$manifestArtifacts = foreach ($artifact in (Get-ChildItem -LiteralPath $outputPath -File | Sort-Object Name)) {
    $hash = Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256
    [ordered]@{
        name = $artifact.Name
        bytes = $artifact.Length
        sha256 = $hash.Hash.ToLowerInvariant()
    }
}
$manifest = [ordered]@{
    product = [string]$tauriConfig.productName
    version = $version
    commit = $commit.Trim()
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    signing = "unsigned"
    artifacts = @($manifestArtifacts)
}
$manifestPath = Join-Path $outputPath "release-manifest.json"
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 5), [System.Text.UTF8Encoding]::new($false))

$checksumScript = Join-Path $PSScriptRoot "generate-checksums.ps1"
Invoke-Checked "Artifact checksum generation" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $checksumScript, "-InputPath", $outputPath) $repoRoot

Write-Host ("Unsigned release artifacts written to " + $outputPath)
