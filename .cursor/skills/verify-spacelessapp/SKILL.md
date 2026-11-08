---
name: verify-spacelessapp
description: Verify SpaceLessApp on Windows through its Tauri window, responsive web preview, or Rust REST and WebSocket API. Use for smoke tests, bug reproductions, and collecting evidence of user-visible behavior.
---

# Verify SpaceLessApp

Run from the repository root in PowerShell 7. Read the relevant journey in [the feature map](features/README.md) before driving it. The Windows Tauri window is the primary user surface. The Vite preview runs the same React UI at adjustable widths. The Rust API runs independently. Orbit sign-in, sign-up and recovery currently validate locally and display preview status.

## Launch

Install app dependencies with `npm ci` if `node_modules` is absent. For the desktop path, use Windows with Rust, MSVC Build Tools and WebView2. Build the current checkout before using the executable.

```powershell
npm run tauri -- build --no-bundle
```

Resolve Playwright from Codex's bundled workspace dependencies. Set `SPACELESS_PLAYWRIGHT` to the absolute `playwright` directory under the returned Node.js packages directory. Outside Codex, install `playwright` into a scratch npm prefix and point the variable at that package's directory. The desktop path connects to the installed WebView2 runtime over CDP.

For web journeys, run `npm run build`. Install the Chromium headless shell for the same Playwright package if it is missing.

```powershell
$env:PLAYWRIGHT_BROWSERS_PATH = Join-Path $env:TEMP 'spaceless-verify-browsers'
node (Join-Path $env:SPACELESS_PLAYWRIGHT 'cli.js') install chromium --only-shell
```

The UI helper launches the release executable or serves `dist/` with `vite preview` on a fresh loopback port. It creates a temporary WebView2 data directory for desktop or a fresh browser context for web. It drives one feature, captures evidence and tears down the instance in one command.

For the API, follow [the server runbook](../../../docs/rust-server-run.md) with a dedicated, empty PostgreSQL database that supports `und-x-icu`. Set `DATABASE_URL` to that database, generate a fresh `APP_JWT_SECRET` and set `SERVER_BIND` to an unused loopback port. Start `cargo run --manifest-path server/Cargo.toml --locked` in its own terminal. It applies migrations before printing `SpaceLess server ready`. The API smoke script registers its own fixture accounts. Two API runs need separate databases as well as separate ports because each API takes a database lease.

## Doctor

The UI helper always checks its child is alive, probes `/json/version` for WebView2 or `/` for Vite preview and waits for the expected heading in the launched document. For desktop it also verifies the WebView2 document is on `tauri.localhost` and records the binary SHA-256. If a journey fails, inspect `result.json` and `process.log` before retrying.

For the API, verify the test database and bind address in the launch terminal, then run this read-only probe. Expect `401` with `UNAUTHORIZED`. The smoke script then proves auth by registering, logging in and calling protected routes.

```powershell
$baseUrl = "http://$env:SERVER_BIND/api/v1"
$response = Invoke-WebRequest "$baseUrl/users/me" -SkipHttpErrorCheck
if ($response.StatusCode -ne 401 -or ($response.Content | ConvertFrom-Json).error.code -ne 'UNAUTHORIZED') {
    throw 'Expected an unauthenticated response from the SpaceLessApp test API'
}
```

## Drive

```powershell
node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs desktop sign-in
node .cursor/skills/verify-spacelessapp/scripts/verify-ui.mjs web sign-in 390
```

Replace `sign-in` with `sign-up` or `recovery` to test those forms. Use `web design` for the comparison room. The optional web width defaults to 1280. For API scenarios use [the API journey](features/server-api.md). When reproducing a new issue, use labels and roles from the feature map and exercise the real entry point. Run the packaged window for native regressions and use the browser for narrow layouts.

## Evidence

Use `docs/qa/`. The UI helper prints the run folder and writes before/after screenshots, accessibility snapshots, `process.log`, and `result.json` with actions, observations and cleanup status. Validation journeys also capture the error state. The design journey keeps the Markdown download. Open the screenshots to check visual claims, such as clipping or layout. Use the JSON to verify whether a preview submission made a request. Use synthetic values and omit credentials, token values, request bodies and signed URL queries from any extra evidence.

For API journeys, keep the command transcript and exit status in `docs/qa/`. Verify side effects through the documented REST reads or WebSocket events. The optional `-Psql` fixture seeds a reset-token hash to exercise the reset endpoint and can accelerate upload expiration. That proves neither SMTP delivery nor natural expiration timing. The local LiveKit fixture checks server-side webhook and generation handling. LiveKit Cloud is required to verify actual media and token revocation.

## Cleanup

The UI helper closes its own Playwright connection and child process and removes the WebView2 scratch profile. It checks the loopback port is closed and verifies the evidence files still exist. Keep the evidence directory. For an API run, stop the API and gateway from the terminals that started them and stop any database or storage instance created for that run. Remove only the dedicated test database and bucket that you created and keep the transcript.

## Helpers

- `scripts/verify-ui.mjs` runs the commands under **Drive**.
- `server/tests/api-smoke.ps1` drives the REST and WebSocket workflows listed in the API journey.
- `server/tests/voice-smoke.mjs` drives the API against a local LiveKit protocol fixture. See the server runbook for its command.
