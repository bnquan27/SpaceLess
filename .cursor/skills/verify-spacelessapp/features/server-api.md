# Server API

## Sub-features

- Register, log in, refresh, log out and read the current account.
- Friendship, DM, room visibility, messages, reactions and read state.
- Presigned upload, authorized download, cleanup and voice session generation.

## How to get to it (user POV)

A client uses `/api/v1/auth/register`, `/api/v1/auth/login` and `/api/v1/bootstrap`. It sends the issued Bearer to protected REST endpoints and to `/api/v1/ws` during the WebSocket upgrade. The desktop preview uses local forms and sample data.

## Drive the integration suite

Build the server and set `DATABASE_URL` to a fresh, empty PostgreSQL database on `127.0.0.1`. Use a name beginning with `spaceless_test_` and a PostgreSQL build with the `und-x-icu` collation. Use Node.js 24 and make `psql.exe` available.

```powershell
cargo build --manifest-path server/Cargo.toml --locked
$psql = (Get-Command psql.exe).Source
$evidence = Join-Path 'docs/qa' ('api-integration-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
node server/test/api-integration.mjs server/target/debug/spaceless-server.exe $psql $evidence
```

The runner starts the API, gateway, and local S3 and LiveKit fixtures. It probes the API and gateway for `401 UNAUTHORIZED` before the first case. Read the case results and route coverage in `result.json`. Check `summary.failed`, `summary.uncovered`, and any `summary.setupError`. The runner stops its processes and fixtures. Remove the dedicated database or cluster after saving the evidence.

Use a real SMTP relay to test delivery, a live S3-compatible service to test signature enforcement, and LiveKit Cloud clients to test media transport and cloud token revocation. See [the API integration report](../../../../docs/server-api-test-report.md).

## Driving it with REST and WebSocket

Launch the isolated API as described in the skill. Run the repository smoke script from a client terminal with `$baseUrl` set to the test API URL, including `/api/v1`.

```powershell
New-Item -ItemType Directory -Force docs/qa | Out-Null
$log = Join-Path 'docs/qa' ('api-smoke-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
pwsh -NoProfile -File server/tests/api-smoke.ps1 -BaseUrl $baseUrl 2>&1 | Tee-Object $log
$code = $LASTEXITCODE
Add-Content $log "Exit code $code"
if ($code -ne 0) { throw "API smoke failed. Evidence in $log" }
```

Expect the pass line for auth, channel permissions, message retry, history and WebSocket delivery. `-WithFiles` additionally drives the S3 and gateway routes. For a loopback gateway on a custom port, set `FILE_GATEWAY_BIND` in the gateway process and point `FILE_GATEWAY_PUBLIC_URL` at that address in the API process. `-Psql psql` checks reset and, with `-WithFiles`, file cleanup. See [the server runbook](../../../../docs/rust-server-run.md) for provider setup.

For voice session generation and revocation, build the server, set `DATABASE_URL` to a test database and run the local voice fixture. It starts its own API on loopback. Stop the previous API before reusing its database, or give the voice fixture a separate database.

```powershell
node server/tests/voice-smoke.mjs server/target/debug/spaceless-server.exe
```

## Gotchas

Pass only a dedicated test API because the script creates users and writes messages. Supply the exact same test `DATABASE_URL` to `-Psql`. Read output from the latest run. A fixture result does not prove live email delivery, media transport or Cloud token revocation.
