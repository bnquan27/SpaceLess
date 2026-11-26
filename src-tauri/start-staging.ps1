param(
    [string]$StagingSecretsPath,
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $PSScriptRoot 'target/release/spaceless-app.exe'
$settings = @{}
Get-Content -LiteralPath (Join-Path $projectRoot '.env.staging.local') | ForEach-Object {
    if ($_ -match '^\s*(?:export\s+)?(GOOGLE_OAUTH_CLIENT_ID|STAGING_GATE_TOKEN)\s*=\s*(.*?)\s*$') {
        $settings[$Matches[1]] = $Matches[2].Trim('"', "'")
    }
}

$clientId = $settings['GOOGLE_OAUTH_CLIENT_ID']
$gate = $env:SPACELESS_DESKTOP_STAGING_TOKEN
if ($StagingSecretsPath) {
    $gate = (Get-Content -Raw -LiteralPath $StagingSecretsPath | ConvertFrom-Json).STAGING_GATE_TOKEN
} elseif (-not $gate) {
    $gate = $settings['STAGING_GATE_TOKEN']
}
if ($clientId -notmatch '^[^\s]+\.apps\.googleusercontent\.com$') {
    throw 'Thiếu GOOGLE_OAUTH_CLIENT_ID hợp lệ trong .env.staging.local.'
}
if ($gate -notmatch '^[!-~]{32,512}$') {
    throw 'Thiếu STAGING_GATE_TOKEN hợp lệ cho API staging.'
}

if ($Build -or -not (Test-Path -LiteralPath $desktop -PathType Leaf)) {
    Push-Location -LiteralPath $projectRoot
    try {
        npm run tauri -- build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw 'Không thể build ứng dụng Tauri.' }
    } finally {
        Pop-Location
    }
}

$process = [System.Diagnostics.ProcessStartInfo]::new($desktop)
$process.UseShellExecute = $false
$process.EnvironmentVariables['SPACELESS_DESKTOP_STAGING'] = '1'
$process.EnvironmentVariables['SPACELESS_DESKTOP_GOOGLE_CLIENT_ID'] = $clientId
$process.EnvironmentVariables['SPACELESS_DESKTOP_STAGING_TOKEN'] = $gate
[void][System.Diagnostics.Process]::Start($process)
