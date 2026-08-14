[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArtifactDirectory,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$ReleaseBaseUrl,
    [string]$OutputPath,
    [string]$Notes = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$artifactRoot = [System.IO.Path]::GetFullPath($ArtifactDirectory)
if (-not (Test-Path -LiteralPath $artifactRoot -PathType Container)) {
    throw "Updater artifact directory is missing: $artifactRoot"
}
if ([string]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    throw "Version must be a semantic release version in MAJOR.MINOR.PATCH form"
}
$baseUri = $null
if (-not [Uri]::TryCreate($ReleaseBaseUrl, [UriKind]::Absolute, [ref]$baseUri) -or $baseUri.Scheme -ne "https") {
    throw "ReleaseBaseUrl must be an absolute HTTPS URL"
}
if ($baseUri.Host -match "^(localhost|127\.|10\.|192\.168\.|169\.254\.|172\.(1[6-9]|2[0-9]|3[0-1])\.|47\.93\.203\.36|43\.155\.210\.216$)") {
    throw "ReleaseBaseUrl must not target a local or private host"
}
$releasePath = $baseUri.AbsolutePath.TrimEnd("/")
if (-not $releasePath.EndsWith("/v" + $Version, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "ReleaseBaseUrl version must match the semantic release version"
}

function Select-SignedPackage {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][System.IO.FileInfo[]]$PrimaryPackages,
        [Parameter(Mandatory = $true)][System.IO.FileInfo[]]$FallbackPackages
    )

    $primary = @($PrimaryPackages)
    $fallback = @($FallbackPackages)
    $packages = @(if ($primary.Count -gt 0) { $primary } else { $fallback })
    if ($packages.Count -gt 1) {
        throw "Expected at most one $Label updater package, found $($packages.Count)"
    }
    if ($packages.Count -eq 0) {
        return $null
    }

    $package = $packages[0]
    $signaturePath = $package.FullName + ".sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        throw "Updater signature is missing for $($package.Name)"
    }
    $signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($signature)) {
        throw "Updater signature is empty for $($package.Name)"
    }

    return [ordered]@{
        package = $package
        signature = $signature
    }
}

function New-PlatformEntry {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$SelectedPackage,
        [Parameter(Mandatory = $true)][string]$ReleaseBase
    )

    return [ordered]@{
        signature = [string]$SelectedPackage.signature
        url = $ReleaseBase.TrimEnd("/") + "/" + $SelectedPackage.package.Name
    }
}

$nsisPackage = Select-SignedPackage `
    -Label "NSIS" `
    -PrimaryPackages @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*-setup.exe") `
    -FallbackPackages @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*.nsis.zip")
$msiPackage = Select-SignedPackage `
    -Label "MSI" `
    -PrimaryPackages @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*.msi") `
    -FallbackPackages @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*.msi.zip")
if ($null -eq $nsisPackage -and $null -eq $msiPackage) {
    throw "No signed Windows updater package found"
}

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $artifactRoot "latest.json"
}
$output = [System.IO.Path]::GetFullPath($OutputPath)
$manifest = [ordered]@{
    version = $Version
    notes = $Notes
    pub_date = [DateTime]::UtcNow.ToString("o")
    platforms = [ordered]@{}
}
if ($null -ne $nsisPackage) {
    $manifest.platforms."windows-x86_64-nsis" = New-PlatformEntry -SelectedPackage $nsisPackage -ReleaseBase $ReleaseBaseUrl
    $manifest.platforms."windows-x86_64" = New-PlatformEntry -SelectedPackage $nsisPackage -ReleaseBase $ReleaseBaseUrl
}
if ($null -ne $msiPackage) {
    $manifest.platforms."windows-x86_64-msi" = New-PlatformEntry -SelectedPackage $msiPackage -ReleaseBase $ReleaseBaseUrl
    if ($null -eq $nsisPackage) {
        $manifest.platforms."windows-x86_64" = New-PlatformEntry -SelectedPackage $msiPackage -ReleaseBase $ReleaseBaseUrl
    }
}
[System.IO.File]::WriteAllText(
    $output,
    ($manifest | ConvertTo-Json -Depth 8),
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output ("Updater manifest written to " + $output)
