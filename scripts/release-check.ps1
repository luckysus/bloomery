[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$WithE2E,
    [switch]$Package,
    [switch]$AllowDirty
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

if (-not $AllowDirty) {
    $dirty = git -C $repoRoot status --porcelain
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to inspect Git status"
    }
    if ($dirty) {
        throw "Release checks require a clean worktree. Use -AllowDirty only for local investigation."
    }
}

$frontendRoot = Join-Path $repoRoot "frontend"
$rustRoot = Join-Path $repoRoot "src-tauri"
$tauriConfigPath = Join-Path $rustRoot "tauri.conf.json"
$packagePath = Join-Path $frontendRoot "package.json"
$cargoPath = Join-Path $rustRoot "Cargo.toml"

foreach ($requiredPath in @($tauriConfigPath, $packagePath, $cargoPath, (Join-Path $repoRoot "README.md"), (Join-Path $repoRoot "LICENSE"), (Join-Path $repoRoot "docs\PROTOCOL.md"))) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required release file is missing: $requiredPath"
    }
}

$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw | ConvertFrom-Json
$frontendPackage = Get-Content -LiteralPath $packagePath -Raw | ConvertFrom-Json
$cargoText = Get-Content -LiteralPath $cargoPath -Raw
$tauriVersion = [string]$tauriConfig.version
$frontendVersion = [string]$frontendPackage.version
$cargoVersionMatch = [regex]::Match($cargoText, '(?m)^version\s*=\s*"([^"]+)"')

if (-not $tauriVersion -or -not $frontendVersion -or -not $cargoVersionMatch.Success) {
    throw "Unable to read versions from release manifests"
}
if (($tauriVersion -ne $frontendVersion) -or ($tauriVersion -ne $cargoVersionMatch.Groups[1].Value)) {
    throw "Version mismatch: tauri=$tauriVersion frontend=$frontendVersion cargo=$($cargoVersionMatch.Groups[1].Value)"
}
if (-not $tauriConfig.bundle.active) {
    throw "Tauri bundling must be enabled for a release"
}

$sourceRoots = @(
    (Join-Path $repoRoot "frontend\src"),
    (Join-Path $repoRoot "src-tauri\src"),
    (Join-Path $repoRoot "domain-packs")
)
$forbiddenPatterns = @(
    '47\.93\.203\.36',
    '43\.155\.210\.216',
    'https?://[^\s"'']*steel-agent[^\s"'']*'
)
$sourceMatches = @()
foreach ($sourceRoot in $sourceRoots) {
    if (Test-Path -LiteralPath $sourceRoot -PathType Container) {
        $sourceMatches += Get-ChildItem -LiteralPath $sourceRoot -Recurse -File | Where-Object {
            $_.FullName -notmatch '\\node_modules\\|\\target\\'
        } | Select-String -Pattern $forbiddenPatterns -AllMatches
    }
}
if ($sourceMatches.Count -gt 0) {
    $locations = ($sourceMatches | ForEach-Object { $_.Path + ":" + $_.LineNumber }) -join ", "
    throw "Private or project-owned endpoint found in release source: $locations"
}

$scriptArguments = @()
if ($Offline) {
    $scriptArguments += "-Offline"
}
$testScript = Join-Path $PSScriptRoot "test.ps1"
Invoke-Checked "Deterministic release test suite" "powershell" (@("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $testScript) + $scriptArguments) $repoRoot

$securityScript = Join-Path $PSScriptRoot "security-check.ps1"
$securityArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $securityScript)
if ($Offline) {
    $securityArguments += "-Offline"
}
Invoke-Checked "Application security checks" "powershell" $securityArguments $repoRoot

$lifecycleScript = Join-Path $PSScriptRoot "lifecycle-check.ps1"
$lifecycleArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $lifecycleScript)
if ($Package) {
    $candidateInstaller = Get-ChildItem -LiteralPath (Join-Path $repoRoot "artifacts") -Recurse -Filter "*-setup.exe" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime |
        Select-Object -Last 1
    if ($null -eq $candidateInstaller) {
        throw "Packaged release does not contain an NSIS installer"
    }
    $lifecycleArguments += @("-InstallerPath", $candidateInstaller.FullName, "-AllowUnsigned")
}
Invoke-Checked "Windows data lifecycle checks" "powershell" $lifecycleArguments $repoRoot

if ($WithE2E) {
    Invoke-Checked "Frontend end-to-end tests" "npm" @("run", "test:e2e") $frontendRoot
}

if ($Package) {
    $buildScript = Join-Path $PSScriptRoot "build-release.ps1"
    $packageArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $buildScript, "-SkipTests")
    if ($Offline) {
        $packageArguments += "-Offline"
    }
    Invoke-Checked "Windows release package" "powershell" $packageArguments $repoRoot
}

Write-Host ("Release checks passed for Bloomery " + $tauriVersion + ".")
