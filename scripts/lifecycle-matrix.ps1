[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OldInstallerPath,
    [Parameter(Mandatory = $true)][string]$NewInstallerPath,
    [switch]$SkipMigrationTests,
    [switch]$AllowUnsigned,
    [switch]$RunInstallerSmoke,
    [switch]$RunUpgradeDowngrade,
    [string]$ReportPath
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

function Assert-Installer {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = Get-Item -LiteralPath ([System.IO.Path]::GetFullPath($Path)) -ErrorAction Stop
    if ($resolved.Extension -ne ".exe" -or $resolved.Length -le 0) {
        throw "Installer must be a non-empty .exe file: $($resolved.FullName)"
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved.FullName
    if ($signature.Status -eq "NotSigned") {
        if (-not $AllowUnsigned) {
            throw "Installer is unsigned: $($resolved.Name). Pass -AllowUnsigned only for engineering validation."
        }
        Write-Warning ("Installer is unsigned: " + $resolved.Name)
    } elseif ($signature.Status -ne "Valid") {
        throw "Installer signature status is $($signature.Status) for $($resolved.Name)"
    }
    [pscustomobject]@{
        Path = $resolved.FullName
        Name = $resolved.Name
        Bytes = $resolved.Length
        Sha256 = (Get-FileHash -LiteralPath $resolved.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        Signature = [string]$signature.Status
        ProductVersion = [string]$resolved.VersionInfo.ProductVersion
    }
}

function Find-Application {
    param([Parameter(Mandatory = $true)][string]$InstallRoot)

    $application = Get-ChildItem -LiteralPath $InstallRoot -Filter "Bloomery.exe" -File -Recurse |
        Select-Object -First 1
    if ($null -eq $application) {
        throw "Installed Bloomery.exe was not found under $InstallRoot"
    }
    $application
}

function Find-Uninstaller {
    param([Parameter(Mandatory = $true)][string]$InstallRoot)

    $uninstaller = Get-ChildItem -LiteralPath $InstallRoot -Filter "uninstall*.exe" -File -Recurse |
        Select-Object -First 1
    if ($null -eq $uninstaller) {
        throw "Tauri uninstaller was not found under $InstallRoot"
    }
    $uninstaller
}

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
    $true
}

function Stop-Application {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    if (-not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
        if (-not $Process.WaitForExit(10000)) {
            throw "Bloomery process did not exit after the lifecycle stop request"
        }
    }
}

function Install-And-Launch {
    param(
        [Parameter(Mandatory = $true)][string]$Installer,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$DataRoot,
        [Parameter(Mandatory = $true)][string]$Phase
    )

    $installProcess = Start-Process -FilePath $Installer -ArgumentList @("/S", "/D=$InstallRoot") -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "$Phase installer exited with code $($installProcess.ExitCode)"
    }
    $application = Find-Application -InstallRoot $InstallRoot

    $env:BLOOMERY_DATA_DIR = $DataRoot
    $applicationProcess = Start-Process -FilePath $application.FullName -WorkingDirectory $InstallRoot -PassThru
    $databasePath = Join-Path $DataRoot "bloomery.sqlite3"
    if (-not (Wait-For-ApplicationReady -Process $applicationProcess -DatabasePath $databasePath -Phase $Phase)) {
        Stop-Application -Process $applicationProcess
        $exitCode = if ($applicationProcess.HasExited) { $applicationProcess.ExitCode } else { "unknown" }
        throw "$Phase did not create its app-data database at $databasePath (exit code $exitCode)"
    }
    Stop-Application -Process $applicationProcess
    $databaseHash = (Get-FileHash -LiteralPath $databasePath -Algorithm SHA256).Hash.ToLowerInvariant()

    [pscustomobject]@{
        Phase = $Phase
        Application = $application.FullName
        Database = $databasePath
        DatabaseBytes = (Get-Item -LiteralPath $databasePath).Length
        DatabaseSha256 = $databaseHash
        SentinelPresent = Test-Path -LiteralPath (Join-Path $DataRoot "retention-sentinel.txt") -PathType Leaf
    }
}

function Assert-DataPreserved {
    param(
        [Parameter(Mandatory = $true)][string]$DataRoot,
        [Parameter(Mandatory = $true)][string]$Phase
    )

    $databasePath = Join-Path $DataRoot "bloomery.sqlite3"
    $sentinelPath = Join-Path $DataRoot "retention-sentinel.txt"
    if (-not (Test-Path -LiteralPath $databasePath -PathType Leaf)) {
        throw "$Phase removed the application database"
    }
    if (-not (Test-Path -LiteralPath $sentinelPath -PathType Leaf)) {
        throw "$Phase removed the data-preservation sentinel"
    }
}

if (-not $RunInstallerSmoke -and -not $RunUpgradeDowngrade) {
    throw "Choose -RunInstallerSmoke, -RunUpgradeDowngrade, or both"
}
if (-not (Test-Path -LiteralPath $tauriConfigPath -PathType Leaf)) {
    throw "Tauri configuration is missing: $tauriConfigPath"
}
if (-not $SkipMigrationTests) {
    Invoke-Checked "Migration and backup lifecycle tests" "cargo" @(
        "test", "--test", "migrations", "--test", "backup", "--", "--test-threads=1"
    ) $rustRoot
}

$oldInstaller = Assert-Installer -Path $OldInstallerPath
$newInstaller = Assert-Installer -Path $NewInstallerPath
if ($RunUpgradeDowngrade -and $oldInstaller.Sha256 -eq $newInstaller.Sha256) {
    throw "Upgrade/downgrade matrix requires distinct old and new installer artifacts"
}
if ($RunUpgradeDowngrade -and (
    [string]::IsNullOrWhiteSpace($oldInstaller.ProductVersion) -or
    [string]::IsNullOrWhiteSpace($newInstaller.ProductVersion))) {
    throw "Upgrade/downgrade matrix requires product versions on both installer artifacts"
}
if ($RunUpgradeDowngrade -and $oldInstaller.ProductVersion -eq $newInstaller.ProductVersion) {
    throw "Upgrade/downgrade matrix requires distinct product versions"
}
$lifecycleRoot = Join-Path $repoRoot "artifacts\lifecycle-runs"
$tempRoot = Join-Path $lifecycleRoot ("bloomery-lifecycle-matrix-" + [guid]::NewGuid().ToString("N"))
$unicodeInstallDirectoryName = -join ([char[]](0x5B89, 0x88C5, 0x8DEF, 0x5F84))
$unicodeDataDirectoryName = -join ([char[]](0x7528, 0x6237, 0x6570, 0x636E))
$installRoot = Join-Path $tempRoot $unicodeInstallDirectoryName
$dataRoot = Join-Path $tempRoot $unicodeDataDirectoryName
$oldBloomeryDataDir = $env:BLOOMERY_DATA_DIR
$results = [System.Collections.Generic.List[object]]::new()

try {
    New-Item -ItemType Directory -Path $installRoot, $dataRoot -Force | Out-Null
    if ($RunInstallerSmoke) {
        [void]$results.Add((Install-And-Launch -Installer $newInstaller.Path -InstallRoot $installRoot -DataRoot $dataRoot `
            -Phase "fresh-install"))
        Set-Content -LiteralPath (Join-Path $dataRoot "retention-sentinel.txt") `
            -Value "installer smoke" -Encoding UTF8
        $uninstaller = Find-Uninstaller -InstallRoot $installRoot
        $uninstallProcess = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
        if ($uninstallProcess.ExitCode -ne 0) {
            throw "Fresh-install uninstaller exited with code $($uninstallProcess.ExitCode)"
        }
        Assert-DataPreserved -DataRoot $dataRoot -Phase "fresh-install uninstall"
        Write-Output "Installer smoke passed: install, launch, Unicode path, non-default data path, uninstall, and data-preservation."
    }

    if ($RunUpgradeDowngrade) {
        [void]$results.Add((Install-And-Launch -Installer $oldInstaller.Path -InstallRoot $installRoot -DataRoot $dataRoot `
            -Phase "old-install"))
        Set-Content -LiteralPath (Join-Path $dataRoot "retention-sentinel.txt") `
            -Value "upgrade-downgrade matrix" -Encoding UTF8
        [void]$results.Add((Install-And-Launch -Installer $newInstaller.Path -InstallRoot $installRoot -DataRoot $dataRoot `
            -Phase "upgrade"))
        Assert-DataPreserved -DataRoot $dataRoot -Phase "upgrade"
        $upgradedDatabaseHash = ($results | Where-Object Phase -eq "upgrade" | Select-Object -Last 1).DatabaseSha256
        [void]$results.Add((Install-And-Launch -Installer $oldInstaller.Path -InstallRoot $installRoot -DataRoot $dataRoot `
            -Phase "downgrade"))
        Assert-DataPreserved -DataRoot $dataRoot -Phase "downgrade"
        $downgradedDatabaseHash = ($results | Where-Object Phase -eq "downgrade" | Select-Object -Last 1).DatabaseSha256
        if ($downgradedDatabaseHash -ne $upgradedDatabaseHash) {
            throw "Downgrade attempt changed the database; expected read-only protection"
        }
        Write-Output "Upgrade/downgrade matrix passed: old-to-new-to-old launch and data-preservation."
    }

    if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $reportDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($ReportPath))
        if (-not [string]::IsNullOrWhiteSpace($reportDirectory)) {
            New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
        }
        [ordered]@{
            generated_at = [DateTime]::UtcNow.ToString("o")
            old_installer = $oldInstaller
            new_installer = $newInstaller
            run_installer_smoke = [bool]$RunInstallerSmoke
            run_upgrade_downgrade = [bool]$RunUpgradeDowngrade
            phases = @($results)
        } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
        Write-Output ("Lifecycle matrix report written to " + [System.IO.Path]::GetFullPath($ReportPath))
    }
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
