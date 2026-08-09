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
$baseUri = $null
if (-not [Uri]::TryCreate($ReleaseBaseUrl, [UriKind]::Absolute, [ref]$baseUri) -or $baseUri.Scheme -ne "https") {
    throw "ReleaseBaseUrl must be an absolute HTTPS URL"
}
if ($baseUri.Host -match "^(localhost|127\.|10\.|192\.168\.|169\.254\.|172\.(1[6-9]|2[0-9]|3[0-1])\.|47\.93\.203\.36|43\.155\.210\.216$)") {
    throw "ReleaseBaseUrl must not target a local or private host"
}

$zipPackages = @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*.nsis.zip")
$installerPackages = @(Get-ChildItem -LiteralPath $artifactRoot -File -Filter "*-setup.exe")
$packages = @($zipPackages) + @($installerPackages)
if ($packages.Count -ne 1) {
    throw "Expected exactly one NSIS updater package (*.nsis.zip or *-setup.exe), found $($packages.Count)"
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

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $artifactRoot "latest.json"
}
$output = [System.IO.Path]::GetFullPath($OutputPath)
$manifest = [ordered]@{
    version = $Version
    notes = $Notes
    pub_date = [DateTime]::UtcNow.ToString("o")
    platforms = [ordered]@{
        "windows-x86_64-nsis" = [ordered]@{
            signature = $signature
            url = $ReleaseBaseUrl.TrimEnd("/") + "/" + $package.Name
        }
    }
}
[System.IO.File]::WriteAllText(
    $output,
    ($manifest | ConvertTo-Json -Depth 8),
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output ("Updater manifest written to " + $output)
