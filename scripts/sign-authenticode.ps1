[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]]$Path
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$pfxBase64 = [string]$env:BLOOMERY_AUTHENTICODE_PFX_BASE64
$pfxPassword = [string]$env:BLOOMERY_AUTHENTICODE_PFX_PASSWORD
$timestampUrl = [string]$env:BLOOMERY_AUTHENTICODE_TIMESTAMP_URL

if ([string]::IsNullOrWhiteSpace($pfxBase64)) {
    throw "BLOOMERY_AUTHENTICODE_PFX_BASE64 is required for an Authenticode release"
}
if ([string]::IsNullOrWhiteSpace($pfxPassword)) {
    throw "BLOOMERY_AUTHENTICODE_PFX_PASSWORD is required for an Authenticode release"
}
if ([string]::IsNullOrWhiteSpace($timestampUrl)) {
    throw "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL is required for an Authenticode release"
}

$timestampUri = $null
if (-not [Uri]::TryCreate($timestampUrl, [UriKind]::Absolute, [ref]$timestampUri) -or
    $timestampUri.Scheme -notin @("http", "https")) {
    throw "BLOOMERY_AUTHENTICODE_TIMESTAMP_URL must be an absolute HTTP(S) URL"
}

try {
    $pfxBytes = [Convert]::FromBase64String($pfxBase64)
}
catch {
    throw "BLOOMERY_AUTHENTICODE_PFX_BASE64 is not valid base64"
}
if ($pfxBytes.Length -eq 0) {
    throw "BLOOMERY_AUTHENTICODE_PFX_BASE64 decoded to an empty certificate"
}

$signTool = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
if ($null -eq $signTool) {
    $sdkRoots = @(
        (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"),
        (Join-Path $env:ProgramFiles "Windows Kits\10\bin")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_ -PathType Container) }
    $signToolPath = $sdkRoots |
        ForEach-Object { Get-ChildItem -LiteralPath $_ -Filter "signtool.exe" -File -Recurse -ErrorAction SilentlyContinue } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if ($null -eq $signToolPath) {
        throw "signtool.exe was not found in PATH or the installed Windows SDK"
    }
    $signTool = $signToolPath
}

$resolvedPaths = @(
    foreach ($candidate in $Path) {
        $resolved = Get-Item -LiteralPath ([System.IO.Path]::GetFullPath($candidate)) -ErrorAction Stop
        if ($resolved.PSIsContainer) {
            throw "Authenticode signing requires file paths, not directories: $($resolved.FullName)"
        }
        $resolved
    }
)
if ($resolvedPaths.Count -eq 0) {
    throw "At least one Authenticode target is required"
}

$signablePaths = @($resolvedPaths | Where-Object {
    $_.Extension.ToLowerInvariant() -in @(".exe", ".dll", ".msi")
})
if ($signablePaths.Count -eq 0) {
    Write-Output ("Skipping non-signable target(s): " + (($resolvedPaths | ForEach-Object { $_.Name }) -join ", "))
    exit 0
}

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("bloomery-authenticode-" + [Guid]::NewGuid().ToString("N"))
$pfxPath = Join-Path $temporaryRoot "signing.pfx"
$certificate = $null
$certificateWasAlreadyPresent = $false
try {
    New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
    [System.IO.File]::WriteAllBytes($pfxPath, $pfxBytes)
    $existingThumbprints = @(
        Get-ChildItem -Path "Cert:\CurrentUser\My" -ErrorAction SilentlyContinue |
            ForEach-Object { [string]$_.Thumbprint }
    )
    $securePassword = ConvertTo-SecureString -String $pfxPassword -AsPlainText -Force
    $certificate = Import-PfxCertificate `
        -FilePath $pfxPath `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -Password $securePassword `
        -Exportable:$false
    if ($null -eq $certificate -or [string]::IsNullOrWhiteSpace($certificate.Thumbprint)) {
        throw "The Authenticode certificate could not be imported"
    }
    $certificateWasAlreadyPresent = $existingThumbprints -contains [string]$certificate.Thumbprint

    foreach ($target in $signablePaths) {
        Write-Output ("Signing " + $target.Name)
        $signToolPath = if ($signTool.PSObject.Properties["Source"]) {
            [string]$signTool.Source
        } else {
            [string]$signTool.FullName
        }
        & $signToolPath sign `
            /sha1 $certificate.Thumbprint `
            /fd SHA256 `
            /tr $timestampUrl `
            /td SHA256 `
            $target.FullName
        if ($LASTEXITCODE -ne 0) {
            throw "signtool failed for $($target.FullName) with exit code $LASTEXITCODE"
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $target.FullName
        if ($signature.Status -ne "Valid") {
            throw "Authenticode signature is not valid for $($target.FullName): $($signature.Status)"
        }
    }
}
finally {
    if ($certificate -and $certificate.Thumbprint -and -not $certificateWasAlreadyPresent) {
        Remove-Item -LiteralPath ("Cert:\CurrentUser\My\" + $certificate.Thumbprint) -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    Remove-Variable pfxBytes, pfxPassword, securePassword -ErrorAction SilentlyContinue
}

Write-Output ("Authenticode signing passed for " + $signablePaths.Count + " file(s).")
