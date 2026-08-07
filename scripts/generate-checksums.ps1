[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [string]$OutputFile = "SHA256SUMS.txt"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$resolvedInput = (Resolve-Path -LiteralPath $InputPath -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $resolvedInput -PathType Container)) {
    throw "Checksum input must be a directory: $InputPath"
}

$artifacts = Get-ChildItem -LiteralPath $resolvedInput -File | Where-Object {
    $_.Name -ne $OutputFile
} | Sort-Object Name
if ($artifacts.Count -eq 0) {
    throw "No release artifacts found in $resolvedInput"
}

$lines = foreach ($artifact in $artifacts) {
    $hash = Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256
    "{0} *{1}" -f $hash.Hash.ToLowerInvariant(), $artifact.Name
}

$outputPath = Join-Path $resolvedInput $OutputFile
[System.IO.File]::WriteAllLines($outputPath, [string[]]$lines, [System.Text.UTF8Encoding]::new($false))
Write-Host ("Wrote " + $outputPath)
