param([int]$Port = 18080, [string]$ToolRoot = (Join-Path $env:TEMP 'spaceless-api-tools'))
$ErrorActionPreference = 'Stop'
$checks = @()
$base = "http://127.0.0.1:$Port/api/v1"
try {
    $response = Invoke-WebRequest "$base/users/me" -Headers @{ Origin = 'http://127.0.0.1:1421' } -SkipHttpErrorCheck -TimeoutSec 5
    $json = $response.Content | ConvertFrom-Json
    $checks += [pscustomobject]@{ Name = 'REST authentication'; Ready = ($response.StatusCode -eq 401 -and $json.error.code -eq 'UNAUTHORIZED') }
    $checks += [pscustomobject]@{ Name = 'CORS for UI 1421'; Ready = (@($response.Headers['Access-Control-Allow-Origin']) -contains 'http://127.0.0.1:1421') }
} catch { $checks += [pscustomobject]@{Name='REST authentication';Ready=$false} }
$psql = Join-Path $ToolRoot 'pgsql/bin/psql.exe'
if (Test-Path -LiteralPath $psql) {
    $ready = & $psql -h 127.0.0.1 -p 55432 -U spaceless_test -d spaceless_current_ui -qAt -c "SELECT EXISTS(SELECT 1 FROM pg_collation WHERE collname='und-x-icu') AND (SELECT count(*) FROM _sqlx_migrations WHERE success)>=6"
    $checks += [pscustomobject]@{Name='PostgreSQL / ICU / migrations';Ready=($LASTEXITCODE -eq 0 -and $ready -eq 't')}
}
$checks | Format-Table -AutoSize
Write-Host 'Optional provider variables in this terminal (not a delivery/media test):'
foreach ($group in @(
    @{Name='SMTP';Keys=@('SMTP_HOST','SMTP_FROM','PASSWORD_RESET_URL')},
    @{Name='S3 / gateway';Keys=@('S3_BUCKET','AWS_REGION','FILE_GATEWAY_PUBLIC_URL','FILE_GATEWAY_SECRET')},
    @{Name='LiveKit Cloud';Keys=@('LIVEKIT_URL','LIVEKIT_API_KEY','LIVEKIT_API_SECRET','LIVEKIT_CLOUD_REVOCATION')}
)) {
    $missing = @($group.Keys | Where-Object { ![Environment]::GetEnvironmentVariable($_) })
    Write-Host "$($group.Name): $(if($missing.Count){'not configured in this terminal'}else{'variables present; verify with real provider'})"
}
if ($checks.Ready -contains $false) { exit 1 }
