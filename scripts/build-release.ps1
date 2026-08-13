[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$SkipTests,
    [switch]$Signed,
    [ValidateSet("msi", "nsis", "all")][string]$Bundles = "all",
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$rustRoot = Join-Path $repoRoot "src-tauri"
$tauriConfigPath = Join-Path $rustRoot "tauri.conf.json"
$workerRoot = Join-Path $repoRoot "compute-worker"
$workerBuildScript = Join-Path $workerRoot "build.ps1"
$domainPackageRoot = Join-Path $repoRoot "domain-packs\steel"
$domainSignaturePath = Join-Path $domainPackageRoot "signature.json"
$workerResourceRoot = Join-Path $rustRoot "resources\compute-worker"
$workerArtifactNames = @(
    "bloomery-compute-worker.exe",
    "worker-artifact-manifest.json",
    "worker-sbom.json",
    "bloomery-compute-worker.sha256"
)
$authenticodeScript = Join-Path $PSScriptRoot "sign-authenticode.ps1"

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

function Remove-EnvironmentVariable {
    param(
        [Parameter(Mandatory = $true)][string]$Name
    )

    $path = "Env:\$Name"
    if (Test-Path -LiteralPath $path) {
        Remove-Item -LiteralPath $path -Force
        if (Test-Path -LiteralPath $path) {
            throw "Failed to remove sensitive environment variable: $Name"
        }
    }
}

function Copy-RequiredFile {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Required release file is missing: $Source"
    }
    $destinationParent = Split-Path -Parent $Destination
    if (-not [string]::IsNullOrWhiteSpace($destinationParent)) {
        New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
}

function Copy-RequiredDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
        throw "Required release directory is missing: $Source"
    }
    if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Recurse -Force
    }
    $destinationParent = Split-Path -Parent $Destination
    if (-not [string]::IsNullOrWhiteSpace($destinationParent)) {
        New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Recurse -Force -ErrorAction Stop
}

function New-ZipFromDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$SourceDirectory,
        [Parameter(Mandatory = $true)][string]$DestinationPath
    )

    if (-not (Test-Path -LiteralPath $SourceDirectory -PathType Container)) {
        throw "Zip source directory is missing: $SourceDirectory"
    }
    if (Test-Path -LiteralPath $DestinationPath) {
        Remove-Item -LiteralPath $DestinationPath -Force
    }
    Compress-Archive -LiteralPath $SourceDirectory -DestinationPath $DestinationPath -CompressionLevel Optimal
    $archive = Get-Item -LiteralPath $DestinationPath -ErrorAction Stop
    if ($archive.Length -le 0) {
        throw "Release archive is empty: $DestinationPath"
    }
}

function Invoke-Authenticode {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Paths
    )

    if (-not $Signed) {
        return
    }
    $signArguments = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $authenticodeScript,
        "-Path"
    ) + @($Paths)
    Invoke-Checked $Name "powershell" $signArguments $repoRoot
}

function Assert-AuthenticodeValid {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Paths
    )

    foreach ($path in $Paths) {
        $file = Get-Item -LiteralPath $path -ErrorAction Stop
        if ($file.Extension.ToLowerInvariant() -notin @(".exe", ".dll", ".msi")) {
            throw "Authenticode verification requires an executable target: $path"
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
        if ($signature.Status -ne "Valid") {
            throw "$Name failed for $($file.Name): $($signature.Status)"
        }
    }
    Write-Output ("{0} passed for {1} file(s)." -f $Name, $Paths.Count)
}

function Update-WorkerArtifactMetadata {
    param(
        [Parameter(Mandatory = $true)][string]$WorkerRoot
    )

    $executable = Join-Path $WorkerRoot "bloomery-compute-worker.exe"
    $manifestPath = Join-Path $WorkerRoot "worker-artifact-manifest.json"
    $sbomPath = Join-Path $WorkerRoot "worker-sbom.json"
    $checksumPath = Join-Path $WorkerRoot "bloomery-compute-worker.sha256"
    foreach ($requiredPath in @($executable, $manifestPath, $sbomPath)) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Worker metadata file is missing: $requiredPath"
        }
    }

    $hash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $manifest.sha256 = $hash
    if ($Signed) {
        $manifest.signature = "authenticode-valid"
        $manifest.signature_note = "Authenticode signature was verified before this artifact entered the release package."
    } else {
        $manifest.signature = "unsigned-explicit"
        $manifest.signature_note = "Artifact is intentionally unsigned in this build; release signing happens in the release-quality gate and unsigned artifacts must be clearly marked."
    }
    [System.IO.File]::WriteAllText(
        $manifestPath,
        ($manifest | ConvertTo-Json -Depth 20),
        [System.Text.UTF8Encoding]::new($false)
    )

    $sbom = Get-Content -LiteralPath $sbomPath -Raw | ConvertFrom-Json
    foreach ($component in @($sbom.components)) {
        if ([string]$component.name -eq "bloomery-compute-worker" -and
            $null -ne $component.PSObject.Properties["sha256"]) {
            $component.sha256 = $hash
        }
    }
    [System.IO.File]::WriteAllText(
        $sbomPath,
        ($sbom | ConvertTo-Json -Depth 20),
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::WriteAllText(
        $checksumPath,
        ("{0}  bloomery-compute-worker.exe" -f $hash),
        [System.Text.UTF8Encoding]::new($false)
    )
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
$bundleRoot = Join-Path $rustRoot "target\release\bundle"
$bundleDirectories = if ($Bundles -eq "all") { @("msi", "nsis") } else { @($Bundles) }

if (-not $SkipTests) {
    $testScript = Join-Path $PSScriptRoot "test.ps1"
    $testArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $testScript)
    if ($Offline) {
        $testArguments += "-Offline"
    }
    Invoke-Checked "Deterministic test suite" "powershell" $testArguments $repoRoot
}

$workerBuildOutput = Join-Path $env:TEMP ("bloomery-worker-" + [Guid]::NewGuid().ToString("N"))
$portableWorkerSource = Join-Path $env:TEMP ("bloomery-worker-release-source-" + [Guid]::NewGuid().ToString("N"))
$domainSignatureCreated = $false
try {
    if ($Signed) {
        if (-not (Test-Path -LiteralPath $authenticodeScript -PathType Leaf)) {
            throw "Authenticode signing script is missing: $authenticodeScript"
        }
        foreach ($requiredAuthenticodeVariable in @(
            "BLOOMERY_AUTHENTICODE_PFX_BASE64",
            "BLOOMERY_AUTHENTICODE_PFX_PASSWORD",
            "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL"
        )) {
            if ([string]::IsNullOrWhiteSpace([string](Get-Item -Path ("Env:" + $requiredAuthenticodeVariable)).Value)) {
                throw "$requiredAuthenticodeVariable is required for a signed release"
            }
        }
        if ([string]::IsNullOrWhiteSpace($env:BLOOMERY_OFFICIAL_PRIVATE_KEY_2026)) {
            throw "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026 is required for a signed release"
        }
        if ($env:BLOOMERY_OFFICIAL_PRIVATE_KEY_2026 -notmatch '^[0-9a-fA-F]{64}$') {
            throw "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026 must be exactly 64 hexadecimal characters"
        }
        if (-not (Test-Path -LiteralPath $domainPackageRoot -PathType Container)) {
            throw "Official steel domain package is missing: $domainPackageRoot"
        }
        if (Test-Path -LiteralPath $domainSignaturePath -PathType Leaf) {
            throw "Official steel domain package already contains signature.json"
        }
        $signArguments = @(
            "run",
            "--bin",
            "sign_domain_package",
            "--",
            "--root",
            $domainPackageRoot
        )
        if ($Offline) {
            $signArguments = @("--offline") + $signArguments
        }
        Invoke-Checked "Official domain package signature" "cargo" $signArguments $rustRoot
        if (-not (Test-Path -LiteralPath $domainSignaturePath -PathType Leaf)) {
            throw "Domain package signer completed without signature.json"
        }
        $domainSignatureCreated = $true
        Remove-EnvironmentVariable "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026"
    }
    if (-not (Test-Path -LiteralPath $workerBuildScript -PathType Leaf)) {
        throw "Compute worker build script is missing: $workerBuildScript"
    }
    $workerBuildArguments = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $workerBuildScript,
        "-OutputDirectory",
        $workerBuildOutput
    )
    if ($Offline) {
        $workerBuildArguments += "-Offline"
    }
    Invoke-Checked "Compute Worker package" "powershell" $workerBuildArguments $repoRoot

    $workerExecutable = Join-Path $workerBuildOutput "bloomery-compute-worker.exe"
    if ($Signed) {
        Invoke-Authenticode "Authenticode compute Worker signature" @($workerExecutable)
        Assert-AuthenticodeValid "Authenticode compute Worker signature" @($workerExecutable)
    }
    Update-WorkerArtifactMetadata $workerBuildOutput
    Copy-RequiredDirectory $workerBuildOutput $portableWorkerSource

    New-Item -ItemType Directory -Path $workerResourceRoot -Force | Out-Null
    foreach ($artifactName in $workerArtifactNames) {
        $sourcePath = Join-Path $workerBuildOutput $artifactName
        if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
            throw "Compute worker artifact is missing: $sourcePath"
        }
        $destinationPath = Join-Path $workerResourceRoot $artifactName
        if (Test-Path -LiteralPath $destinationPath) {
            Remove-Item -LiteralPath $destinationPath -Force
        }
        Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Force
    }

    $bundleValue = if ($Bundles -eq "all") { "msi,nsis" } else { $Bundles }
    foreach ($bundleDirectory in $bundleDirectories) {
        $staleBundlePath = Join-Path $bundleRoot $bundleDirectory
        if (Test-Path -LiteralPath $staleBundlePath) {
            Remove-Item -LiteralPath $staleBundlePath -Recurse -Force
        }
    }
    $buildArguments = @(
        "tauri",
        "build",
        "--ci",
        "--features",
        "custom-protocol",
        "--bundles",
        $bundleValue
    )
    $updaterConfigPath = $null
    if ($Signed) {
        if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
            throw "TAURI_SIGNING_PRIVATE_KEY is required for a signed release"
        }
        if ([string]::IsNullOrWhiteSpace($env:BLOOMERY_OFFICIAL_PUBLIC_KEY_2026)) {
            throw "BLOOMERY_OFFICIAL_PUBLIC_KEY_2026 is required for a signed release"
        }
        if ($env:BLOOMERY_OFFICIAL_PUBLIC_KEY_2026 -notmatch '^[0-9a-fA-F]{64}$') {
            throw "BLOOMERY_OFFICIAL_PUBLIC_KEY_2026 must be exactly 64 hexadecimal characters"
        }
        if ([string]::IsNullOrWhiteSpace($env:BLOOMERY_UPDATER_PUBLIC_KEY)) {
            throw "BLOOMERY_UPDATER_PUBLIC_KEY is required for a signed release"
        }
        if ([string]::IsNullOrWhiteSpace($env:BLOOMERY_UPDATER_ENDPOINT)) {
            throw "BLOOMERY_UPDATER_ENDPOINT is required for a signed release"
        }
        if ([string]::IsNullOrWhiteSpace($env:BLOOMERY_RELEASE_ASSET_BASE_URL)) {
            throw "BLOOMERY_RELEASE_ASSET_BASE_URL is required for a signed release"
        }
        $updaterConfigPath = Join-Path $env:TEMP ("bloomery-updater-" + [Guid]::NewGuid().ToString("N") + ".json")
        $configScript = Join-Path $PSScriptRoot "write-updater-config.ps1"
        Invoke-Checked "Signed updater configuration" "powershell" @(
            "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $configScript,
            "-OutputPath", $updaterConfigPath
        ) $repoRoot
        $buildArguments += @("--config", $updaterConfigPath)
    } else {
        $buildArguments += "--no-sign"
    }
    if ($Offline) {
        $buildArguments = @("--offline") + $buildArguments
    }
    $buildArguments += @("--", "--bin", "bloomery")
    try {
        $buildName = if ($Signed) { "Signed Windows package build" } else { "Unsigned Windows package build" }
        Invoke-Checked $buildName "cargo" $buildArguments $rustRoot
        if ($Signed) {
            Assert-AuthenticodeValid "Authenticode packaged Worker verification" @(
                (Join-Path $workerResourceRoot "bloomery-compute-worker.exe")
            )
        }
    } finally {
        if ($updaterConfigPath -and (Test-Path -LiteralPath $updaterConfigPath)) {
            Remove-Item -LiteralPath $updaterConfigPath -Force
            if (Test-Path -LiteralPath $updaterConfigPath) {
                throw "Failed to remove temporary updater configuration: $updaterConfigPath"
            }
        }
    }
} finally {
    if ($Signed) {
        Remove-EnvironmentVariable "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026"
    }
    if ($domainSignatureCreated -and (Test-Path -LiteralPath $domainSignaturePath -PathType Leaf)) {
        Remove-Item -LiteralPath $domainSignaturePath -Force
        if (Test-Path -LiteralPath $domainSignaturePath -PathType Leaf) {
            throw "Failed to remove temporary official domain package signature"
        }
    }
    foreach ($artifactName in $workerArtifactNames) {
        $stagedArtifact = Join-Path $workerResourceRoot $artifactName
        if (Test-Path -LiteralPath $stagedArtifact) {
            Remove-Item -LiteralPath $stagedArtifact -Force
        }
    }
    if (Test-Path -LiteralPath $workerBuildOutput) {
        Remove-Item -LiteralPath $workerBuildOutput -Recurse -Force
    }
}

$portableBuildArguments = @("build", "--release", "--features", "custom-protocol", "--bin", "bloomery")
if ($Offline) {
    $portableBuildArguments += "--offline"
}
Invoke-Checked "Portable application binary" "cargo" $portableBuildArguments $rustRoot

$releaseArtifacts = @(
    foreach ($bundleDirectory in $bundleDirectories) {
        $bundlePath = Join-Path $bundleRoot $bundleDirectory
        if (Test-Path -LiteralPath $bundlePath -PathType Container) {
            Get-ChildItem -LiteralPath $bundlePath -Recurse -File | Where-Object {
                $_.Extension -in @(".exe", ".msi", ".zip", ".sig")
            }
        }
    }
) | Sort-Object Name
if (@($releaseArtifacts).Count -eq 0) {
    throw "Tauri completed without producing an MSI or NSIS artifact"
}
if ($Signed) {
    $installerTargets = @(
        $releaseArtifacts |
            Where-Object { $_.Extension.ToLowerInvariant() -in @(".exe", ".msi") } |
            ForEach-Object { $_.FullName }
    )
    if ($installerTargets.Count -eq 0) {
        throw "Signed release produced no executable installer targets"
    }
    Assert-AuthenticodeValid "Authenticode packaged installers" $installerTargets
}

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
foreach ($artifact in $releaseArtifacts) {
    Copy-Item -LiteralPath $artifact.FullName -Destination (Join-Path $outputPath $artifact.Name) -ErrorAction Stop
}
foreach ($metadataFile in @("LICENSE", "NOTICE")) {
    Copy-Item -LiteralPath (Join-Path $repoRoot $metadataFile) -Destination (Join-Path $outputPath $metadataFile) -ErrorAction Stop
}

$runtimeRoot = Join-Path $rustRoot "target\release"
$portableName = "Bloomery-$version-windows-x64-portable"
$portableStage = Join-Path $env:TEMP ($portableName + "-" + [Guid]::NewGuid().ToString("N"))
$portableRoot = Join-Path $portableStage $portableName
$addonName = "Bloomery-$version-compute-worker-addon-windows-x64"
$addonStage = Join-Path $env:TEMP ($addonName + "-" + [Guid]::NewGuid().ToString("N"))
$addonRoot = Join-Path $addonStage $addonName
try {
    New-Item -ItemType Directory -Path $portableRoot, $addonRoot -Force | Out-Null
    Copy-RequiredFile (Join-Path $runtimeRoot "bloomery.exe") (Join-Path $portableRoot "bloomery.exe")
    if ($Signed) {
        Invoke-Authenticode "Authenticode portable binaries" @(
            (Join-Path $portableRoot "bloomery.exe")
        )
        Assert-AuthenticodeValid "Authenticode portable binaries" @(
            (Join-Path $portableRoot "bloomery.exe")
        )
    }
    Copy-RequiredDirectory (Join-Path $runtimeRoot "domain-packs") (Join-Path $portableRoot "domain-packs")
    Copy-RequiredDirectory $portableWorkerSource (Join-Path $portableRoot "compute-worker")
    foreach ($metadataFile in @("LICENSE", "NOTICE")) {
        Copy-RequiredFile (Join-Path $repoRoot $metadataFile) (Join-Path $portableRoot $metadataFile)
        Copy-RequiredFile (Join-Path $repoRoot $metadataFile) (Join-Path $addonRoot $metadataFile)
    }
    Copy-RequiredDirectory $portableWorkerSource (Join-Path $addonRoot "compute-worker")
    if ($Signed) {
        Assert-AuthenticodeValid "Authenticode packaged Worker verification" @(
            (Join-Path $portableWorkerSource "bloomery-compute-worker.exe"),
            (Join-Path $portableRoot "compute-worker\bloomery-compute-worker.exe"),
            (Join-Path $addonRoot "compute-worker\bloomery-compute-worker.exe")
        )
    }
    New-ZipFromDirectory $portableRoot (Join-Path $outputPath ($portableName + ".zip"))
    New-ZipFromDirectory $addonRoot (Join-Path $outputPath ($addonName + ".zip"))
} finally {
    foreach ($stage in @($portableStage, $addonStage)) {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
            if (Test-Path -LiteralPath $stage) {
                throw "Failed to remove release staging directory: $stage"
            }
        }
    }
    if (Test-Path -LiteralPath $portableWorkerSource) {
        Remove-Item -LiteralPath $portableWorkerSource -Recurse -Force
        if (Test-Path -LiteralPath $portableWorkerSource) {
            throw "Failed to remove staged Worker source: $portableWorkerSource"
        }
    }
}

if ($Signed) {
    $updaterManifestScript = Join-Path $PSScriptRoot "generate-updater-manifest.ps1"
    Invoke-Checked "Updater metadata" "powershell" @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $updaterManifestScript,
        "-ArtifactDirectory", $outputPath,
        "-Version", $version,
        "-ReleaseBaseUrl", $env:BLOOMERY_RELEASE_ASSET_BASE_URL
    ) $repoRoot
}

$sbomScript = Join-Path $PSScriptRoot "generate-sbom.ps1"
$sbomArguments = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $sbomScript,
    "-OutputDirectory", $outputPath
)
if ($Offline) {
    $sbomArguments += "-Offline"
}
Invoke-Checked "SBOM and third-party notices" "powershell" $sbomArguments $repoRoot
foreach ($requiredReleaseFile in @(
    "bloomery-rust-sbom.cdx.json",
    "bloomery-frontend-sbom.cdx.json",
    "bloomery-frontend-sbom.spdx.json",
    "bloomery-python-worker-sbom.cdx.json",
    "THIRD_PARTY_NOTICES.txt"
)) {
    $requiredPath = Join-Path $outputPath $requiredReleaseFile
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required release SBOM or notice file is missing: $requiredReleaseFile"
    }
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
    signing = if ($Signed) { "signed" } else { "unsigned" }
    artifacts = @($manifestArtifacts)
}
$manifestPath = Join-Path $outputPath "release-manifest.json"
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 5), [System.Text.UTF8Encoding]::new($false))

$checksumScript = Join-Path $PSScriptRoot "generate-checksums.ps1"
Invoke-Checked "Artifact checksum generation" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $checksumScript, "-InputPath", $outputPath) $repoRoot

$releaseLabel = if ($Signed) { "Signed" } else { "Unsigned" }
Write-Host ($releaseLabel + " release artifacts written to " + $outputPath)
