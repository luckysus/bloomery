[CmdletBinding()]
param(
    [string]$InstallerPath,
    [switch]$SkipMigrationTests,
    [switch]$AllowUnsigned,
    [switch]$RunInstallerSmoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$rustRoot = Join-Path $repoRoot "src-tauri"
$tauriConfigPath = Join-Path $rustRoot "tauri.conf.json"

function Wait-For-ApplicationReady {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$DatabasePath,
        [Parameter(Mandatory = $true)][string]$Phase,
        [int]$TimeoutSeconds = 30
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (-not (Test-Path -LiteralPath $DatabasePath -PathType Leaf)) {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "$Phase exited before creating its app-data database (exit code $($Process.ExitCode))"
        }
        if ([DateTime]::UtcNow -ge $deadline) {
            return $false
        }
        Start-Sleep -Milliseconds 250
    }

    $Process.Refresh()
    if ($Process.HasExited) {
        throw "$Phase did not stay alive after creating its app-data database (exit code $($Process.ExitCode))"
    }
    Start-Sleep -Milliseconds 250
    $Process.Refresh()
    if ($Process.HasExited) {
        throw "$Phase did not stay alive after creating its app-data database (exit code $($Process.ExitCode))"
    }
    return $true
}

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

if (-not $SkipMigrationTests) {
    Invoke-Checked "Migration and backup lifecycle tests" "cargo" @("test", "--test", "migrations", "--test", "backup") $rustRoot
}

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    if ($RunInstallerSmoke) {
        throw "-RunInstallerSmoke requires -InstallerPath"
    }
    Write-Output "Lifecycle checks passed without an installer artifact."
    exit 0
}

$installer = Get-Item -LiteralPath ([System.IO.Path]::GetFullPath($InstallerPath)) -ErrorAction Stop
if ($installer.Extension -ne ".exe" -or $installer.Length -le 0) {
    throw "Installer must be a non-empty .exe file: $($installer.FullName)"
}

$hash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$signature = Get-AuthenticodeSignature -LiteralPath $installer.FullName
if ($signature.Status -eq "NotSigned") {
    if (-not $AllowUnsigned) {
        throw "Installer is unsigned. Pass -AllowUnsigned only for engineering validation."
    }
    Write-Warning "Installer is unsigned; this is not a public release artifact."
} elseif ($signature.Status -ne "Valid") {
    throw "Installer signature status is $($signature.Status)"
}

Write-Output ("Installer verified: {0} ({1} bytes, sha256 {2})" -f $installer.Name, $installer.Length, $hash)

if (-not $RunInstallerSmoke) {
    Write-Output "Lifecycle checks passed. Installer smoke test was not requested."
    exit 0
}

if ($signature.Status -eq "NotSigned" -and -not $AllowUnsigned) {
    throw "-RunInstallerSmoke requires -AllowUnsigned for an unsigned engineering installer"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("bloomery-lifecycle-" + [guid]::NewGuid().ToString("N"))
$unicodeInstallDirectoryName = -join ([char[]](0x5B89, 0x88C5, 0x8DEF, 0x5F84))
$unicodeDataDirectoryName = -join ([char[]](0x7528, 0x6237, 0x6570, 0x636E))
$installRoot = Join-Path $tempRoot $unicodeInstallDirectoryName
$dataRoot = Join-Path $tempRoot $unicodeDataDirectoryName
$oldBloomeryDataDir = $env:BLOOMERY_DATA_DIR

try {
    if (-not (Test-Path -LiteralPath $tauriConfigPath -PathType Leaf)) {
        throw "Tauri configuration is missing: $tauriConfigPath"
    }
    $tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw | ConvertFrom-Json
    $identifier = [string]$tauriConfig.identifier
    if ([string]::IsNullOrWhiteSpace($identifier)) {
        throw "Tauri application identifier is missing"
    }

    New-Item -ItemType Directory -Path $installRoot, $dataRoot -Force | Out-Null
    $installProcess = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installRoot") -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Installer exited with code $($installProcess.ExitCode)"
    }

    $application = Get-ChildItem -LiteralPath $installRoot -Filter "Bloomery.exe" -File -Recurse | Select-Object -First 1
    if ($null -eq $application) {
        throw "Installed Bloomery.exe was not found under $installRoot"
    }

    $env:BLOOMERY_DATA_DIR = $dataRoot
    $applicationProcess = Start-Process -FilePath $application.FullName -WorkingDirectory $installRoot -PassThru
    $applicationDataDirectory = $dataRoot
    $databasePath = Join-Path $applicationDataDirectory "bloomery.sqlite3"
    if (-not (Wait-For-ApplicationReady -Process $applicationProcess -DatabasePath $databasePath -Phase "Installer smoke")) {
        if (-not $applicationProcess.HasExited) {
            Stop-Process -Id $applicationProcess.Id -Force
            $applicationProcess.WaitForExit(10000)
        }
        $exitCode = if ($applicationProcess.HasExited) { $applicationProcess.ExitCode } else { "unknown" }
        throw "Bloomery did not create its app-data database at $databasePath (exit code $exitCode)"
    }
    if (-not $applicationProcess.HasExited) {
        Stop-Process -Id $applicationProcess.Id -Force
        if (-not $applicationProcess.WaitForExit(10000)) {
            throw "Bloomery process did not exit after the lifecycle smoke stop request"
        }
    }
    $sentinelPath = Join-Path $applicationDataDirectory "retention-sentinel.txt"
    Set-Content -LiteralPath $sentinelPath -Value "installer smoke" -Encoding UTF8

    $uninstaller = Get-ChildItem -LiteralPath $installRoot -Filter "uninstall*.exe" -File -Recurse | Select-Object -First 1
    if ($null -eq $uninstaller) {
        throw "Tauri uninstaller was not found under $installRoot"
    }
    $uninstallProcess = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
    if ($uninstallProcess.ExitCode -ne 0) {
        throw "Uninstaller exited with code $($uninstallProcess.ExitCode)"
    }
    if (Test-Path -LiteralPath $application.FullName) {
        throw "Installed application still exists after uninstall"
    }
    if (-not (Test-Path -LiteralPath $databasePath -PathType Leaf)) {
        throw "Application database was removed by uninstall"
    }
    if (-not (Test-Path -LiteralPath $sentinelPath -PathType Leaf)) {
        throw "Application data was removed by uninstall"
    }
    Write-Output "Installer smoke passed: install, launch, app-data database creation, uninstall, Unicode path, and data retention."
}
finally {
    if ($null -eq $oldBloomeryDataDir) {
        Remove-Item Env:\BLOOMERY_DATA_DIR -ErrorAction SilentlyContinue
    } else {
        $env:BLOOMERY_DATA_DIR = $oldBloomeryDataDir
    }
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
