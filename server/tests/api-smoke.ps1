param([string]$BaseUrl = 'http://127.0.0.1:8080/api/v1', [switch]$WithFiles, [string]$Psql)

$ErrorActionPreference = 'Stop'

function Assert-True($Condition, [string]$Message) {
    if (!$Condition) { throw $Message }
}

function Request([string]$Method, [string]$Path, [string]$Token, $Body, [int]$Status = 200) {
    $arguments = @{ Uri = "$BaseUrl$Path"; Method = $Method; SkipHttpErrorCheck = $true }
    if ($Token) { $arguments.Headers = @{ Authorization = "Bearer $Token" } }
    if ($null -ne $Body) {
        $arguments.ContentType = 'application/json'
        $arguments.Body = ConvertTo-Json -InputObject $Body -Compress -Depth 12
    }
    $response = Invoke-WebRequest @arguments
    Assert-True ($response.StatusCode -eq $Status) "$Method $Path expected $Status, got $($response.StatusCode): $($response.Content)"
    if ($response.Content) { ConvertFrom-Json -InputObject $response.Content -AsHashtable }
}

function Post-Concurrently([string]$Path, [string]$Token, $First, $Second, [string]$ExpectedStatuses) {
    $client = [Net.Http.HttpClient]::new()
    try {
        if ($Token) {
            $client.DefaultRequestHeaders.Authorization = [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', $Token)
        }
        $pending = foreach ($body in @($First, $Second)) {
            $json = ConvertTo-Json -InputObject $body -Compress
            $client.PostAsync("$BaseUrl$Path", [Net.Http.StringContent]::new($json, [Text.Encoding]::UTF8, 'application/json'))
        }
        $responses = $pending | ForEach-Object { $_.GetAwaiter().GetResult() }
        $statuses = ($responses | ForEach-Object { [int]$_.StatusCode } | Sort-Object) -join ','
        Assert-True ($statuses -eq $ExpectedStatuses) "Concurrent POST $Path expected $ExpectedStatuses, got $statuses"
        $created = $responses | Where-Object StatusCode -eq Created
        $created.Content.ReadAsStringAsync().GetAwaiter().GetResult() | ConvertFrom-Json -AsHashtable
    } finally { $client.Dispose() }
}

function New-Account([string]$Name) {
    $password = 'integration-only password'
    $profile = Request POST '/auth/register' '' @{
        username = $Name; password = $password; full_name = $Name
        date_of_birth = '1990-01-01'; email = "$Name@example.test"; phone = '+84900000000'
    } 201
    Assert-True (($profile.user_id -is [string]) -and ([long]$profile.user_id -gt 9007199254740991)) 'Register must return a decimal string Snowflake above 2^53'
    $session = Request POST '/auth/login' '' @{ username = $Name; password = $password }
    @{ id = $profile.user_id; token = $session.access_token; name = $Name; password = $password }
}

function Open-Socket([string]$Token) {
    $socket = [Net.WebSockets.ClientWebSocket]::new()
    $socket.Options.SetRequestHeader('Authorization', "Bearer $Token")
    $address = $BaseUrl -replace '^https:', 'wss:' -replace '^http:', 'ws:'
    $socket.ConnectAsync([uri]"$address/ws", [Threading.CancellationToken]::None).GetAwaiter().GetResult() | Out-Null
    $socket
}

function Wait-Event($Socket, [string]$Type, [scriptblock]$Predicate) {
    $cancel = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(12))
    try {
        $buffer = [byte[]]::new(65536)
        while (!$cancel.IsCancellationRequested) {
            try { $frame = $Socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), $cancel.Token).GetAwaiter().GetResult() }
            catch { throw "Did not receive $Type ($($_.Exception.Message))" }
            Assert-True ($frame.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Text -and $frame.EndOfMessage) "Socket closed or returned a fragmented test frame before $Type"
            $event = [Text.Encoding]::UTF8.GetString($buffer, 0, $frame.Count) | ConvertFrom-Json -AsHashtable
            if ($event.type -eq $Type -and (& $Predicate $event.data)) { return $event }
        }
    } finally { $cancel.Dispose() }
    throw "Did not receive $Type"
}

function Put-Attachment([string]$Room, [string]$Token, [switch]$CheckReplay) {
    $payload = [Text.Encoding]::UTF8.GetBytes('SpaceLess attachment')
    $upload = Request POST "/rooms/$Room/uploads/presign" $Token @{
        file_name = 'note.txt'; mime_type = 'text/plain'; file_size_bytes = $payload.Length
    }
    $put = @{
        Uri = $upload.put_url; Method = 'PUT'; Body = $payload; SkipHttpErrorCheck = $true
        ContentType = 'text/plain'; Headers = @{ 'If-None-Match' = '*' }
    }
    foreach ($key in $upload.required_headers.Keys) {
        if ($key -notin @('host', 'content-length', 'content-type')) { $put.Headers[$key] = $upload.required_headers[$key] }
    }
    $written = Invoke-WebRequest @put
    Assert-True ($written.StatusCode -eq 200) "Presigned upload failed: $($written.StatusCode) $($written.Content)"
    if ($CheckReplay) {
        $replayed = Invoke-WebRequest @put
        Assert-True ($replayed.StatusCode -eq 412) 'Repeated PUT must fail the conditional write'
    }
    $upload
}

function Test-Query([string]$Sql) {
    $value = & $Psql --dbname $env:DATABASE_URL -v ON_ERROR_STOP=1 -qAt -c $Sql
    if ($LASTEXITCODE -ne 0) { throw 'Test database query failed' }
    $value
}

$suffix = [guid]::NewGuid().ToString('N').Substring(0, 12)
$first = @{
    username = "Race$suffix"; password = 'integration-only password'; full_name = 'Race'
    date_of_birth = '1990-01-01'; email = "race1$suffix@example.test"; phone = '+84900000000'
}
$second = $first.Clone()
$second.username = "race$suffix"
$second.email = "race2$suffix@example.test"
Post-Concurrently '/auth/register' '' $first $second '201,409' | Out-Null
$alice = New-Account "smokea$suffix"
$bob = New-Account "smokeb$suffix"
$charlie = New-Account "smokec$suffix"
$socket = Open-Socket $bob.token
try {
    $found = Request GET "/users/search?q=$($bob.name)" $alice.token $null
    Assert-True ($found.items.Count -eq 1 -and !$found.items[0].Contains('email')) 'Search must omit private data'
    Request PATCH '/users/me' $alice.token @{ role = 'CREATOR' } 400 | Out-Null
    Request POST '/auth/login' '' @{ username = $alice.name; password = 'wrong' } 401 | Out-Null

    $friend = Request POST '/friends/requests' $alice.token @{ target_user_id = $bob.id } 201
    $requestId = $friend.request_id
    Wait-Event $socket FRIEND_REQUEST { param($data) $data.request_id -eq $requestId } | Out-Null
    Request PATCH "/friends/requests/$requestId" $bob.token @{ decision = 'ACCEPT' } | Out-Null
    $dm = Request POST '/dms' $alice.token @{ target_user_id = $bob.id } 201
    $existingDm = Request POST '/dms' $bob.token @{ target_user_id = $alice.id }
    Assert-True ($dm.room_id -eq $existingDm.room_id) 'DM must be unique for a user pair'

    $created = Request POST '/channels' $alice.token @{ channel_name = "Smoke $suffix" } 201
    $channel = $created.channel.channel_id
    $inviteBody = @{ target_user_id = $bob.id }
    $invitation = Post-Concurrently "/channels/$channel/invitations" $alice.token $inviteBody $inviteBody '201,409'
    Request PATCH "/invitations/$($invitation.invitation_id)" $bob.token @{ decision = 'ACCEPT' } | Out-Null
    $category = Request POST "/channels/$channel/categories" $alice.token @{ category_name = 'Test group' } 201
    $room = Request POST "/channels/$channel/rooms" $alice.token @{
        room_name = 'Private text'; type = 'TEXT'; is_private = $true
        category_id = $category.category_id; member_ids = @($bob.id)
    } 201
    $roomId = $room.room_id
    Wait-Event $socket ROOM_CREATE { param($data) $data.room_id -eq $roomId } | Out-Null
    Request GET "/rooms/$roomId/messages" $charlie.token $null 404 | Out-Null

    $messagePath = "/rooms/$roomId/messages"
    $messageBody = @{ client_id = 'integration-test'; content = 'One message' }
    $beforeSend = Request GET $messagePath $alice.token $null
    $message = Post-Concurrently $messagePath $alice.token $messageBody $messageBody '200,201'
    $again = Request POST $messagePath $alice.token $messageBody
    Assert-True ($again.message_id -eq $message.message_id -and $again.room_sequence -eq '1') 'Retry must preserve message identity and sequence'
    $afterSend = Request GET $messagePath $alice.token $null
    Assert-True ($afterSend.items.Count -eq 1 -and ([long]$afterSend.revision - [long]$beforeSend.revision) -eq 1) 'Concurrent sends must commit exactly one message and one event'
    $messageId = $message.message_id
    $event = Wait-Event $socket MESSAGE_CREATE { param($data) $data.message_id -eq $messageId }
    Assert-True ($event.data.room_sequence -is [string] -and $event.data.content -eq $messageBody.content) 'WebSocket must resolve the message view with a string sequence'
    if ($Psql) {
        $storedMessageId = [long]$messageId
        Assert-True ((Test-Query "SELECT count(*) FROM outbox_events WHERE data->>'message_id'='$storedMessageId' AND data ? 'content'") -eq '0') 'Outbox must not retain message text after hard deletion'
    }
    Request PUT "$messagePath/$messageId/reactions/me" $bob.token @{ reaction_type = 'LIKE' } 204 | Out-Null
    $reaction = Wait-Event $socket MESSAGE_REACTION_UPDATE { param($data) $data.message_id -eq $messageId }
    Assert-True ($reaction.data.my_reaction -eq 'LIKE') 'Reaction event must carry the receiver reaction'
    Request PUT "/rooms/$roomId/read-state" $bob.token @{ last_read_sequence = '1' } | Out-Null
    $read = Request PUT "/rooms/$roomId/read-state" $bob.token @{ last_read_sequence = '0' }
    Assert-True ($read.last_read_sequence -eq '1') 'Read state must not go backwards'
    Request PATCH "$messagePath/$messageId" $bob.token @{ content = 'not the author' } 403 | Out-Null
    Request PATCH "$messagePath/$messageId" $alice.token @{ content = 'Edited message' } | Out-Null
    $editedRetry = Request POST $messagePath $alice.token $messageBody
    Assert-True ($editedRetry.message_id -eq $messageId -and $editedRetry.content -eq 'Edited message') 'Retry must match the original request after an edit'
    $conflict = Request POST $messagePath $alice.token @{ client_id = $messageBody.client_id; content = 'Edited message' } 409
    Assert-True ($conflict.error.code -eq 'CLIENT_ID_REUSED') 'The edited text must not replace the original receipt payload'
    Request DELETE "$messagePath/$messageId" $alice.token $null 204 | Out-Null
    $deleted = Request POST $messagePath $alice.token $messageBody 409
    Assert-True ($deleted.error.code -eq 'MESSAGE_DELETED') 'A deleted message must keep its receipt'
    $conflict = Request POST $messagePath $alice.token @{ client_id = $messageBody.client_id; content = 'Different message' } 409
    Assert-True ($conflict.error.code -eq 'CLIENT_ID_REUSED') 'A deleted receipt must still reject a different payload'
    $gap = Request GET "$($messagePath)?after_sequence=0" $bob.token $null
    Assert-True ($gap.items.Count -eq 0 -and $gap.next_cursor -eq '1') 'Cursor must advance past a deleted sequence'

    if ($WithFiles) {
        $upload = Put-Attachment $roomId $alice.token -CheckReplay
        $attachmentBody = @{ client_id = 'attachment'; content = ''; upload_ids = @($upload.upload_id) }
        $attached = Request POST $messagePath $alice.token $attachmentBody 201
        $attachedRetry = Request POST $messagePath $alice.token $attachmentBody
        Assert-True ($attachedRetry.message_id -eq $attached.message_id) 'A receipt must allow retry after consuming the staged upload'
        $link = Request GET "/attachments/$($attached.attachments[0].attachment_id)/url" $bob.token $null
        $authHeaders = @{ Authorization = "Bearer $($bob.token)" }
        $download = Invoke-WebRequest -Uri $link.url -Headers $authHeaders -SkipHttpErrorCheck
        Assert-True ($download.StatusCode -eq 200 -and $download.Content -eq 'SpaceLess attachment') 'Gateway must return the verified object bytes'
        $authHeaders.Range = 'bytes=0-8'
        $range = Invoke-WebRequest -Uri $link.url -Headers $authHeaders -SkipHttpErrorCheck
        Assert-True ($range.StatusCode -eq 206 -and $range.Content -eq 'SpaceLess') 'Gateway must support authorized Range requests'
        $authHeaders.Remove('Range')

        if ($Psql) {
            $staged = Put-Attachment $roomId $alice.token
            $toDelete = Put-Attachment $roomId $alice.token
            $trash = Request POST $messagePath $alice.token @{ client_id = 'to-delete'; content = ''; upload_ids = @($toDelete.upload_id) } 201
            Request DELETE "$messagePath/$($trash.message_id)" $alice.token $null 204 | Out-Null
            $stagedId = [long]$staged.upload_id
            $deletedId = [long]$toDelete.upload_id
            $keptId = [long]$upload.upload_id
            Assert-True ((Test-Query "SELECT staging_key <> storage_key FROM file_uploads WHERE id=$keptId") -eq 't') 'The staged and final objects must have different keys'
            Test-Query "UPDATE file_uploads SET expires_at=now()-interval '1 second',put_expires_at=now()-interval '1 second' WHERE id IN ($stagedId,$deletedId,$keptId)" | Out-Null
        }
    }

    $roomPatch = "/channels/$channel/rooms/$roomId"
    $bobRole = "/channels/$channel/members/$($bob.id)/role"
    Request PATCH $bobRole $alice.token @{ role = 'MODERATOR' } | Out-Null
    Request PATCH $roomPatch $bob.token @{ denied_moderator_ids = @($bob.id) } 403 | Out-Null
    Request PATCH $roomPatch $alice.token @{ denied_moderator_ids = @($bob.id) } | Out-Null
    Wait-Event $socket ROOM_DELETE { param($data) $data.room_id -eq $roomId } | Out-Null
    Request GET $messagePath $bob.token $null 404 | Out-Null
    $deniedSnapshot = Request GET '/bootstrap' $bob.token $null
    Assert-True (!($deniedSnapshot.rooms | Where-Object room_id -eq $roomId)) 'A denied Moderator must not see the room in bootstrap'
    $visibleRoom = Request POST "/channels/$channel/rooms" $alice.token @{
        room_name = 'Visible room'; type = 'TEXT'; is_private = $false; category_id = $category.category_id
    } 201
    $visibleSnapshot = Request GET '/bootstrap' $bob.token $null
    $visiblePosition = ($visibleSnapshot.rooms | Where-Object room_id -eq $visibleRoom.room_id).position
    Assert-True ($visiblePosition -eq 1) 'Bootstrap must keep the visible room at position one'
    $inserted = Request POST "/channels/$channel/rooms" $bob.token @{
        room_name = 'Inserted room'; type = 'TEXT'; is_private = $false; category_id = $category.category_id; position = $visiblePosition
    } 201
    Assert-True ($inserted.position -eq $visiblePosition) 'Create room must accept a position returned by bootstrap'
    $creatorSnapshot = Request GET '/bootstrap' $alice.token $null
    Assert-True (($creatorSnapshot.rooms | Where-Object room_id -eq $roomId).position -eq 0) 'Room insertion must leave the hidden position unchanged'
    Request PATCH $bobRole $alice.token @{ role = 'MEMBER' } | Out-Null
    Wait-Event $socket ROOM_CREATE { param($data) $data.room_id -eq $roomId } | Out-Null
    $memberSnapshot = Request GET '/bootstrap' $bob.token $null
    Assert-True (($memberSnapshot.rooms | Where-Object room_id -eq $roomId).last_read_sequence -eq '1') 'A denial and its removal must preserve the read mark'
    Request PATCH $roomPatch $alice.token @{ member_ids = @() } | Out-Null
    Wait-Event $socket ROOM_DELETE { param($data) $data.room_id -eq $roomId } | Out-Null
    Request GET $messagePath $bob.token $null 404 | Out-Null
    if ($WithFiles) {
        $denied = Invoke-WebRequest -Uri $link.url -Headers $authHeaders -SkipHttpErrorCheck
        Assert-True ($denied.StatusCode -eq 404) 'A previously signed URL must stop working after room access is revoked'
        if ($Psql) {
            $until = [DateTime]::UtcNow.AddSeconds(70)
            do {
                $remaining = Test-Query "SELECT count(*) FROM file_uploads WHERE id IN ($stagedId,$deletedId) OR (id=$keptId AND staging_key IS NOT NULL)"
                if ($remaining -eq '0') { break }
                Start-Sleep -Seconds 1
            } while ([DateTime]::UtcNow -lt $until)
            Assert-True ($remaining -eq '0') 'Worker did not confirm storage deletion for the expired upload and deleted attachment'
            Assert-True ((Test-Query "SELECT status FROM file_uploads WHERE id=$keptId") -eq 'ATTACHED') 'Cleanup must preserve an attachment even after its upload expiration'
            $ownerLink = Request GET "/attachments/$($attached.attachments[0].attachment_id)/url" $alice.token $null
            $kept = Invoke-WebRequest -Uri $ownerLink.url -Headers @{ Authorization = "Bearer $($alice.token)" } -SkipHttpErrorCheck
            Assert-True ($kept.StatusCode -eq 200 -and $kept.Content -eq 'SpaceLess attachment') 'Cleanup must preserve the attached object bytes'
            'PASS: worker confirms object deletion for staged and removed files, and leaves the live attachment readable'
        }
    }
    Request DELETE "/channels/$channel/categories/$($category.category_id)" $alice.token $null 204 | Out-Null
    $snapshot = Request GET '/bootstrap' $alice.token $null
    Assert-True (($snapshot.rooms | Where-Object room_id -eq $roomId).category_id -eq $null) 'Deleting a category must retain its rooms without a category'

    Request POST '/blocks' $bob.token @{ target_user_id = $alice.id } 204 | Out-Null
    $dmPath = "/rooms/$($dm.room_id)/messages"
    Request POST $dmPath $alice.token @{ client_id = 'blocked'; content = 'Blocked' } 403 | Out-Null
    Request GET $dmPath $alice.token $null | Out-Null
    $snapshot = Request GET '/bootstrap' $alice.token $null
    Assert-True ($snapshot.dms[0].is_read_only -and $snapshot.blocks.Count -eq 0) 'A block must make DM read-only without revealing the block direction'

    Request POST "/channels/$channel/transfer-ownership" $alice.token @{ target_user_id = $bob.id } | Out-Null
    Request DELETE "/channels/$channel" $alice.token $null 403 | Out-Null
    Request DELETE "/channels/$channel/members/me" $alice.token $null 204 | Out-Null
    $refreshed = Request POST '/auth/refresh' $bob.token $null
    Request GET '/users/me' $bob.token $null 401 | Out-Null
    Request POST '/auth/refresh' $bob.token $null 401 | Out-Null
    Request GET '/users/me' $refreshed.access_token $null | Out-Null
    Request POST '/auth/logout' $refreshed.access_token $null 204 | Out-Null
    Request GET '/users/me' $refreshed.access_token $null 401 | Out-Null
    if ($Psql) {
        if (!$env:DATABASE_URL) { throw 'DATABASE_URL must point to the API test database when using -Psql' }
        $secondSession = Request POST '/auth/login' '' @{ username = $charlie.name; password = $charlie.password }
        $resetBytes = [Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
        $resetToken = [Convert]::ToHexString($resetBytes).ToLowerInvariant()
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($resetBytes)).ToLowerInvariant()
        $charlieId = [long]$charlie.id
        $fixture = "INSERT INTO password_reset_tokens(token_hash,user_id,expires_at) VALUES(decode('$hash','hex'),$charlieId,now()+interval '1 minute');"
        Test-Query $fixture | Out-Null
        $newPassword = 'new integration-only password'
        $resetBody = @{ token = $resetToken; new_password = $newPassword }
        Request POST '/auth/reset-password' '' $resetBody 204 | Out-Null
        Request POST '/auth/reset-password' '' $resetBody 400 | Out-Null
        Request GET '/users/me' $charlie.token $null 401 | Out-Null
        Request GET '/users/me' $secondSession.access_token $null 401 | Out-Null
        Request POST '/auth/login' '' @{ username = $charlie.name; password = $newPassword } | Out-Null
        'PASS: password reset consumes the token and revokes both previous sessions'
    }
    'PASS: authentication, social, channel permissions, message retry, history gap, read state and WebSocket delivery'
    if ($WithFiles) { 'PASS: presigned PUT, object verification, download, Range, and revocation of an already issued URL' }
} finally { $socket.Dispose() }
