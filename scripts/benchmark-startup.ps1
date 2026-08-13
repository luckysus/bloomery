[CmdletBinding()]
param(
    [string]$OutputPath = "",
    [int]$Rounds = 5,
    [int]$IdleSamples = 10,
    [int]$IdleSampleIntervalMilliseconds = 100,
    [int]$StartupTimeoutMilliseconds = 15000,
    [int]$IdleSettleMilliseconds = 3000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "Windows startup benchmark requires Windows"
}
if ($Rounds -lt 3 -or $IdleSamples -lt 3) {
    throw "Rounds and IdleSamples must be at least 3"
}
if ($StartupTimeoutMilliseconds -le 0 -or $IdleSettleMilliseconds -lt 0) {
    throw "Startup and idle timing limits must be positive"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"
$binaryPath = Join-Path $tauriRoot "target\release\bloomery.exe"
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $tauriRoot "target\startup-performance-benchmark.json"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $repoRoot $OutputPath
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null

Push-Location $tauriRoot
try {
    & cargo build --release --offline --bin bloomery
    if ($LASTEXITCODE -ne 0) {
        throw "Release binary build failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Release binary is missing: $binaryPath"
}

$startupP95MaximumMilliseconds = 3000.0
$idleWorkingSetMaximumMegabytes = 300.0
$binaryHash = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
$startupSamples = New-Object System.Collections.Generic.List[double]
$idleMemorySamples = New-Object System.Collections.Generic.List[double]

function Get-Percentile {
    param(
        [Parameter(Mandatory = $true)][double[]]$Values,
        [Parameter(Mandatory = $true)][double]$Percentile
    )
    if ($Values.Count -eq 0) {
        return 0.0
    }
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Min(
        $sorted.Count - 1,
        [Math]::Max(0, [int][Math]::Ceiling($sorted.Count * $Percentile) - 1)
    )
    return [double]$sorted[$index]
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$ProcessId)

    $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $ProcessId" -ErrorAction SilentlyContinue)
    foreach ($child in $children) {
        Stop-ProcessTree -ProcessId ([int]$child.ProcessId)
    }
    Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
}

function Get-WorkingSetMegabytes {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $Process.Refresh()
    if ($Process.HasExited) {
        throw "Bloomery exited before idle memory sampling"
    }
    return [double]$Process.WorkingSet64 / 1MB
}

$previousAppData = $env:APPDATA
$previousLocalAppData = $env:LOCALAPPDATA
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("bloomery-startup-" + [Guid]::NewGuid().ToString("N"))

try {
    if (@(Get-Process -Name "bloomery" -ErrorAction SilentlyContinue).Count -gt 0) {
        throw "Bloomery is already running; close it before the startup benchmark"
    }

    for ($round = 0; $round -lt $Rounds; $round++) {
        $roundRoot = Join-Path $temporaryRoot ("round-" + $round)
        New-Item -ItemType Directory -Force -Path $roundRoot | Out-Null
        $env:APPDATA = Join-Path $roundRoot "roaming"
        $env:LOCALAPPDATA = Join-Path $roundRoot "local"
        $env:TEMP = Join-Path $roundRoot "temp"
        $env:TMP = $env:TEMP
        New-Item -ItemType Directory -Force -Path $env:APPDATA, $env:LOCALAPPDATA, $env:TEMP | Out-Null

        $started = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $binaryPath -WorkingDirectory $tauriRoot -PassThru
        $ready = $false
        try {
            while ($started.ElapsedMilliseconds -lt $StartupTimeoutMilliseconds) {
                $process.Refresh()
                if ($process.HasExited) {
                    throw "Bloomery exited during startup with code $($process.ExitCode)"
                }
                if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                    $ready = $true
                    break
                }
                Start-Sleep -Milliseconds 50
            }
            if (-not $ready) {
                throw "Bloomery did not expose a main window within $StartupTimeoutMilliseconds ms"
            }
            $startupSamples.Add([double]$started.Elapsed.TotalMilliseconds)

            if ($IdleSettleMilliseconds -gt 0) {
                Start-Sleep -Milliseconds $IdleSettleMilliseconds
            }
            $roundMemory = New-Object System.Collections.Generic.List[double]
            for ($sample = 0; $sample -lt $IdleSamples; $sample++) {
                $roundMemory.Add((Get-WorkingSetMegabytes -Process $process))
                if ($sample -lt $IdleSamples - 1) {
                    Start-Sleep -Milliseconds $IdleSampleIntervalMilliseconds
                }
            }
            $idleMemorySamples.Add((Get-Percentile -Values ([double[]]$roundMemory.ToArray()) -Percentile 0.50))
        } finally {
            if ($null -ne $process) {
                try {
                    $process.Refresh()
                    if (-not $process.HasExited) {
                        Stop-ProcessTree -ProcessId $process.Id
                    }
                } catch {
                    # Cleanup must not hide the measured startup failure.
                }
            }
        }
        Remove-Item -LiteralPath $roundRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
} finally {
    $env:APPDATA = $previousAppData
    $env:LOCALAPPDATA = $previousLocalAppData
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$startupRaw = [double[]]$startupSamples.ToArray()
$memoryRaw = [double[]]$idleMemorySamples.ToArray()
$startupP50 = Get-Percentile -Values $startupRaw -Percentile 0.50
$startupP95 = Get-Percentile -Values $startupRaw -Percentile 0.95
$memoryP50 = Get-Percentile -Values $memoryRaw -Percentile 0.50
$memoryP95 = Get-Percentile -Values $memoryRaw -Percentile 0.95
$passed = $startupP95 -le $startupP95MaximumMilliseconds -and
    $memoryP95 -le $idleWorkingSetMaximumMegabytes

$report = [ordered]@{
    schema_version = 1
    binary_sha256 = $binaryHash
    rounds = $Rounds
    idle_samples_per_round = $IdleSamples
    reference_machine = [ordered]@{
        os = "windows"
        architecture = [Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
        logical_cpus = [Environment]::ProcessorCount
        processor = [Environment]::GetEnvironmentVariable("PROCESSOR_IDENTIFIER")
    }
    startup = [ordered]@{
        samples = $startupRaw.Count
        p50_ms = $startupP50
        p95_ms = $startupP95
        max_ms = ($startupRaw | Measure-Object -Maximum).Maximum
        raw_ms = $startupRaw
        readiness = "main_window_handle"
    }
    idle_memory = [ordered]@{
        samples = $memoryRaw.Count
        p50_mb = $memoryP50
        p95_mb = $memoryP95
        max_mb = ($memoryRaw | Measure-Object -Maximum).Maximum
        raw_mb = $memoryRaw
        sample_definition = "median working set after a 3-second settle period per fresh temporary profile"
    }
    gate = [ordered]@{
        startup_p95_ms_maximum = $startupP95MaximumMilliseconds
        idle_working_set_p95_mb_maximum = $idleWorkingSetMaximumMegabytes
        passed = $passed
    }
}

$json = $report | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($OutputPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Output $json
if (-not $passed) {
    throw "Startup performance gate failed: p95=$startupP95 ms, idle_memory_p95=$memoryP95 MB"
}
Write-Output ("Output: " + $OutputPath)
