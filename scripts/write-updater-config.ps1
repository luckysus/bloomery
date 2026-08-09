[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$publicKey = [string]$env:BLOOMERY_UPDATER_PUBLIC_KEY
$endpoint = [string]$env:BLOOMERY_UPDATER_ENDPOINT
if ([string]::IsNullOrWhiteSpace($publicKey)) {
    throw "BLOOMERY_UPDATER_PUBLIC_KEY is required for a signed release"
}
if ([string]::IsNullOrWhiteSpace($endpoint)) {
    throw "BLOOMERY_UPDATER_ENDPOINT is required for a signed release"
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

$output = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $output
if ($parent) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
$overlay = [ordered]@{
    bundle = [ordered]@{
        createUpdaterArtifacts = $true
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
