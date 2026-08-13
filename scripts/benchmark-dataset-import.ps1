[CmdletBinding()]
param(
    [string]$OutputPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $tauriRoot "target/dataset-import-benchmark.json"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $repoRoot $OutputPath
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null

$hadPreviousOutput = Test-Path Env:BLOOMERY_BENCHMARK_OUTPUT
$previousOutput = $env:BLOOMERY_BENCHMARK_OUTPUT
try {
    $env:BLOOMERY_BENCHMARK_OUTPUT = $OutputPath
    Push-Location $tauriRoot
    try {
        & cargo bench -j 1 --offline --bench dataset_import
        if ($LASTEXITCODE -ne 0) {
            throw "Dataset import benchmark failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} finally {
    if ($hadPreviousOutput) {
        $env:BLOOMERY_BENCHMARK_OUTPUT = $previousOutput
    } else {
        Remove-Item Env:BLOOMERY_BENCHMARK_OUTPUT -ErrorAction SilentlyContinue
    }
}

if (-not (Test-Path -LiteralPath $OutputPath)) {
    throw "Dataset import benchmark did not write $OutputPath"
}
$result = Get-Content -Raw -LiteralPath $OutputPath | ConvertFrom-Json
if (-not $result.gate.passed) {
    throw "Dataset import gate failed: rows=$($result.preview_rows), preview_p95_ms=$($result.preview.p95_ms), working_set_mb=$($result.memory.gate_working_set_mb)"
}

[pscustomobject]@{
    SourceRows = $result.source_rows
    PreviewRows = $result.preview_rows
    Columns = $result.preview_columns
    PreviewP95Ms = $result.preview.p95_ms
    WorkingSetMb = $result.memory.gate_working_set_mb
    Output = $OutputPath
} | Format-List
