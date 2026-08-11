[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$SkipTests,
    [string]$OutputDirectory = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$workerRoot = $PSScriptRoot
$distRoot = if ($OutputDirectory) { $OutputDirectory } else { Join-Path $workerRoot "dist" }
New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

function Invoke-Checked {
    param([string]$Name, [scriptblock]$Invocation)
    Write-Output "==> $Name"
    Push-Location $workerRoot
    try {
        & $Invocation
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode"
    }
}

Invoke-Checked "Locked isolated environment" {
    if ($Offline) {
        uv sync --frozen --extra packaging --extra test --offline
    }
    else {
        uv sync --frozen --extra packaging --extra test
    }
}

$venvPython = Join-Path $workerRoot ".venv\Scripts\python.exe"
if (-not (Test-Path -LiteralPath $venvPython -PathType Leaf)) {
    throw "Packaging virtual environment is missing: $venvPython"
}

if (-not $SkipTests) {
    Invoke-Checked "Worker tests in locked environment" {
        & $venvPython -m pytest -q
    }
}

Invoke-Checked "PyInstaller single-file worker" {
    & $venvPython -m PyInstaller --onefile --noconfirm `
        --name bloomery-compute-worker `
        --distpath $distRoot `
        --workpath (Join-Path $workerRoot "build") `
        --specpath $workerRoot `
        (Join-Path $workerRoot "worker_entry.py")
}

$executable = Join-Path $distRoot "bloomery-compute-worker.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Packaged worker executable is missing: $executable"
}
$executableHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLower()

$versions = & $venvPython -c "import json,sys; import importlib.metadata as md; print(json.dumps({'python': sys.version.split()[0], 'packages': sorted([{'name': d.metadata['Name'], 'version': d.version} for d in md.distributions()], key=lambda item: item['name'])}))"
if ($LASTEXITCODE -ne 0) {
    throw "Collecting locked package versions failed with exit code $LASTEXITCODE"
}
$versionInfo = $versions | ConvertFrom-Json

$workerVersion = & $venvPython -c "from bloomery_worker.worker import WORKER_VERSION; print(WORKER_VERSION)"
if ($LASTEXITCODE -ne 0) {
    throw "Reading worker version failed with exit code $LASTEXITCODE"
}

$manifest = [ordered]@{
    schema_version   = "1.0.0"
    artifact         = "bloomery-compute-worker"
    worker_version   = "$workerVersion"
    executable       = "bloomery-compute-worker.exe"
    sha256           = $executableHash
    python           = $versionInfo.python
    packages         = $versionInfo.packages
    signature        = "unsigned-explicit"
    signature_note   = "Artifact is intentionally unsigned in this build; release signing happens in the release-quality gate and unsigned artifacts must be clearly marked."
    private_urls     = @()
    generated_at     = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
}
$manifestPath = Join-Path $distRoot "worker-artifact-manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8

$sbom = [ordered]@{
    schema_version = "1.0.0"
    component      = "bloomery-compute-worker"
}
$components = @()
$components += @{ name = "bloomery-compute-worker"; version = "$workerVersion"; sha256 = $executableHash }
foreach ($package in $versionInfo.packages) {
    $components += @{ name = [string]$package.name; version = [string]$package.version }
}
$sbom["components"] = $components
$sbomPath = Join-Path $distRoot "worker-sbom.json"
$sbom | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $sbomPath -Encoding utf8

"$executableHash  bloomery-compute-worker.exe" | Set-Content -LiteralPath (Join-Path $distRoot "bloomery-compute-worker.sha256") -Encoding utf8

Write-Output "Worker artifact manifest written to $manifestPath"
Write-Output "Packaged worker build passed."
$global:LASTEXITCODE = 0
