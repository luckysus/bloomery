[CmdletBinding()]
param(
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $tauriRoot "target/retrieval-benchmark.json"
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
        & cargo bench -j 1 --offline --bench retrieval
        if ($LASTEXITCODE -ne 0) {
            throw "Retrieval benchmark failed with exit code $LASTEXITCODE"
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
    throw "Retrieval benchmark did not write $OutputPath"
}
$result = Get-Content -Raw -LiteralPath $OutputPath | ConvertFrom-Json
if (-not $result.gate.passed) {
    throw "Retrieval gate failed: recall=$($result.recall.minimum), total_p95_ms=$($result.total.p95_ms)"
}

[pscustomobject]@{
    CorpusChunks = $result.corpus_chunks
    MinimumRecall = $result.recall.minimum
    FtsP95Ms = $result.fts.p95_ms
    HnswP95Ms = $result.hnsw.p95_ms
    FusionP95Ms = $result.fusion.p95_ms
    TotalP95Ms = $result.total.p95_ms
    Output = $OutputPath
} | Format-List
