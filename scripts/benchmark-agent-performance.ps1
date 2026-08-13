[CmdletBinding()]
param(
    [string]$OutputPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $tauriRoot "target/agent-performance-benchmark.json"
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
        & cargo bench -j 1 --offline --bench agent_performance
        if ($LASTEXITCODE -ne 0) {
            throw "Agent performance benchmark failed with exit code $LASTEXITCODE"
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
    throw "Agent performance benchmark did not write $OutputPath"
}
$result = Get-Content -Raw -LiteralPath $OutputPath | ConvertFrom-Json
if (-not $result.gate.passed) {
    throw "Agent performance gate failed: throughput=$($result.event_append.minimum_events_per_second), replay_p95_ms=$($result.conversation_replay.p95_ms), working_set_mb=$($result.memory.gate_working_set_mb)"
}

[pscustomobject]@{
    ConversationMessages = $result.conversation_messages
    EventCount = $result.event_count
    MinimumEventsPerSecond = $result.event_append.minimum_events_per_second
    ReplayP95Ms = $result.conversation_replay.p95_ms
    WorkingSetMb = $result.memory.gate_working_set_mb
    Output = $OutputPath
} | Format-List
