param(
    [Parameter(Position = 0)]
    [ValidateSet("dev", "build", "preview")]
    [string]$Mode = "dev",

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ExtraArgs
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$toolsRoot = Join-Path $projectRoot ".tools"
$nodeVersion = "22.22.2"
$nodeFolderName = "node-v$nodeVersion-win-x64"
$nodeRoot = Join-Path $toolsRoot $nodeFolderName
$nodeExe = Join-Path $nodeRoot "node.exe"
$nodeZip = Join-Path $toolsRoot "$nodeFolderName.zip"
$nodeLock = Join-Path $toolsRoot "$nodeFolderName.lock"
$nodeUrl = "https://nodejs.org/dist/v$nodeVersion/$nodeFolderName.zip"
$viteBin = Join-Path $projectRoot "node_modules\vite\bin\vite.js"
$tscBin = Join-Path $projectRoot "node_modules\typescript\bin\tsc"

function Ensure-PortableNode {
    if (Test-Path $nodeExe) {
        return
    }

    New-Item -ItemType Directory -Force -Path $toolsRoot | Out-Null
    $lockStream = $null

    try {
        for ($attempt = 0; $attempt -lt 120; $attempt++) {
            if (Test-Path $nodeExe) {
                return
            }

            try {
                $lockStream = [System.IO.File]::Open(
                    $nodeLock,
                    [System.IO.FileMode]::OpenOrCreate,
                    [System.IO.FileAccess]::ReadWrite,
                    [System.IO.FileShare]::None
                )
                break
            } catch {
                Start-Sleep -Seconds 1
            }
        }

        if (-not $lockStream) {
            throw "[vite-runner] Timed out waiting for the portable Node.js lock: $nodeLock"
        }

        if (-not (Test-Path $nodeZip)) {
            Write-Host "[vite-runner] Downloading Node.js $nodeVersion for Windows x64..."
            Invoke-WebRequest -UseBasicParsing -Uri $nodeUrl -OutFile $nodeZip
        } else {
            Write-Host "[vite-runner] Reusing downloaded Node.js archive: $nodeZip"
        }

        if (Test-Path $nodeRoot) {
            Remove-Item -LiteralPath $nodeRoot -Recurse -Force
        }

        Write-Host "[vite-runner] Extracting portable Node.js runtime..."
        Expand-Archive -LiteralPath $nodeZip -DestinationPath $toolsRoot -Force

        if (-not (Test-Path $nodeExe)) {
            throw "[vite-runner] Portable Node.js extraction failed: $nodeExe"
        }
    } finally {
        if ($lockStream) {
            $lockStream.Dispose()
        }

        Remove-Item -LiteralPath $nodeLock -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-PortableNode {
    param(
        [Parameter(Mandatory = $true)]
        [string]$EntryFile,

        [string[]]$Arguments = @()
    )

    & $nodeExe $EntryFile @Arguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

if ($env:OS -eq "Windows_NT" -and $PSVersionTable.PSVersion.Major -lt 7) {
    Write-Host "[vite-runner] PowerShell $($PSVersionTable.PSVersion) detected."
}

if ($env:npm_node_execpath) {
    try {
        $currentNodeVersion = & $env:npm_node_execpath -p "process.versions.node"
        if ($currentNodeVersion) {
            Write-Host "[vite-runner] Current npm host Node: $currentNodeVersion"
        }
    } catch {
    }
}

Ensure-PortableNode

if (-not (Test-Path $viteBin)) {
    throw "[vite-runner] Missing Vite entry file: $viteBin"
}

if ($Mode -eq "build") {
    if (-not (Test-Path $tscBin)) {
        throw "[vite-runner] Missing TypeScript entry file: $tscBin"
    }

    Write-Host "[vite-runner] Running TypeScript build with portable Node.js..."
    Invoke-PortableNode -EntryFile $tscBin -Arguments @("-b")
}

$viteArgs = @()
if ($Mode -ne "dev") {
    $viteArgs += $Mode
}

if ($ExtraArgs) {
    $viteArgs += $ExtraArgs
}

Write-Host "[vite-runner] Running Vite with portable Node.js $nodeVersion..."
Invoke-PortableNode -EntryFile $viteBin -Arguments $viteArgs
