[CmdletBinding()]
param(
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$frontendRoot = Join-Path $repoRoot "frontend"
$rustRoot = Join-Path $repoRoot "src-tauri"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    Write-Host ("==> " + $Name)
    Push-Location $WorkingDirectory
    try {
        & $File @Arguments
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw ("{0} failed with exit code {1}" -f $Name, $exitCode)
        }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path -LiteralPath (Join-Path $rustRoot "Cargo.toml") -PathType Leaf)) {
    throw "src-tauri/Cargo.toml is missing"
}
if (-not (Test-Path -LiteralPath (Join-Path $frontendRoot "package.json") -PathType Leaf)) {
    throw "frontend/package.json is missing"
}

$sourceRoots = @(
    (Join-Path $repoRoot "frontend\src"),
    (Join-Path $repoRoot "src-tauri\src"),
    (Join-Path $repoRoot "domain-packs")
)
$secretPatterns = @(
    '47\.93\.203\.36',
    '43\.155\.210\.216',
    'https?://[^\s"'']*steel-agent[^\s"'']*',
    '-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
    '\b(?:sk|rk)-[A-Za-z0-9_-]{20,}\b',
    '\bgh[pousr]_[A-Za-z0-9_]{20,}\b',
    '\bAIza[0-9A-Za-z_-]{30,}\b'
)

$matches = @()
foreach ($sourceRoot in $sourceRoots) {
    if (-not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
        continue
    }
    $matches += Get-ChildItem -LiteralPath $sourceRoot -Recurse -File | Where-Object {
        $_.FullName -notmatch '\\node_modules\\|\\target\\|\\dist\\|\\test-results\\'
    } | Select-String -Pattern $secretPatterns -AllMatches
}
if ($matches.Count -gt 0) {
    $locations = ($matches | ForEach-Object { $_.Path + ":" + $_.LineNumber }) -join ", "
    throw "Potential private endpoint or credential material found in release source: $locations"
}

Invoke-Checked "Frontend security boundary scan" "npm" @("run", "test:boundaries") $frontendRoot

# 前端渲染安全回归（Vitest）：确保 AnswerRenderer 对 javascript:/data:/HTML 注入
# 与 proxyImg scheme 白名单的防护失效时，安全门禁以非零退出码失败。
Invoke-Checked "Frontend security tests" "npm" @("run", "test", "--", "src/components/answer/__tests__/AnswerRenderer.security.test.tsx") $frontendRoot

$securityTests = @(
    "architecture",
    "backup",
    "domain_commands",
    "domains",
    "http_redaction",
    "mcp_transports",
    "permission_paths",
    "permission_repository",
    "permissions",
    "providers",
    "rag_fts",
    "rag_mineru",
    "rag_parse",
    "secret_scan",
    "tools"
)
foreach ($testName in $securityTests) {
    $cargoArguments = @("test")
    if ($Offline) {
        $cargoArguments += "--offline"
    }
    $cargoArguments += @("--test", $testName, "--", "--test-threads=1")
    Invoke-Checked ("Rust security tests: " + $testName) "cargo" $cargoArguments $rustRoot
}

Write-Host "Security checks passed."
