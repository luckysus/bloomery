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
$installRoot = Join-Path $tempRoot "install"
$dataRoot = Join-Path $tempRoot "data"
$oldAppData = $env:APPDATA
$oldLocalAppData = $env:LOCALAPPDATA

try {
    New-Item -ItemType Directory -Path $installRoot, $dataRoot -Force | Out-Null
    $installProcess = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installRoot") -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Installer exited with code $($installProcess.ExitCode)"
    }

    $application = Get-ChildItem -LiteralPath $installRoot -Filter "Bloomery.exe" -File -Recurse | Select-Object -First 1
    if ($null -eq $application) {
        throw "Installed Bloomery.exe was not found under $installRoot"
    }

    $env:APPDATA = $dataRoot
    $env:LOCALAPPDATA = $dataRoot
    $applicationProcess = Start-Process -FilePath $application.FullName -WorkingDirectory $installRoot -PassThru
    Start-Sleep -Seconds 5
    if (-not $applicationProcess.HasExited) {
        Stop-Process -Id $applicationProcess.Id -Force
    }
    Set-Content -LiteralPath (Join-Path $dataRoot "retention-sentinel.txt") -Value "installer smoke" -Encoding UTF8

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
    if (-not (Test-Path -LiteralPath (Join-Path $dataRoot "retention-sentinel.txt"))) {
        throw "Application data was removed by uninstall"
    }
    Write-Output "Installer smoke passed: install, launch, uninstall, and data retention."
}
finally {
    $env:APPDATA = $oldAppData
    $env:LOCALAPPDATA = $oldLocalAppData
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
