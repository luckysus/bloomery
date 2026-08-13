[CmdletBinding()]
param(
    [switch]$Offline,
    [string]$ReportPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$rustRoot = Join-Path $repoRoot "src-tauri"
$workerRoot = Join-Path $repoRoot "compute-worker"
$workerPython = Join-Path $workerRoot ".venv\Scripts\python.exe"
$report = if ($ReportPath) {
    $ReportPath
} else {
    Join-Path $repoRoot "artifacts\case-study\steel-case-study.json"
}
$results = [System.Collections.Generic.List[object]]::new()
$status = "passed"

function Write-Report {
    $reportDirectory = Split-Path -Parent $report
    if ($reportDirectory) {
        New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
    }
    [ordered]@{
        schema_version = "1.0.0"
        case_study = "steel-release"
        status = $status
        repository = "bloomery"
        commit = ((git -C $repoRoot rev-parse HEAD 2>$null) | Select-Object -First 1)
        generated_at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        host = [System.Environment]::OSVersion.VersionString
        powershell = $PSVersionTable.PSVersion.ToString()
        steps = @($results)
        secrets = "none"
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $report -Encoding utf8
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    Write-Host ("==> " + $Name)
    Push-Location $WorkingDirectory
    try {
        & $File @Arguments
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw ("{0} failed with exit code {1}" -f $Name, $exitCode)
        }
        $results.Add([ordered]@{
            name = $Name
            working_directory = $WorkingDirectory
            command = (($File) + " " + ($Arguments -join " "))
            status = "passed"
            duration_ms = $stopwatch.ElapsedMilliseconds
        })
    }
    catch {
        $results.Add([ordered]@{
            name = $Name
            working_directory = $WorkingDirectory
            command = (($File) + " " + ($Arguments -join " "))
            status = "failed"
            duration_ms = $stopwatch.ElapsedMilliseconds
            error = $_.Exception.Message
        })
        throw
    }
    finally {
        Pop-Location
    }
}

try {
    foreach ($requiredPath in @(
        (Join-Path $rustRoot "Cargo.toml"),
        (Join-Path $workerRoot "pyproject.toml"),
        $workerPython
    )) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Case-study prerequisite is missing: $requiredPath"
        }
    }

    $cargoPrefix = @()
    if ($Offline) {
        $cargoPrefix += "--offline"
    }

    Invoke-Checked "Steel dataset import" "cargo" `
        (@("test") + $cargoPrefix + @("--test", "steel_datasets", "--", "--test-threads=1")) $rustRoot
    Invoke-Checked "Steel prediction with applicability" "cargo" `
        (@("test") + $cargoPrefix + @("--test", "compute_task", "scheduler_runs_prediction", "--", "--test-threads=1")) $rustRoot
    Invoke-Checked "Constrained steel optimization" "cargo" `
        (@("test") + $cargoPrefix + @("--test", "compute_task", "scheduler_runs_optimization", "--", "--test-threads=1")) $rustRoot
    Invoke-Checked "ONNX parity" "cargo" `
        (@("test") + $cargoPrefix + @("--test", "compute_task", "scheduler_exports_onnx", "--", "--test-threads=1")) $rustRoot
    Invoke-Checked "Rust steel evaluation" "cargo" `
        (@("test") + $cargoPrefix + @("--test", "steel_evaluations", "--", "--test-threads=1")) $rustRoot

    Invoke-Checked "Deterministic Worker training" $workerPython `
        @("-m", "pytest", "tests/test_training.py", "tests/test_sklearn_training.py", "-q") $workerRoot
    Invoke-Checked "Constrained Worker optimization" $workerPython `
        @("-m", "pytest", "tests/test_optimization.py", "-q") $workerRoot
    Invoke-Checked "Worker ONNX export" $workerPython `
        @("-m", "pytest", "tests/test_onnx_export.py", "-q") $workerRoot
    Invoke-Checked "Versioned steel evaluation" $workerPython `
        @("-m", "pytest", "tests/test_evaluations.py", "-q") $workerRoot
}
catch {
    $status = "failed"
    throw
}
finally {
    Write-Report
}

Write-Output ("Steel case study passed. Report: " + $report)
