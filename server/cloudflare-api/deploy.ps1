#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidatePattern('^[0-9a-f]{32}$')] [string] $HyperdriveId,
    [string] $CloudflareCli = 'cf',
    [string] $WorkerBuild = 'worker-build',
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'
if (!$DryRun -and ($env:STAGING_GATE_TOKEN.Length -lt 32 -or $env:APP_JWT_SECRET.Length -lt 32)) {
    throw 'Set STAGING_GATE_TOKEN and APP_JWT_SECRET to independent values of at least 32 characters.'
}

$workerName = 'spaceless-staging-api'
$bindings = @{
    DB = @{ type = 'hyperdrive'; id = $HyperdriveId }
    AUTH_API = @{ type = 'durable-object'; worker = $workerName; exportName = 'AuthApi' }
    USER_SOCKETS = @{ type = 'durable-object'; worker = $workerName; exportName = 'UserSockets' }
    MESSAGE_OUTBOX = @{ type = 'durable-object'; worker = $workerName; exportName = 'MessageOutbox' }
}
$secrets = @{
    STAGING_GATE_TOKEN = $env:STAGING_GATE_TOKEN
    APP_JWT_SECRET = $env:APP_JWT_SECRET
}
if ($env:R2_BUCKET) {
    if ($env:R2_ACCOUNT_ID -notmatch '^[0-9a-f]{32}$' -or $env:R2_BUCKET -notmatch '^[a-z0-9-]+$' -or
        (!$DryRun -and (!$env:R2_ACCESS_KEY_ID -or !$env:R2_SECRET_ACCESS_KEY -or $env:FILE_GATEWAY_SECRET.Length -lt 32))) {
        throw 'Configure the R2 account, private bucket and signing secrets before deploying file routes.'
    }
    $bindings.FILES = @{ type = 'r2'; name = $env:R2_BUCKET }
    $bindings.R2_ACCOUNT_ID = @{ type = 'text'; value = $env:R2_ACCOUNT_ID }
    $bindings.R2_BUCKET = @{ type = 'text'; value = $env:R2_BUCKET }
    $secrets.R2_ACCESS_KEY_ID = $env:R2_ACCESS_KEY_ID
    $secrets.R2_SECRET_ACCESS_KEY = $env:R2_SECRET_ACCESS_KEY
    $secrets.FILE_GATEWAY_SECRET = $env:FILE_GATEWAY_SECRET
}
if ($env:GOOGLE_OAUTH_CLIENT_ID -or $env:GOOGLE_OAUTH_CLIENT_SECRET) {
    if (!$env:GOOGLE_OAUTH_CLIENT_ID -or !$env:GOOGLE_OAUTH_CLIENT_SECRET) { throw 'Configure both Google OAuth values.' }
    $secrets.GOOGLE_OAUTH_CLIENT_ID = $env:GOOGLE_OAUTH_CLIENT_ID
    $secrets.GOOGLE_OAUTH_CLIENT_SECRET = $env:GOOGLE_OAUTH_CLIENT_SECRET
}
if ($env:LIVEKIT_URL) {
    if ($env:LIVEKIT_URL -notmatch '^wss://[^/?#@]+(?:/)?$' -or !$env:LIVEKIT_API_KEY -or !$env:LIVEKIT_API_SECRET) {
        throw 'Configure a LiveKit Cloud WSS URL with the staging API key and secret.'
    }
    $bindings.LIVEKIT_URL = @{ type = 'text'; value = $env:LIVEKIT_URL }
    $bindings.LIVEKIT_CLOUD_REVOCATION = @{ type = 'text'; value = 'true' }
    $secrets.LIVEKIT_API_KEY = $env:LIVEKIT_API_KEY
    $secrets.LIVEKIT_API_SECRET = $env:LIVEKIT_API_SECRET
}
$scratch = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("spaceless-api-" + [Guid]::NewGuid().ToString('N')))
$secretPath = Join-Path $scratch.FullName 'secrets.json'
$outputRoot = Join-Path $scratch.FullName '.cloudflare/output/v1'
$workerRoot = Join-Path $outputRoot 'workers/default'
$bundle = Join-Path $workerRoot 'bundle'

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
    ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputRoot 'config.json')
@{
    name = $workerName
    compatibilityDate = '2026-11-20'
    workersDev = $true
    previewUrls = $false
    triggers = @(@{ type = 'scheduled'; schedule = '* * * * *' })
    placement = @{ region = 'aws:ap-southeast-1' }
    observability = @{
        enabled = $true
        redactQueryString = $true
        logs = @{ enabled = $true; invocationLogs = $true }
        traces = @{ enabled = $true }
    }
    env = $bindings
    exports = @{
        AuthApi = @{ type = 'durable-object'; storage = 'sqlite' }
        UserSockets = @{ type = 'durable-object'; storage = 'sqlite' }
        MessageOutbox = @{ type = 'durable-object'; storage = 'sqlite' }
    }
    manifest = @{
        type = 'complete'
        mainModule = 'index.js'
        modules = @{ 'index.js' = @{ type = 'esm' }; 'index_bg.wasm' = @{ type = 'wasm' } }
    }
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $workerRoot 'worker.config.json')

Push-Location $scratch.FullName
try {
    & $CloudflareCli deploy --prebuilt --dry-run
    if ($LASTEXITCODE -ne 0) { throw 'Cloudflare prebuilt dry run failed.' }
    if ($DryRun) { return }
    $secrets | ConvertTo-Json -Compress | Set-Content -LiteralPath $secretPath
    & $CloudflareCli deploy --prebuilt --secrets-file $secretPath --message 'Deploy the SpaceLess Rust staging API'
    if ($LASTEXITCODE -ne 0) { throw 'Cloudflare staging deployment failed.' }
} finally {
    if (Test-Path -LiteralPath $secretPath) { Remove-Item -LiteralPath $secretPath }
    Pop-Location
}
