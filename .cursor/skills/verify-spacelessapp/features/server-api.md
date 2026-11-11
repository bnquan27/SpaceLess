# Server API

## Sub-features

- Register, log in, refresh, log out and read the current account.
- Friendship, DM, room visibility, messages, reactions and read state.
- Presigned upload, authorized download, cleanup and voice session generation.

## How to get to it (user POV)

A client uses `/api/v1/auth/register`, `/api/v1/auth/login` and `/api/v1/bootstrap`. It sends the issued Bearer to protected REST endpoints and to `/api/v1/ws` during the WebSocket upgrade. The desktop preview currently uses its local forms.

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
