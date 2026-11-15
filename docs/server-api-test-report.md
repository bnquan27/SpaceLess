# SpaceLessApp server API test report

This audit checks whether each server operation works through its public API and records problems that callers can observe. The integration suite covers the Rust API, WebSocket upgrade, LiveKit webhook, and separate file gateway. [Integration suite](../server/test/api-integration.mjs)

## Results

| Measure | Result |
| --- | ---: |
| Integration cases | 24 |
| Passed | 23 |
| Failed | 1 |
| Skipped | 0 |
| Registered operations with a successful request in a passing case | 52 / 52 |
| Additional documented `/members/me` alias covered | 1 / 1 |
| Recorded requests and WebSocket upgrades | 347 |
| Existing Rust unit tests passed | 5 / 5 |

The final run took place on 15 November 2026 from 21:58:48 to 21:59:30 (UTC+07:00). The first and final runs used separate empty PostgreSQL databases. Both produced 23 passes, one failure, and 53 covered operations including the alias. The Node runner exited with code 1 because the public preview icon test failed. The Rust tests exited with code 0. [Final results](server-api-test-evidence/result.json) · [First run](server-api-test-evidence/first-result.json) · [Run manifest](server-api-test-evidence/run-manifest.txt) · [Rust test output](server-api-test-evidence/unit-tests.log)

A case is one workflow with setup, API calls, and assertions. A method and route count as covered when at least one passing case observes a successful response or a WebSocket upgrade. The 347 requests include fixture setup through the API and expected error responses. The total excludes readiness probes, SQL fixtures, and calls to the object-storage and LiveKit fixtures. The matrix counts the methods explicitly registered in Axum and the documented `/members/me` alias.

## Findings

### API-01. The public channel preview returns an icon that its visitor cannot load

A signed-in visitor opens a `PUBLIC_LINK` channel preview before joining. The preview replies `200` and includes `icon_url`. The visitor follows that URL with their Bearer token and receives `404 NOT_FOUND`. The channel owner follows the same advertised image link, gets `200`, and downloads the original 70-byte PNG. A client using the preview as returned cannot show the icon to the prospective member.

| Step | Caller | Expected | Observed |
| --- | --- | ---: | --- |
| `GET /api/v1/channel-links/{invite_code}` | Signed-in visitor | 200 | 200 with `icon_url` |
| `GET /api/v1/images/{upload_id}/url` | Channel owner | 200 | 200 |
| `GET /files/{upload_id}` | Channel owner | 200 | 200 with the original 70 bytes |
| `GET /api/v1/images/{upload_id}/url` | Signed-in visitor | 200 | `404 NOT_FOUND` |

The preview DTO publishes the URL. The image lookup requires an existing channel membership for channel images. The failed test exercises that mismatch in the advertised preview workflow. [Failing test](../server/test/media.mjs#L119) · [Preview DTO](../server/src/channels.rs#L140) · [Preview handler](../server/src/channels.rs#L622) · [Image access check](../server/src/files.rs#L555) · [Recorded request sequence](server-api-test-evidence/result.json)

### DOC-01. The design document describes the old JWT refresh behavior

The server rotates `jti`, preserves `sid`, and returns `401 UNAUTHORIZED` for the previous token. The integration evidence records `jtiChanged: true` and `sidPreserved: true`. This matches the API contract. The design document still describes preserving `jti`, which can mislead a client implementation that follows that text. [Observed token behavior](server-api-test-evidence/result.json) · [API contract](rust-server-api.md#L21) · [Design description](rust-server-design.md#L68)

## Environment and proof

I compared the MVP specification, API contract, server design, and runbook with the router and handlers in `server/src`. I used the existing REST and voice smoke tests as workflow references. [MVP specification](Dac_ta_he_thong_MVP_v1.docx) · [API contract](rust-server-api.md) · [Server design](rust-server-design.md) · [Runbook](rust-server-run.md) · [Router](../server/src/lib.rs#L290) · [REST smoke test](../server/tests/api-smoke.ps1) · [Voice smoke test](../server/tests/voice-smoke.mjs)

I built the Windows debug binary from source with the dependency versions in `Cargo.lock`. Each run started the Rust API and download gateway on IPv4 loopback, applied migrations to disposable PostgreSQL 17.11, and used Node.js v24.22.1 to make calls. The suite checks the Axum registration list and exits with an error if an operation lacks a successful request in a passing case. [Runner](../server/test/api-integration.mjs) · [Binary hash and commands](server-api-test-evidence/run-manifest.txt) · [API startup](server-api-test-evidence/api.log) · [Gateway startup](server-api-test-evidence/gateway.log) · [Database lifecycle](server-api-test-evidence/postgres.log) · [Decision log](server-api-test-evidence/decisions.tsv)

- Case 16 reconnects the WebSocket, recovers message history through a fixed sequence boundary, and advances across deleted sequences. Case 18 checks that read state, unread counts, and the WebSocket event agree. [Message tests](../server/test/messaging.mjs) · [Run output](server-api-test-evidence/integration.log)
- Case 20 observes 37 uploaded and downloaded bytes. Its range request receives `206` with 9 matching bytes and `Content-Range: bytes 0-8/37`. An earlier download URL receives `404` after the message is deleted. [File tests](../server/test/media.mjs) · [Recorded observations](server-api-test-evidence/result.json)
- Case 23 observes voice state advance from `PENDING` to `CONNECTED`, receives both screen-share events, and sees no active session after leave. The LiveKit fixture receives one removal call even though the client sends leave twice. [Voice tests](../server/test/media.mjs) · [Recorded observations](server-api-test-evidence/result.json)

The local provider tests cover object bytes, conditional PUT, gateway reads, signed LiveKit webhooks, and the Rust server's removal requests. S3 signature enforcement and cloud-storage behavior require a live S3-compatible service. Media transport and cloud token revocation require LiveKit Cloud clients. The recovery case checks HTTP acceptance and quotas with SMTP delivery intentionally unavailable. The reset tests insert short-lived token hashes into the test database and then drive reset through REST. Delivering an email and opening its link require a trusted SMTP relay. These runs exercise a fresh schema and the workflows below. Migration from populated older schemas, background worker timing, load, and other input combinations require separate cases. [Provider fixtures](../server/test/providers.mjs) · [Recovery tests](../server/test/accounts.mjs)

## Rerun

Run these commands from the repository root on Windows. Create a fresh, local PostgreSQL database with ICU collation `und-x-icu` and a name beginning with `spaceless_test_`. Set `DATABASE_URL` to that database before starting the suite. Use a new evidence directory for each run.

```powershell
cargo build --manifest-path server/Cargo.toml --locked
cargo test --manifest-path server/Cargo.toml --locked
$psql = (Get-Command psql.exe).Source
node server/test/api-integration.mjs server/target/debug/spaceless-server.exe $psql docs/qa/api-integration-rerun
```

The runner starts and closes the API, gateway, and local provider fixtures. Look in the run directory for `result.json`, `api.log`, and `gateway.log`. The runner leaves database administration to the caller. Use an empty database on each run.

## Case results

| Case | Workflow | Requests | Result |
| ---: | --- | ---: | --- |
| 1 | Registration returns a profile and username sign-in grants access to it | 6 | Pass |
| 2 | Registration validates required fields and rejects duplicate identities | 9 | Pass |
| 3 | Profile updates persist, reject duplicate email, and retire recovery for the old address | 10 | Pass |
| 4 | Refresh replaces the current token and logout ends only its sign-in | 9 | Pass |
| 5 | Recovery returns the same message for known and unknown addresses and enforces its quota | 7 | Pass |
| 6 | Password reset accepts a valid token once, replaces the password, and ends all prior sign-ins | 12 | Pass |
| 7 | User search paginates public profiles using decimal string cursors | 9 | Pass |
| 8 | Accepted friendship opens one DM and removing friendship makes it read-only | 20 | Pass |
| 9 | Declining clears a request and each person controls their own block | 16 | Pass |
| 10 | Channel settings, public preview, idempotent join, and deletion | 18 | Pass |
| 11 | Channel invitations record individual outcomes, accept, decline, and cancel on deletion | 22 | Pass |
| 12 | Creator changes member roles, transfers ownership, kicks a member, and leaves | 22 | Pass |
| 13 | Category and room mutations persist order and preserve rooms when a category is deleted | 20 | Pass |
| 14 | Private room visibility follows explicit membership and privacy changes | 19 | Pass |
| 15 | Message retries preserve identity and edits, and deletion preserves the retry receipt | 13 | Pass |
| 16 | Reconnect catches up through a fixed history snapshot and advances across deleted sequences | 19 | Pass |
| 17 | Reactions report caller-specific state and paginate, replace, and remove one reaction per user | 20 | Pass |
| 18 | Read marks only advance and synchronize room history, unread counts, and socket events | 17 | Pass |
| 19 | Refresh closes the old WebSocket and logout closes the replacement | 9 | Pass |
| 20 | Attachment uploads preserve bytes and the gateway serves a range | 12 | Pass |
| 21 | Avatar and both channel image purposes can be uploaded and read back | 15 | Pass |
| 22 | A signed-in visitor can open the icon supplied by a public channel preview | 11 | **Fail** |
| 23 | Voice joins, signed webhooks update screen state, and leave removes the session | 19 | Pass |
| 24 | Switching devices revokes the previous voice session and ignores its late leave | 13 | Pass |

## Operation matrix

API paths have the `/api/v1` prefix. `GET /files/{upload_id}` runs on the file gateway without that prefix. Each row names a passing example. The JSON evidence lists every recorded case for each operation, including the failing visitor workflow on the image route. [Full coverage map](server-api-test-evidence/result.json)

| Operation | Passing case |
| --- | ---: |
| `GET /users/me` | 1 |
| `POST /auth/login` | 1 |
| `POST /auth/register` | 1 |
| `PATCH /users/me` | 3 |
| `POST /auth/logout` | 4 |
| `POST /auth/refresh` | 4 |
| `POST /auth/forgot-password` | 5 |
| `POST /auth/reset-password` | 6 |
| `GET /users/search` | 7 |
| `DELETE /friends/{target_user_id}` | 8 |
| `PATCH /friends/requests/{request_id}` | 8 |
| `POST /dms` | 8 |
| `POST /friends/requests` | 8 |
| `DELETE /blocks/{target_user_id}` | 9 |
| `POST /blocks` | 9 |
| `DELETE /channels/{channel_id}` | 10 |
| `GET /channel-links/{invite_code}` | 10 |
| `PATCH /channels/{channel_id}` | 10 |
| `POST /channel-links/{invite_code}/join` | 10 |
| `POST /channels` | 10 |
| `PATCH /invitations/{invitation_id}` | 11 |
| `POST /channels/{channel_id}/invitations` | 11 |
| `DELETE /channels/{channel_id}/members/{user_id}` | 12 |
| `DELETE /channels/{channel_id}/members/me` | 12 |
| `PATCH /channels/{channel_id}/members/{user_id}/role` | 12 |
| `POST /channels/{channel_id}/transfer-ownership` | 12 |
| `DELETE /channels/{channel_id}/categories/{category_id}` | 13 |
| `DELETE /channels/{channel_id}/rooms/{room_id}` | 13 |
| `PATCH /channels/{channel_id}/categories/{category_id}` | 13 |
| `PATCH /channels/{channel_id}/rooms/{room_id}` | 13 |
| `POST /channels/{channel_id}/categories` | 13 |
| `POST /channels/{channel_id}/rooms` | 13 |
| `PUT /channels/{channel_id}/categories/order` | 13 |
| `PUT /channels/{channel_id}/rooms/order` | 13 |
| `DELETE /rooms/{room_id}/messages/{message_id}` | 15 |
| `PATCH /rooms/{room_id}/messages/{message_id}` | 15 |
| `POST /rooms/{room_id}/messages` | 15 |
| `GET /rooms/{room_id}/messages` | 16 |
| `DELETE /rooms/{room_id}/messages/{message_id}/reactions/me` | 17 |
| `GET /rooms/{room_id}/messages/{message_id}/reactions` | 17 |
| `PUT /rooms/{room_id}/messages/{message_id}/reactions/me` | 17 |
| `GET /bootstrap` | 18 |
| `PUT /rooms/{room_id}/read-state` | 18 |
| `GET /ws` | 19 |
| `GET /attachments/{attachment_id}/url` | 20 |
| `GET /files/{upload_id}` | 20 |
| `POST /rooms/{room_id}/uploads/presign` | 20 |
| `GET /images/{upload_id}/url` | 21 |
| `POST /channels/{channel_id}/images/uploads/presign` | 21 |
| `POST /users/me/avatar/uploads/presign` | 21 |
| `DELETE /rooms/{room_id}/voice/leave` | 23 |
| `POST /rooms/{room_id}/voice/join` | 23 |
| `POST /webhooks/livekit` | 23 |
