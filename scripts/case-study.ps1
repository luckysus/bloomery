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
$caseStudyRoot = Join-Path $repoRoot "case-studies\steel-release"
$caseStudyProvenancePath = Join-Path $caseStudyRoot "provenance.json"
$caseStudyDatasetPath = Join-Path $caseStudyRoot "data\steel-demo.csv"
$report = if ($ReportPath) {
    $ReportPath
} else {
    Join-Path $repoRoot "artifacts\case-study\steel-case-study.json"
}
$results = [System.Collections.Generic.List[object]]::new()
$dataProvenance = $null
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
        data_provenance = $dataProvenance
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
        $workerPython,
        $caseStudyProvenancePath,
        $caseStudyDatasetPath
    )) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Case-study prerequisite is missing: $requiredPath"
        }
    }

    $provenance = Get-Content -LiteralPath $caseStudyProvenancePath -Raw | ConvertFrom-Json
    if ([string]$provenance.schema_version -ne "1.0.0" -or
        [string]$provenance.case_study -ne "steel-release") {
        throw "Case-study provenance schema or identifier is unsupported"
    }
    if ([string]$provenance.license -ne "Apache-2.0") {
        throw "Case-study provenance must declare the Apache-2.0 license"
    }
    if (-not [bool]$provenance.synthetic) {
        throw "Case-study data must be explicitly marked synthetic"
    }
    if ([bool]$provenance.restricted_text_redistributed) {
        throw "Case-study data must not redistribute restricted standards text"
    }
    if ([string]$provenance.dataset.path -ne "data/steel-demo.csv") {
        throw "Case-study provenance points to an unexpected dataset path"
    }
    $sourceLedgerRelativePath = [string]$provenance.provenance.source_ledger
    if (-not $sourceLedgerRelativePath) {
        throw "Case-study provenance must identify a source ledger"
    }
    $sourceLedgerPath = Join-Path $caseStudyRoot ($sourceLedgerRelativePath -replace "/", "\")
    $resolvedRepoRoot = (Resolve-Path -LiteralPath $repoRoot).Path.TrimEnd("\") + "\"
    $resolvedSourceLedgerPath = (Resolve-Path -LiteralPath $sourceLedgerPath).Path
    if (-not $resolvedSourceLedgerPath.StartsWith(
            $resolvedRepoRoot,
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        throw "Case-study source ledger must remain inside the repository"
    }
    $sourceLedger = Get-Content -LiteralPath $resolvedSourceLedgerPath -Raw | ConvertFrom-Json
    if ([string]$sourceLedger.schema_version -ne "1.0.0" -or
        [string]$sourceLedger.license -ne "Apache-2.0" -or
        -not [string]$sourceLedger.policy) {
        throw "Case-study source ledger schema or license policy is unsupported"
    }
    if (@($sourceLedger.entries) | Where-Object {
            [bool]$_.restricted_text_redistributed
        }) {
        throw "Case-study source ledger contains redistributed restricted text"
    }
    $datasetHash = (Get-FileHash -LiteralPath $caseStudyDatasetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([string]$provenance.dataset.sha256 -ne $datasetHash) {
        throw "Case-study dataset SHA-256 does not match provenance.json"
    }
    $sourceLedgerHash = (Get-FileHash -LiteralPath $resolvedSourceLedgerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $dataProvenance = [ordered]@{
        manifest = "case-studies/steel-release/provenance.json"
        dataset = "case-studies/steel-release/data/steel-demo.csv"
        source_ledger = $sourceLedgerRelativePath
        license = [string]$provenance.license
        synthetic = [bool]$provenance.synthetic
        restricted_text_redistributed = [bool]$provenance.restricted_text_redistributed
        sha256 = $datasetHash
        source_ledger_sha256 = $sourceLedgerHash
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
