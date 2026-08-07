[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$requiredScripts = @(
    @{ Path = "scripts/check.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
    @{ Path = "scripts/test.ps1"; RequiresOffline = $true; RequiresExitCode = $true },
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

Write-Output "Release script contract passed."
