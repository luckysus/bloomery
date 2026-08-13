[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$publicKey = [string]$env:BLOOMERY_UPDATER_PUBLIC_KEY
$endpoint = [string]$env:BLOOMERY_UPDATER_ENDPOINT
$authenticodePfx = [string]$env:BLOOMERY_AUTHENTICODE_PFX_BASE64
$authenticodePassword = [string]$env:BLOOMERY_AUTHENTICODE_PFX_PASSWORD
$authenticodeTimestamp = [string]$env:BLOOMERY_AUTHENTICODE_TIMESTAMP_URL
$authenticodeScript = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "sign-authenticode.ps1"))
if ([string]::IsNullOrWhiteSpace($publicKey)) {
    throw "BLOOMERY_UPDATER_PUBLIC_KEY is required for a signed release"
}
if ([string]::IsNullOrWhiteSpace($endpoint)) {
    throw "BLOOMERY_UPDATER_ENDPOINT is required for a signed release"
}
if ([string]::IsNullOrWhiteSpace($authenticodePfx)) {
    throw "BLOOMERY_AUTHENTICODE_PFX_BASE64 is required for a signed release"
}
if ([string]::IsNullOrWhiteSpace($authenticodePassword)) {
    throw "BLOOMERY_AUTHENTICODE_PFX_PASSWORD is required for a signed release"
}
if ([string]::IsNullOrWhiteSpace($authenticodeTimestamp)) {
    throw "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL is required for a signed release"
}
if (-not (Test-Path -LiteralPath $authenticodeScript -PathType Leaf)) {
    throw "Authenticode signing script is missing: $authenticodeScript"
}
if ($publicKey -match "\s") {
    throw "BLOOMERY_UPDATER_PUBLIC_KEY must be a single-line public key"
}

$uri = $null
if (-not [Uri]::TryCreate($endpoint, [UriKind]::Absolute, [ref]$uri) -or $uri.Scheme -ne "https") {
    throw "BLOOMERY_UPDATER_ENDPOINT must be an absolute HTTPS URL"
}
if ($uri.Host -match "^(localhost|127\.|10\.|192\.168\.|169\.254\.|172\.(1[6-9]|2[0-9]|3[0-1])\.|47\.93\.203\.36|43\.155\.210\.216$)") {
    throw "BLOOMERY_UPDATER_ENDPOINT must not target a local or private host"
}
$timestampUri = $null
if (-not [Uri]::TryCreate($authenticodeTimestamp, [UriKind]::Absolute, [ref]$timestampUri) -or
    $timestampUri.Scheme -notin @("http", "https")) {
    throw "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL must be an absolute HTTP(S) URL"
}

$output = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $output
if ($parent) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
$overlay = [ordered]@{
    bundle = [ordered]@{
        createUpdaterArtifacts = $true
        windows = [ordered]@{
            digestAlgorithm = "sha256"
            signCommand = [ordered]@{
                cmd = "powershell"
                args = @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", $authenticodeScript,
                    "-Path", "%1"
                )
            }
        }
    }
    plugins = [ordered]@{
        updater = [ordered]@{
            endpoints = @($endpoint)
            pubkey = $publicKey
        }
    }
}
[System.IO.File]::WriteAllText(
    $output,
    ($overlay | ConvertTo-Json -Depth 6),
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output ("Signed updater config written to " + $output)
