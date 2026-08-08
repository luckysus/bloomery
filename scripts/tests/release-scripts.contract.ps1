[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$requiredScripts = @(
    @{ Path = "scripts/check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/test.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/security-check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/release-check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/build-release.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
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
function powershell {
    $global:LASTEXITCODE = 37
}
Assert-InjectedFailure -Name "release-check.ps1" -Invocation { & $releaseCheckScript -AllowDirty } -ExpectedMessage "Deterministic release test suite failed with exit code 37"
Remove-Item Function:\powershell -ErrorAction SilentlyContinue

$buildScript = Join-Path $repoRoot "scripts\build-release.ps1"
function cargo {
    $global:LASTEXITCODE = 37
}
$contractOutput = Join-Path $env:TEMP ("bloomery-release-contract-" + [Guid]::NewGuid().ToString())
Assert-InjectedFailure -Name "build-release.ps1" -Invocation { & $buildScript -SkipTests -Bundles nsis -OutputDirectory $contractOutput } -ExpectedMessage "Unsigned Windows package build failed with exit code 37"
Remove-Item Function:\cargo -ErrorAction SilentlyContinue

$global:LASTEXITCODE = 0
Write-Output "Release script contract passed."
