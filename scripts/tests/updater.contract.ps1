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
$authenticodeScript = Join-Path $repoRoot "scripts\sign-authenticode.ps1"
$manifestScript = Join-Path $repoRoot "scripts\generate-updater-manifest.ps1"

foreach ($path in @($configPath, $cargoPath, $packagePath, $buildPath, $workflowPath, $configScript, $authenticodeScript, $manifestScript)) {
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

$configFixtureRoot = Join-Path $env:TEMP ("bloomery-updater-config-" + [Guid]::NewGuid().ToString("N"))
$configFixturePath = Join-Path $configFixtureRoot "overlay.json"
$configEnvironmentNames = @(
    "BLOOMERY_UPDATER_PUBLIC_KEY",
    "BLOOMERY_UPDATER_ENDPOINT",
    "BLOOMERY_AUTHENTICODE_PFX_BASE64",
    "BLOOMERY_AUTHENTICODE_PFX_PASSWORD",
    "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL"
)
$configEnvironmentSnapshot = @{}
foreach ($name in $configEnvironmentNames) {
    $configEnvironmentSnapshot[$name] = if (Test-Path -LiteralPath ("Env:" + $name)) {
        [string](Get-Item -LiteralPath ("Env:" + $name)).Value
    } else {
        $null
    }
}
try {
    New-Item -ItemType Directory -Path $configFixtureRoot -Force | Out-Null
    $env:BLOOMERY_UPDATER_PUBLIC_KEY = "test-public-key"
    $env:BLOOMERY_UPDATER_ENDPOINT = "https://github.com/luckysus/bloomery/releases/latest/download/latest.json"
    $env:BLOOMERY_AUTHENTICODE_PFX_BASE64 = "ZmFrZQ=="
    $env:BLOOMERY_AUTHENTICODE_PFX_PASSWORD = "test-password"
    $env:BLOOMERY_AUTHENTICODE_TIMESTAMP_URL = "http://timestamp.example.test"
    $timestampError = ""
    try {
        & $configScript -OutputPath $configFixturePath
        if ($LASTEXITCODE -eq 0) {
            throw "write-updater-config.ps1 accepted an insecure HTTP timestamp URL"
        }
    } catch {
        $timestampError = $_.Exception.Message
    }
    if ($timestampError -notmatch "HTTPS") {
        throw "write-updater-config.ps1 must reject non-HTTPS timestamp URLs"
    }

    $timestampFixturePath = Join-Path $configFixtureRoot "candidate.txt"
    Set-Content -LiteralPath $timestampFixturePath -Value "candidate" -Encoding ASCII
    $signTimestampError = ""
    try {
        & $authenticodeScript -Path $timestampFixturePath
        if ($LASTEXITCODE -eq 0) {
            throw "sign-authenticode.ps1 accepted an insecure HTTP timestamp URL"
        }
    } catch {
        $signTimestampError = $_.Exception.Message
    }
    if ($signTimestampError -notmatch "HTTPS") {
        throw "sign-authenticode.ps1 must reject non-HTTPS timestamp URLs"
    }
} finally {
    foreach ($name in $configEnvironmentNames) {
        $path = "Env:" + $name
        if ($null -eq $configEnvironmentSnapshot[$name]) {
            Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue
        } else {
            Set-Item -LiteralPath $path -Value $configEnvironmentSnapshot[$name]
        }
    }
    if (Test-Path -LiteralPath $configFixtureRoot) {
        Remove-Item -LiteralPath $configFixtureRoot -Recurse -Force
    }
}

$manifestFixtureRoot = Join-Path $env:TEMP ("bloomery-updater-manifest-" + [Guid]::NewGuid().ToString("N"))
$manifestFixture = Join-Path $manifestFixtureRoot "Bloomery_0.1.0_x64-setup.exe"
$manifestFixtureSignature = $manifestFixture + ".sig"
$msiManifestFixture = Join-Path $manifestFixtureRoot "Bloomery_0.1.0_x64.msi"
$msiManifestFixtureSignature = $msiManifestFixture + ".sig"
$legacyManifestFixture = Join-Path $manifestFixtureRoot "Bloomery_0.1.0_x64.nsis.zip"
$legacyManifestFixtureSignature = $legacyManifestFixture + ".sig"
$legacyMsiManifestFixture = Join-Path $manifestFixtureRoot "Bloomery_0.1.0_x64.msi.zip"
$legacyMsiManifestFixtureSignature = $legacyMsiManifestFixture + ".sig"
$manifestFixtureOutput = Join-Path $manifestFixtureRoot "latest.json"
try {
    New-Item -ItemType Directory -Path $manifestFixtureRoot -Force | Out-Null
    Set-Content -LiteralPath $manifestFixture -Value "candidate" -Encoding ASCII
    Set-Content -LiteralPath $manifestFixtureSignature -Value "test-signature" -Encoding ASCII
    Set-Content -LiteralPath $msiManifestFixture -Value "msi-candidate" -Encoding ASCII
    Set-Content -LiteralPath $msiManifestFixtureSignature -Value "msi-test-signature" -Encoding ASCII
    Set-Content -LiteralPath $legacyManifestFixture -Value "legacy-candidate" -Encoding ASCII
    Set-Content -LiteralPath $legacyManifestFixtureSignature -Value "legacy-test-signature" -Encoding ASCII
    Set-Content -LiteralPath $legacyMsiManifestFixture -Value "legacy-msi-candidate" -Encoding ASCII
    Set-Content -LiteralPath $legacyMsiManifestFixtureSignature -Value "legacy-msi-test-signature" -Encoding ASCII
    $global:LASTEXITCODE = 0
    & $manifestScript -ArtifactDirectory $manifestFixtureRoot -Version "0.1.0" -ReleaseBaseUrl "https://github.com/luckysus/bloomery/releases/download/v0.1.0-test" -OutputPath $manifestFixtureOutput
    if ($LASTEXITCODE -ne 0) {
        throw "Updater manifest fixture failed with exit code $LASTEXITCODE"
    }
    $manifest = Get-Content -LiteralPath $manifestFixtureOutput -Raw | ConvertFrom-Json
    $nsisProperty = $manifest.platforms.PSObject.Properties | Where-Object Name -eq "windows-x86_64-nsis" | Select-Object -First 1
    $msiProperty = $manifest.platforms.PSObject.Properties | Where-Object Name -eq "windows-x86_64-msi" | Select-Object -First 1
    $genericProperty = $manifest.platforms.PSObject.Properties | Where-Object Name -eq "windows-x86_64" | Select-Object -First 1
    $nsisPlatform = if ($null -eq $nsisProperty) { $null } else { $nsisProperty.Value }
    $msiPlatform = if ($null -eq $msiProperty) { $null } else { $msiProperty.Value }
    $genericPlatform = if ($null -eq $genericProperty) { $null } else { $genericProperty.Value }
    if ($null -eq $nsisPlatform -or $null -eq $msiPlatform) {
        throw "Updater manifest fixture is missing the installer-specific Windows platform keys"
    }
    if ($null -eq $genericPlatform) {
        throw "Updater manifest fixture is missing the portable Windows platform fallback"
    }
    if ($nsisPlatform.url -notmatch 'Bloomery_0\.1\.0_x64-setup\.exe$' -or $nsisPlatform.signature -ne 'test-signature') {
        throw "Updater manifest fixture did not prefer the signed NSIS installer"
    }
    if ($msiPlatform.url -notmatch 'Bloomery_0\.1\.0_x64\.msi$' -or $msiPlatform.signature -ne 'msi-test-signature') {
        throw "Updater manifest fixture did not select the signed MSI installer"
    }
    if ($genericPlatform.url -notmatch 'Bloomery_0\.1\.0_x64-setup\.exe$' -or $genericPlatform.signature -ne 'test-signature') {
        throw "Updater manifest fixture did not use the signed NSIS installer for portable fallback"
    }
}
finally {
    Remove-Item -LiteralPath $manifestFixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output "Updater contract passed."
