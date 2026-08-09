[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$cargoPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$packagePath = Join-Path $repoRoot "frontend\package.json"
$buildPath = Join-Path $repoRoot "scripts\build-release.ps1"
$workflowPath = Join-Path $repoRoot ".github\workflows\release.yml"
$configScript = Join-Path $repoRoot "scripts\write-updater-config.ps1"
$manifestScript = Join-Path $repoRoot "scripts\generate-updater-manifest.ps1"

foreach ($path in @($configPath, $cargoPath, $packagePath, $buildPath, $workflowPath, $configScript, $manifestScript)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Updater release file is missing: $path"
    }
}

$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$cargo = Get-Content -LiteralPath $cargoPath -Raw
$package = Get-Content -LiteralPath $packagePath -Raw | ConvertFrom-Json
$build = Get-Content -LiteralPath $buildPath -Raw
$workflow = Get-Content -LiteralPath $workflowPath -Raw

if (-not $config.plugins.updater) {
    throw "Tauri updater plugin configuration is missing"
}
if ($cargo -notmatch '(?m)^tauri-plugin-updater\s*=') {
    throw "Rust updater plugin dependency is missing"
}
if (-not $package.dependencies.'@tauri-apps/plugin-updater') {
    throw "Frontend updater plugin dependency is missing"
}
if (-not $package.dependencies.'@tauri-apps/plugin-process') {
    throw "Frontend process plugin dependency is missing"
}
if ($build -notmatch '\$bundleDirectories' -or $build -notmatch 'foreach \(\$bundleDirectory') {
    throw "build-release.ps1 must collect artifacts only from the requested bundle directories"
}
if ($build -match 'Unsigned release artifacts written' -and $build -notmatch '\$releaseLabel') {
    throw "build-release.ps1 must not label signed artifacts as unsigned"
}
foreach ($requiredText in @(
    "BLOOMERY_UPDATER_PUBLIC_KEY",
    "BLOOMERY_UPDATER_ENDPOINT",
    "BLOOMERY_RELEASE_ASSET_BASE_URL",
    "generate-updater-manifest.ps1",
    "--config",
    "TAURI_SIGNING_PRIVATE_KEY"
)) {
    if ($build -notmatch [regex]::Escape($requiredText) -and $workflow -notmatch [regex]::Escape($requiredText)) {
        throw "Updater release wiring is missing: $requiredText"
    }
}

$manifestFixtureRoot = Join-Path $env:TEMP ("bloomery-updater-manifest-" + [Guid]::NewGuid().ToString("N"))
$manifestFixture = Join-Path $manifestFixtureRoot "Bloomery_0.1.0_x64-setup.exe"
$manifestFixtureSignature = $manifestFixture + ".sig"
$manifestFixtureOutput = Join-Path $manifestFixtureRoot "latest.json"
try {
    New-Item -ItemType Directory -Path $manifestFixtureRoot -Force | Out-Null
    Set-Content -LiteralPath $manifestFixture -Value "candidate" -Encoding ASCII
    Set-Content -LiteralPath $manifestFixtureSignature -Value "test-signature" -Encoding ASCII
    $global:LASTEXITCODE = 0
    & $manifestScript -ArtifactDirectory $manifestFixtureRoot -Version "0.1.0" -ReleaseBaseUrl "https://github.com/luckysus/bloomery/releases/download/v0.1.0-test" -OutputPath $manifestFixtureOutput
    if ($LASTEXITCODE -ne 0) {
        throw "Updater manifest fixture failed with exit code $LASTEXITCODE"
    }
    $manifest = Get-Content -LiteralPath $manifestFixtureOutput -Raw | ConvertFrom-Json
    $platform = $manifest.platforms.'windows-x86_64-nsis'
    if ($platform.url -notmatch 'Bloomery_0\.1\.0_x64-setup\.exe$' -or $platform.signature -ne 'test-signature') {
        throw "Updater manifest fixture did not select the signed NSIS installer"
    }
}
finally {
    Remove-Item -LiteralPath $manifestFixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output "Updater contract passed."
