#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [Uri] $ProbeUrl,
    [ValidateRange(1, 20)] [int] $Runs = 6
)

$ErrorActionPreference = 'Stop'
if ($ProbeUrl.Scheme -ne 'https' -or $ProbeUrl.UserInfo -or $ProbeUrl.Query -or $ProbeUrl.Fragment) {
    throw 'ProbeUrl must be the HTTPS staging URL.'
}
if ($env:SPACELESS_CF_PROBE_TOKEN.Length -lt 32) {
    throw 'Set SPACELESS_CF_PROBE_TOKEN to the token used for staging deployment.'
}

$base = $ProbeUrl.AbsoluteUri.TrimEnd('/')
$headers = @{ Authorization = "Bearer $env:SPACELESS_CF_PROBE_TOKEN" }
$anonymous = Invoke-WebRequest "$base/check/postgres" -Method Post -TimeoutSec 25 -SkipHttpErrorCheck
if ($anonymous.StatusCode -ne 401) {
    throw "Expected the anonymous request to return HTTP 401. Received $($anonymous.StatusCode)."
}
[pscustomobject]@{ Route = 'postgres (no credential)'; Status = [int]$anonymous.StatusCode }

foreach ($attempt in 1..$Runs) {
    foreach ($check in @{
        'postgres' = 'postgres'
        'argon2/hash' = 'argon2_hash'
        'argon2/verify' = 'argon2_verify'
        'session-revocation' = 'session_revocation_fixture'
    }.GetEnumerator()) {
        $response = Invoke-WebRequest "$base/check/$($check.Key)" -Method Post -Headers $headers -TimeoutSec 30 -SkipHttpErrorCheck
        $json = try { $response.Content | ConvertFrom-Json -ErrorAction Stop } catch { $null }
        [pscustomobject]@{
            Attempt = $attempt
            Route = $check.Key
            Status = [int]$response.StatusCode
            Success = $json.success
            Stage = $json.stage
            Ray = $response.Headers['cf-ray'] -join ','
        }
        if ($response.StatusCode -ne 200 -or $json.success -ne $true -or $json.check -ne $check.Value) {
            throw "Probe check $($check.Key) failed."
        }
    }
}
