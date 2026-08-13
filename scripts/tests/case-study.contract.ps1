[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$caseStudyScript = Join-Path $repoRoot "scripts\case-study.ps1"

if (-not (Test-Path -LiteralPath $caseStudyScript -PathType Leaf)) {
    throw "scripts/case-study.ps1 is missing"
}

$content = Get-Content -LiteralPath $caseStudyScript -Raw
foreach ($requiredText in @(
    '[switch]$Offline',
    'src-tauri',
    'compute-worker',
    'steel_datasets',
    'scheduler_runs_prediction',
    'scheduler_runs_optimization',
    'scheduler_exports_onnx',
    'steel_evaluations',
    'test_training.py',
    'test_sklearn_training.py',
    'test_optimization.py',
    'test_onnx_export.py',
    'test_evaluations.py',
    'Invoke-Checked',
    'LASTEXITCODE'
)) {
    if ($content -notmatch [regex]::Escape($requiredText)) {
        throw "scripts/case-study.ps1 is missing required coverage: $requiredText"
    }
}

Write-Output "Case-study script contract passed."
