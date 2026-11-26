param(
    [string]$DatabaseUrl = $env:DATABASE_URL,
    [string]$ToolRoot = (Join-Path $env:TEMP 'spaceless-api-tools'),
    [int]$Port = 18080
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $repoRoot

# This fallback uses only the isolated database created for the local UI test.
if (!$DatabaseUrl) {
    $pgCtl = Join-Path $ToolRoot 'pgsql/bin/pg_ctl.exe'
    $dataDir = Join-Path $ToolRoot 'data-20261126'
    if (!(Test-Path -LiteralPath $pgCtl) -or !(Test-Path -LiteralPath $dataDir)) {
        throw 'Set DATABASE_URL for an existing PostgreSQL database with und-x-icu. See docs/ui-api-integration.md.'
    }
    & $pgCtl -D $dataDir status *> $null
    if ($LASTEXITCODE -ne 0) {
        & $pgCtl -D $dataDir -o '-h 127.0.0.1 -p 55432' -l (Join-Path $ToolRoot 'postgresql-test.log') -w start
        if ($LASTEXITCODE -ne 0) { throw 'Local PostgreSQL could not start.' }
    }
    $psql = Join-Path $ToolRoot 'pgsql/bin/psql.exe'
    $exists = & $psql -h 127.0.0.1 -p 55432 -U spaceless_test -d postgres -qAt -c "SELECT 1 FROM pg_database WHERE datname='spaceless_current_ui'"
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect local database.' }
    if ($exists -ne '1') {
        & (Join-Path $ToolRoot 'pgsql/bin/createdb.exe') -h 127.0.0.1 -p 55432 -U spaceless_test spaceless_current_ui
        if ($LASTEXITCODE -ne 0) { throw 'Could not create spaceless_current_ui.' }
    }
    $DatabaseUrl = 'postgres://spaceless_test@127.0.0.1:55432/spaceless_current_ui'
}
$env:DATABASE_URL = $DatabaseUrl
$env:SERVER_BIND = "127.0.0.1:$Port"
$publicConfig = Join-Path $repoRoot '.env.local'
if (!(Test-Path -LiteralPath $publicConfig)) {
    Set-Content -LiteralPath $publicConfig -Value "VITE_API_ORIGIN=http://127.0.0.1:$Port`nVITE_FILE_ORIGIN=http://127.0.0.1:18081"
}
if (!$env:APP_JWT_SECRET) {
    $secretBytes = New-Object byte[] 48
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $rng.GetBytes($secretBytes)
    $rng.Dispose()
    $env:APP_JWT_SECRET = [Convert]::ToBase64String($secretBytes)
}
$env:RUST_MIN_STACK = '16777216'
$binary = Join-Path $repoRoot 'server/target/debug/spaceless-server.exe'
if (!(Test-Path -LiteralPath $binary)) {
    throw 'Build the server first: cargo build --manifest-path server/Cargo.toml --locked'
}
Write-Host "API: http://127.0.0.1:$Port/api/v1"
Write-Host 'Open the web app at http://127.0.0.1:1421. Use Ctrl+C to stop the API.'
& $binary
exit $LASTEXITCODE
