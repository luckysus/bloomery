[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$rustRoot = Join-Path $repoRoot "src-tauri"
$tauriConfigPath = Join-Path $rustRoot "tauri.conf.json"
$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw | ConvertFrom-Json
$iconProperty = $tauriConfig.bundle.PSObject.Properties["icon"]
if ($null -eq $iconProperty) {
    throw "tauri.conf.json must declare bundle.icon for Windows packaging"
}
$windowsIcon = @($iconProperty.Value) | Where-Object { [string]$_ -match '\.ico$' } | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace([string]$windowsIcon)) {
    throw "tauri.conf.json must declare a Windows .ico bundle icon"
}
if (-not (Test-Path -LiteralPath (Join-Path $rustRoot ([string]$windowsIcon)) -PathType Leaf)) {
    throw "Configured Windows bundle icon is missing: $windowsIcon"
}

$buildReleasePath = Join-Path $repoRoot "scripts\build-release.ps1"
$buildReleaseContent = Get-Content -LiteralPath $buildReleasePath -Raw
$missingOfficialKeyValidation = $buildReleaseContent -notmatch "BLOOMERY_OFFICIAL_PUBLIC_KEY_2026" `
    -or $buildReleaseContent -notmatch "64 hexadecimal characters"
if ($missingOfficialKeyValidation) {
    throw "signed release must require and validate the official domain-package public key"
}
$domainSignerPath = Join-Path $rustRoot "src\bin\sign_domain_package.rs"
if (-not (Test-Path -LiteralPath $domainSignerPath -PathType Leaf)) {
    throw "release signing helper is missing: src/bin/sign_domain_package.rs"
}
if ($buildReleaseContent -notmatch "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026" `
    -or $buildReleaseContent -notmatch "sign_domain_package") {
    throw "signed release must generate the official domain-package signature"
}
if ($buildReleaseContent -notmatch "BLOOMERY_OFFICIAL_PRIVATE_KEY_2026.*64 hexadecimal characters") {
    throw "signed release must validate the official domain-package private seed"
}
if ($buildReleaseContent -notmatch 'Remove-Item Env:\\BLOOMERY_OFFICIAL_PRIVATE_KEY_2026') {
    throw "build-release.ps1 must remove the official domain private seed before unrelated build steps"
}
if ($buildReleaseContent -notmatch '\$bundleRoot' -or
    $buildReleaseContent -notmatch '\$staleBundlePath' -or
    $buildReleaseContent -notmatch 'Remove-Item -LiteralPath \$staleBundlePath -Recurse -Force') {
    throw "build-release.ps1 must clear stale bundle outputs before collecting release artifacts"
}
if ($buildReleaseContent -notmatch '@\(\$releaseArtifacts\)\.Count') {
    throw "build-release.ps1 must count collected release artifacts through an array wrapper for single-bundle builds"
}

$resourceProperties = @($tauriConfig.bundle.resources.PSObject.Properties.Name)
if ($resourceProperties -notcontains "resources/compute-worker") {
    throw "tauri.conf.json must bundle the packaged compute worker"
}
if ($buildReleaseContent -notmatch "bloomery-python-worker-sbom.cdx.json") {
    throw "release artifacts must include the Python Worker SBOM generated from uv.lock"
}

foreach ($requiredWorkerText in @(
    '$workerBuildScript',
    '$workerResourceRoot',
    "bloomery-compute-worker.exe",
    'Join-Path $workerRoot "build.ps1"'
)) {
    if ($buildReleaseContent -notmatch [regex]::Escape($requiredWorkerText)) {
        throw "scripts/build-release.ps1 is missing Worker release integration: $requiredWorkerText"
    }
}

foreach ($requiredArtifactText in @(
    "portable",
    "compute-worker-addon",
    "Portable application binary",
    "Compress-Archive",
    "Bloomery-"
)) {
    if ($buildReleaseContent -notmatch [regex]::Escape($requiredArtifactText)) {
        throw "scripts/build-release.ps1 must produce portable and compute Worker add-on artifacts: $requiredArtifactText"
    }
}

$requiredScripts = @(
    @{ Path = "scripts/check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/test.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/security-check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/release-check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/build-release.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/generate-sbom.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/generate-checksums.ps1"; RequiresOffline = $false; RequiresExitCode = $false }
)

foreach ($scriptDefinition in $requiredScripts) {
    $relativePath = $scriptDefinition.Path
    $path = Join-Path $repoRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required release script is missing: $relativePath"
    }

    $content = Get-Content -LiteralPath $path -Raw
    if ($content -notmatch "Set-StrictMode -Version Latest") {
        throw "$relativePath must enable strict mode"
    }
    if ($content -notmatch "ErrorActionPreference") {
        throw "$relativePath must stop on errors"
    }
    if ($scriptDefinition.RequiresExitCode -and $content -notmatch "LASTEXITCODE") {
        throw "$relativePath must propagate child-process failures"
    }
    if ($scriptDefinition.RequiresOffline -and $content -notmatch '\[switch\]\$Offline') {
        throw "$relativePath must support offline verification"
    }
    if ($relativePath -eq "scripts/generate-sbom.ps1") {
        foreach ($requiredSbomText in @(
            '$workerRoot',
            '"uv.lock"',
            "bloomery-python-worker-sbom.cdx.json",
            "pkg:pypi/"
        )) {
            if ($content -notmatch [regex]::Escape($requiredSbomText)) {
                throw "$relativePath must generate a Python Worker SBOM from uv.lock"
            }
        }
    }
    if ($relativePath -eq "scripts/test.ps1" -and $content -notmatch '--test-threads=1') {
        throw "$relativePath must serialize Rust tests that mutate process-wide diagnostics state"
    }
    if ($relativePath -eq "scripts/test.ps1" -and $content -notmatch '--jobs.*1') {
        throw "$relativePath must serialize Rust test binaries to avoid cross-binary races"
    }
}

function npm {
    $global:LASTEXITCODE = 37
}

$checkScript = Join-Path $repoRoot "scripts\check.ps1"
try {
    & $checkScript -SkipFrontendBuild
    throw "check.ps1 did not propagate the injected child-process failure"
}
catch {
    if ($_.Exception.Message -notmatch "Frontend runtime boundaries failed with exit code 37") {
        throw
    }
}
finally {
    Remove-Item Function:\npm -ErrorAction SilentlyContinue
}

function Assert-InjectedFailure {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][scriptblock]$Invocation,
        [Parameter(Mandatory = $true)][string]$ExpectedMessage
    )

    try {
        & $Invocation
        throw "$Name did not propagate the injected child-process failure"
    }
    catch {
        if ($_.Exception.Message -notmatch [regex]::Escape($ExpectedMessage)) {
            throw "$Name returned an unexpected error: $($_.Exception.Message)"
        }
    }
    finally {
        $global:LASTEXITCODE = 0
    }
}

$testScript = Join-Path $repoRoot "scripts\test.ps1"
function npm {
    $global:LASTEXITCODE = 37
}
Assert-InjectedFailure -Name "test.ps1" -Invocation { & $testScript -Stage frontend } -ExpectedMessage "Frontend unit and integration tests failed with exit code 37"
Remove-Item Function:\npm -ErrorAction SilentlyContinue

$releaseCheckScript = Join-Path $repoRoot "scripts\release-check.ps1"
$releaseCheckContent = Get-Content -LiteralPath $releaseCheckScript -Raw
$authenticodeScript = Join-Path $repoRoot "scripts\sign-authenticode.ps1"
if (-not (Test-Path -LiteralPath $authenticodeScript -PathType Leaf)) {
    throw "Authenticode signing script is missing"
}
$authenticodeContent = Get-Content -LiteralPath $authenticodeScript -Raw
foreach ($requiredAuthenticodeText in @(
    "Set-StrictMode -Version Latest",
    "BLOOMERY_AUTHENTICODE_PFX_BASE64",
    "BLOOMERY_AUTHENTICODE_PFX_PASSWORD",
    "Get-AuthenticodeSignature",
    "signtool",
    "Remove-Item"
)) {
    if ($authenticodeContent -notmatch [regex]::Escape($requiredAuthenticodeText)) {
        throw "sign-authenticode.ps1 is missing required behavior: $requiredAuthenticodeText"
    }
}
$buildScript = Join-Path $repoRoot "scripts\build-release.ps1"
$buildReleaseContent = Get-Content -LiteralPath $buildScript -Raw
if ($buildReleaseContent -notmatch "sign-authenticode\.ps1") {
    throw "build-release.ps1 must sign Windows release binaries before artifact finalization"
}
foreach ($requiredSignedArtifactText in @(
    'Authenticode compute Worker signature',
    'Authenticode portable binaries',
    'Get-AuthenticodeSignature',
    'signature_note',
    'authenticode'
)) {
    if ($buildReleaseContent -notmatch [regex]::Escape($requiredSignedArtifactText)) {
        throw "build-release.ps1 must verify signed portable and compute Worker artifacts: $requiredSignedArtifactText"
    }
}
foreach ($requiredCurrentWorkerText in @(
    '$portableWorkerSource',
    'Copy-RequiredDirectory $workerBuildOutput $portableWorkerSource',
    'Copy-RequiredDirectory $portableWorkerSource (Join-Path $portableRoot "compute-worker")',
    'Copy-RequiredDirectory $portableWorkerSource (Join-Path $addonRoot "compute-worker")'
)) {
    if ($buildReleaseContent -notmatch [regex]::Escape($requiredCurrentWorkerText)) {
        throw "build-release.ps1 must package the current Worker build, not stale target output: $requiredCurrentWorkerText"
    }
}
if ($buildReleaseContent -notmatch '\$buildArguments \+= @\("--", "--bin", "bloomery"\)') {
    throw "build-release.ps1 must restrict the Tauri package build to the bloomery application binary"
}
if ($buildReleaseContent -notmatch '\$portableBuildArguments = @\("build", "--release", "--features", "custom-protocol", "--bin", "bloomery"\)') {
    throw "build-release.ps1 must restrict the portable build to the bloomery application binary"
}
$portableWorkerCleanup = $buildReleaseContent.LastIndexOf('if (Test-Path -LiteralPath $portableWorkerSource)', [StringComparison]::Ordinal)
$addonCopy = $buildReleaseContent.IndexOf('Copy-RequiredDirectory $portableWorkerSource (Join-Path $addonRoot "compute-worker")', [StringComparison]::Ordinal)
$portableZip = $buildReleaseContent.IndexOf('New-ZipFromDirectory $portableRoot', [StringComparison]::Ordinal)
$addonZip = $buildReleaseContent.IndexOf('New-ZipFromDirectory $addonRoot', [StringComparison]::Ordinal)
if ($portableWorkerCleanup -lt 0 -or $addonCopy -lt 0 -or $portableZip -lt 0 -or $addonZip -lt 0) {
    throw "build-release.ps1 must define Worker packaging and staged-source cleanup"
}
if ($portableWorkerCleanup -lt $addonCopy -or $portableWorkerCleanup -lt $portableZip -or $portableWorkerCleanup -lt $addonZip) {
    throw "build-release.ps1 must not delete the staged Worker source before portable and add-on packaging completes"
}
if ($releaseCheckContent -notmatch '\[switch\]\$RequireTagVersion' -or
    $releaseCheckContent -notmatch 'GITHUB_REF_NAME' -or
    $releaseCheckContent -notmatch '(?i)tag.*version|version.*tag') {
    throw "release-check.ps1 must enforce tag/version consistency for release verification"
}
$lifecycleInvocation = $releaseCheckContent.IndexOf('Invoke-Checked "Windows data lifecycle checks"', [StringComparison]::Ordinal)
$packageInvocation = $releaseCheckContent.IndexOf('Invoke-Checked "Windows release package"', [StringComparison]::Ordinal)
if ($lifecycleInvocation -lt 0 -or $packageInvocation -lt 0 -or $packageInvocation -gt $lifecycleInvocation) {
    throw "release-check.ps1 must build the package before validating the packaged lifecycle"
}
if ($releaseCheckContent -notmatch '\[switch\]\$Performance' -or
    $releaseCheckContent -notmatch 'benchmark-retrieval\.ps1' -or
    $releaseCheckContent -notmatch 'benchmark-dataset-import\.ps1' -or
    $releaseCheckContent -notmatch 'benchmark-agent-performance\.ps1' -or
    $releaseCheckContent -notmatch 'benchmark-startup\.ps1') {
    throw "release-check.ps1 must expose the deterministic performance gate"
}
if ($releaseCheckContent -notmatch 'compute-worker' -or
    $releaseCheckContent -notmatch 'pytest' -or
    $releaseCheckContent -notmatch 'steel_evaluations') {
    throw "release-check.ps1 must run the Python Worker and steel evaluation gates"
}
if ($releaseCheckContent -notmatch '\[switch\]\$InstallerSmoke' -or $releaseCheckContent -notmatch '-RunInstallerSmoke') {
    throw "release-check.ps1 must expose the Windows installer smoke gate"
}
if ($releaseCheckContent -notmatch '\[switch\]\$UpgradeDowngrade' -or
    $releaseCheckContent -notmatch '\[string\]\$OldInstallerPath' -or
    $releaseCheckContent -notmatch 'lifecycle-matrix\.ps1' -or
    $releaseCheckContent -notmatch '-RunUpgradeDowngrade') {
    throw "release-check.ps1 must expose the explicit old-to-new-to-old lifecycle matrix gate"
}
if ($releaseCheckContent -notmatch 'UpgradeDowngrade.*Package|Package.*UpgradeDowngrade' -or
    $releaseCheckContent -notmatch 'OldInstallerPath') {
    throw "release-check.ps1 must require a packaged current installer and an explicit old installer for the lifecycle matrix"
}
if ($releaseCheckContent -notmatch 'RequireSigned' -or
    $releaseCheckContent -notmatch '(?s)if\s*\(\-not\s+\$RequireSigned\).*?AllowUnsigned') {
    throw "release-check.ps1 must allow unsigned artifacts only when signed verification is not requested"
}
if ($releaseCheckContent -notmatch '\[switch\]\$Signed' -or
    $releaseCheckContent -notmatch '\[switch\]\$RequireSigned' -or
    $releaseCheckContent -notmatch 'AllowUnsigned') {
    throw "release-check.ps1 must distinguish unsigned engineering packages from required signed releases"
}
if ($releaseCheckContent -notmatch 'RequireSigned.*Signed|Signed.*RequireSigned') {
    throw "release-check.ps1 must require -Signed when -RequireSigned is requested"
}
foreach ($requiredPackageIsolationText in @(
    '$packageOutputPath',
    '-OutputDirectory',
    'Get-ChildItem -LiteralPath $packageOutputPath',
    '-InstallerPath',
    '$candidateInstaller.FullName',
    '*portable*.zip',
    '*compute-worker-addon*.zip'
)) {
    if ($releaseCheckContent -notmatch [regex]::Escape($requiredPackageIsolationText)) {
    throw "release-check.ps1 must isolate package output and lifecycle validation to the current run"
    }
}
function powershell {
    $global:LASTEXITCODE = 37
}
Assert-InjectedFailure -Name "release-check.ps1" -Invocation { & $releaseCheckScript -AllowDirty } -ExpectedMessage "Deterministic release test suite failed with exit code 37"
Remove-Item Function:\powershell -ErrorAction SilentlyContinue

function cargo {
    $global:LASTEXITCODE = 37
}
function powershell {
    param([Parameter(ValueFromRemainingArguments = $true)][object[]]$Arguments)
    $outputIndex = [Array]::IndexOf($Arguments, "-OutputDirectory")
    if ($outputIndex -lt 0 -or $outputIndex + 1 -ge $Arguments.Count) {
        throw "contract Worker build did not receive an output directory"
    }
    $workerOutputRoot = [string]$Arguments[$outputIndex + 1]
    New-Item -ItemType Directory -Path $workerOutputRoot -Force | Out-Null
    foreach ($artifactName in @(
        "bloomery-compute-worker.exe",
        "worker-artifact-manifest.json",
        "worker-sbom.json",
        "bloomery-compute-worker.sha256"
    )) {
        switch ($artifactName) {
            "worker-artifact-manifest.json" {
                Set-Content -LiteralPath (Join-Path $workerOutputRoot $artifactName) -Value (@{
                    schema_version = "1.0.0"
                    artifact = "bloomery-compute-worker"
                    executable = "bloomery-compute-worker.exe"
                    sha256 = ("0" * 64)
                    signature = "unsigned-explicit"
                    signature_note = "contract fixture"
                } | ConvertTo-Json)
            }
            "worker-sbom.json" {
                Set-Content -LiteralPath (Join-Path $workerOutputRoot $artifactName) -Value (@{
                    schema_version = "1.0.0"
                    component = "bloomery-compute-worker"
                    components = @(@{
                        name = "bloomery-compute-worker"
                        sha256 = ("0" * 64)
                    }, @{
                        name = "bloomery-compute-worker"
                    })
                } | ConvertTo-Json)
            }
            default {
                Set-Content -LiteralPath (Join-Path $workerOutputRoot $artifactName) -Value "contract fixture"
            }
        }
    }
    $global:LASTEXITCODE = 0
}
$contractOutput = Join-Path $env:TEMP ("bloomery-release-contract-" + [Guid]::NewGuid().ToString())
Assert-InjectedFailure -Name "build-release.ps1" -Invocation { & $buildScript -SkipTests -Bundles nsis -OutputDirectory $contractOutput } -ExpectedMessage "Unsigned Windows package build failed with exit code 37"
Remove-Item Function:\cargo -ErrorAction SilentlyContinue
Remove-Item Function:\powershell -ErrorAction SilentlyContinue

$global:LASTEXITCODE = 0
Write-Output "Release script contract passed."
