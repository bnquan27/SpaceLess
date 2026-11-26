#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidatePattern('^[0-9a-f]{32}$')] [string] $HyperdriveId,
    [string] $CloudflareCli = 'cf',
    [string] $WorkerBuild = 'worker-build'
)

$ErrorActionPreference = 'Stop'
if ($env:SPACELESS_CF_PROBE_TOKEN.Length -lt 32) {
    throw 'Set SPACELESS_CF_PROBE_TOKEN to a random value of at least 32 characters.'
}

$workerName = 'spaceless-staging-probe'
$workspace = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("spaceless-probe-" + [Guid]::NewGuid().ToString('N')))
$secretPath = Join-Path $workspace.FullName 'secrets.json'
$output = Join-Path $workspace.FullName '.cloudflare/output/v1'
$workerOutput = Join-Path $output 'workers/default'
$bundle = Join-Path $workerOutput 'bundle'

Push-Location $PSScriptRoot
try {
    & $WorkerBuild --release
    if ($LASTEXITCODE -ne 0) { throw 'worker-build failed.' }
} finally {
    Pop-Location
}

$null = New-Item -ItemType Directory -Path $bundle -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build/index.js'), (Join-Path $PSScriptRoot 'build/index_bg.wasm') -Destination $bundle
@{ buildContext = @{ isPreview = $false } } |
    ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'config.json')
@{
    name = $workerName
    compatibilityDate = '2026-11-20'
    workersDev = $true
    previewUrls = $false
    placement = @{ region = 'aws:ap-southeast-1' }
    observability = @{
        enabled = $true
        redactQueryString = $true
        logs = @{ enabled = $true; invocationLogs = $true }
        traces = @{ enabled = $true }
    }
    env = @{
        DB = @{ type = 'hyperdrive'; id = $HyperdriveId }
        CHECKS = @{ type = 'durable-object'; worker = $workerName; exportName = 'DeploymentCheck' }
    }
    exports = @{ DeploymentCheck = @{ type = 'durable-object'; storage = 'sqlite' } }
    manifest = @{
        type = 'complete'
        mainModule = 'index.js'
        modules = @{ 'index.js' = @{ type = 'esm' }; 'index_bg.wasm' = @{ type = 'wasm' } }
    }
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $workerOutput 'worker.config.json')

Push-Location $workspace.FullName
try {
    & $CloudflareCli deploy --prebuilt --dry-run
    if ($LASTEXITCODE -ne 0) { throw 'Cloudflare prebuilt dry run failed.' }
    @{ PROBE_TOKEN = $env:SPACELESS_CF_PROBE_TOKEN } |
        ConvertTo-Json -Compress | Set-Content -LiteralPath $secretPath
    & $CloudflareCli deploy --prebuilt --secrets-file $secretPath --message 'Verify SpaceLess Rust API prerequisites on staging'
    if ($LASTEXITCODE -ne 0) { throw 'Cloudflare staging deployment failed.' }
} finally {
    if (Test-Path -LiteralPath $secretPath) { Remove-Item -LiteralPath $secretPath }
    Pop-Location
}
