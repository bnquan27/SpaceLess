import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const image = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMw0Qj4DwAC7AGs0MTr5gAAAABJRU5ErkJggg==',
  'base64',
);

async function makeChannel(request, owner, name) {
  const { body } = await request('POST', '/channels', {
    token: owner.token, body: { channel_name: name }, status: 201,
  });
  return body.channel;
}

async function makeRoom(request, owner, channel_id, type) {
  const { body } = await request('POST', '/channels/{channel_id}/rooms', {
    token: owner.token, params: { channel_id },
    body: { room_name: type + ' test room', type, is_private: false }, status: 201,
  });
  return body.room_id;
}

export async function files({ run, observe, request, account, download, store, loopbackUrl }) {
  const metadata = (bytes, mime_type, file_name) => ({ file_size_bytes: bytes.length, mime_type, file_name });

  async function upload(signed, bytes, status = 200) {
    assert.match(signed.upload_id, /^[1-9]\d*$/);
    assert.equal(signed.required_headers['if-none-match'], '*');
    assert.equal(signed.required_headers['content-length'], String(bytes.length));
    assert(Date.parse(signed.staged_expires_at) > Date.parse(signed.expires_at));
    assert(Date.parse(signed.expires_at) > Date.now());
    const url = loopbackUrl(signed.put_url, store.origin);
    assert(url.searchParams.has('X-Amz-Signature'), 'S3 PUT URL includes a signature');
    const response = await fetch(url, {
      method: 'PUT', headers: signed.required_headers, body: bytes,
      redirect: 'error', signal: AbortSignal.timeout(5_000),
    });
    await response.arrayBuffer();
    assert.equal(response.status, status, 'Object storage conditional PUT status');
  }

  async function imageLink(token, upload_id) {
    const { body } = await request('GET', '/images/{upload_id}/url', { token, params: { upload_id } });
    assert(Date.parse(body.expires_at) > Date.now());
    return body.url;
  }

  await run('Attachment uploads preserve bytes and the gateway serves a range', async () => {
    const owner = await account();
    const { channel_id } = await makeChannel(request, owner, 'Attachments');
    const room_id = await makeRoom(request, owner, channel_id, 'TEXT');
    const { token } = owner;
    const bytes = Buffer.from('SpaceLess attachment integration test');
    const body = metadata(bytes, 'text/plain', 'fixture.txt');
    const template = '/rooms/{room_id}/uploads/presign';
    const params = { room_id };
    await request('POST', template, { token, params, body: { ...body, file_size_bytes: 0 }, status: 400 });
    const { body: signed } = await request('POST', template, { token, params, body });
    await upload(signed, bytes);
    await upload(signed, bytes, 412);
    const { body: message } = await request('POST', '/rooms/{room_id}/messages', {
      token, params, body: { client_id: 'with-file', content: '', upload_ids: [signed.upload_id] }, status: 201,
    });
    const [attachment] = message.attachments;
    assert.equal(attachment.file_name, body.file_name);
    assert.equal(attachment.mime_type, body.mime_type);
    assert.equal(attachment.file_size_bytes, bytes.length);
    const { body: link } = await request('GET', '/attachments/{attachment_id}/url', {
      token, params: { attachment_id: attachment.attachment_id },
    });
    const full = await download(link.url, { token });
    assert.deepEqual(full.body, bytes);
    assert.equal(full.headers.get('content-type'), 'text/plain');
    const partial = await download(link.url, { token, headers: { range: 'bytes=0-8' }, status: 206 });
    assert.deepEqual(partial.body, bytes.subarray(0, 9));
    assert.equal(partial.headers.get('content-range'), `bytes 0-8/${bytes.length}`);
    observe('attachmentBytes', { uploaded: bytes.length, downloaded: full.body.length, ranged: partial.body.length,
      contentRange: partial.headers.get('content-range') });
    await request('DELETE', '/rooms/{room_id}/messages/{message_id}', {
      token, params: { ...params, message_id: message.message_id }, status: 204,
    });
    await download(link.url, { token, status: 404 });
  });

  await run('Avatar and both channel image purposes can be uploaded and read back', async () => {
    const owner = await account();
    const { token } = owner;
    const { channel_id } = await makeChannel(request, owner, 'Channel images');
    const body = metadata(image, 'image/png', 'fixture.png');
    await request('POST', '/users/me/avatar/uploads/presign', {
      token, body: { ...body, mime_type: 'text/plain' }, status: 400,
    });
    const { body: avatar } = await request('POST', '/users/me/avatar/uploads/presign', { token, body });
    await upload(avatar, image);
    const profile = await request('PATCH', '/users/me', { token, body: { avatar_upload_id: avatar.upload_id } });
    assert.equal(profile.body.avatar_url, `/api/v1/images/${avatar.upload_id}/url`);
    const patch = {};
    const channelUrlFields = {};
    for (const purpose of ['ICON', 'BACKGROUND']) {
      const { body: signed } = await request('POST', '/channels/{channel_id}/images/uploads/presign', {
        token, params: { channel_id }, body: { ...body, purpose },
      });
      await upload(signed, image);
      patch[purpose.toLowerCase() + '_upload_id'] = signed.upload_id;
      channelUrlFields[purpose.toLowerCase() + '_url'] = `/api/v1/images/${signed.upload_id}/url`;
    }
    const { body: updated } = await request('PATCH', '/channels/{channel_id}', {
      token, params: { channel_id }, body: patch,
    });
    for (const [field, path] of Object.entries(channelUrlFields)) assert.equal(updated[field], path);
    for (const upload_id of [avatar.upload_id, ...Object.values(patch)]) {
      const response = await download(await imageLink(token, upload_id), { token });
      assert.equal(response.headers.get('content-type'), 'image/png');
      assert.deepEqual(response.body, image);
    }
  });

  await run('A signed-in visitor can open the icon supplied by a public channel preview', async () => {
    const owner = await account();
    const visitor = await account();
    const { channel_id, invite_code } = await makeChannel(request, owner, 'Public channel preview icon');
    const { body: signed } = await request('POST', '/channels/{channel_id}/images/uploads/presign', {
      token: owner.token, params: { channel_id }, body: { ...metadata(image, 'image/png', 'icon.png'), purpose: 'ICON' },
    });
    await upload(signed, image);
    await request('PATCH', '/channels/{channel_id}', {
      token: owner.token, params: { channel_id }, body: { join_mode: 'PUBLIC_LINK', icon_upload_id: signed.upload_id },
    });
    const { body: preview } = await request('GET', '/channel-links/{invite_code}', {
      token: visitor.token, params: { invite_code },
    });
    assert.equal(preview.icon_url, `/api/v1/images/${signed.upload_id}/url`);
    const ownerReads = await download(await imageLink(owner.token, signed.upload_id), { token: owner.token });
    assert.deepEqual(ownerReads.body, image);
    observe('previewIcon', { previewReturnsIconUrl: Boolean(preview.icon_url), ownerDownloadedBytes: ownerReads.body.length });
    const visitorReads = await download(await imageLink(visitor.token, signed.upload_id), { token: visitor.token });
    assert.deepEqual(visitorReads.body, image);
    await request('PATCH', '/channels/{channel_id}', {
      token: owner.token, params: { channel_id }, body: { join_mode: 'INVITE_ONLY' },
    });
    await request('GET', '/images/{upload_id}/url', {
      token: visitor.token, params: { upload_id: signed.upload_id }, status: 404,
    });
    await download(await imageLink(owner.token, signed.upload_id), { token: owner.token });
  });
}

export async function voice({ run, observe, request, account, connect, provider }) {
  const join = async (token, room_id, status = 200, code) =>
    (await request('POST', '/rooms/{room_id}/voice/join', { token, params: { room_id }, status, code })).body;
  const leave = (token, room_id, voice_session_id) => request('DELETE', '/rooms/{room_id}/voice/leave', {
    token, params: { room_id }, query: { voice_session_id }, status: 204,
  });
  const snapshot = async token => (await request('GET', '/bootstrap', { token })).body.voice_states;
  const event = (name, room, session, track) => JSON.stringify({
    id: randomUUID(), event: name, room: { name: `room-${room}` },
    participant: { identity: `voice-${session}`, sid: `PA_${session}` }, track,
  });
  const webhook = raw => request('POST', '/webhooks/livekit', {
    raw, headers: { authorization: provider.sign(raw), 'content-type': 'application/webhook+json' }, status: 204,
  });

  await run('Voice joins, signed webhooks update screen state, and leave removes the session', async () => {
    const owner = await account();
    const { token } = owner;
    const { channel_id } = await makeChannel(request, owner, 'Voice lifecycle');
    const room_id = await makeRoom(request, owner, channel_id, 'VOICE');
    const textRoom = await makeRoom(request, owner, channel_id, 'TEXT');
    await join(token, textRoom, 400);
    const ws = await connect(token);
    try {
      const { voice_session_id, livekit_token, livekit_url, expires_at } = await join(token, room_id);
      assert.match(voice_session_id, /^[1-9]\d*$/);
      assert.equal(livekit_url.replace(/\/$/, ''), provider.env.LIVEKIT_URL);
      assert(typeof livekit_token === 'string' && livekit_token.split('.').length === 3);
      assert(Date.parse(expires_at) > Date.now());
      const issued = (await snapshot(token))[0]?.state;
      assert.equal(issued, 'PENDING');
      const joined = event('participant_joined', room_id, voice_session_id);
      await webhook(joined);
      await webhook(joined);
      const connected = (await snapshot(token))[0]?.state;
      assert.equal(connected, 'CONNECTED');
      const track = { sid: 'TR_fixture_screen', source: 'SCREEN_SHARE' };
      await webhook(event('track_published', room_id, voice_session_id, track));
      const started = await ws.wait('SCREEN_SHARE_START', data => data.voice_session_id === voice_session_id);
      assert.equal((await snapshot(token))[0]?.screen_video_sid, track.sid);
      await webhook(event('track_unpublished', room_id, voice_session_id, track));
      const stopped = await ws.wait('SCREEN_SHARE_STOP', data => data.voice_session_id === voice_session_id);
      assert.equal((await snapshot(token))[0]?.screen_video_sid, null);
      await leave(token, room_id, voice_session_id);
      await leave(token, room_id, voice_session_id);
      const remaining = await snapshot(token);
      const removals = provider.removals.filter(item => item.identity === `voice-${voice_session_id}`).length;
      assert.deepEqual(remaining, []);
      assert.equal(removals, 1);
      observe('voiceLifecycle', { states: [issued, connected], events: [started.type, stopped.type],
        sessionsAfterLeave: remaining.length, providerRemovalCalls: removals });
    } finally {
      ws.close();
    }
  });

  await run('Switching devices revokes the previous voice session and ignores its late leave', async () => {
    const owner = await account();
    const { token } = owner;
    const { body: login } = await request('POST', '/auth/login', {
      body: { username: owner.username, password: owner.password },
    });
    const { channel_id } = await makeChannel(request, owner, 'Voice devices');
    const room_id = await makeRoom(request, owner, channel_id, 'VOICE');
    const old = (await join(token, room_id)).voice_session_id;
    await webhook(event('participant_joined', room_id, old));
    const active = (await join(login.access_token, room_id)).voice_session_id;
    assert.notEqual(active, old);
    assert(provider.removals.some(item => item.identity === `voice-${old}` && Number(item.revoke_token_ts) > 0));
    await webhook(event('participant_left', room_id, old));
    await leave(token, room_id, old);
    assert.deepEqual((await snapshot(login.access_token)).map(item => item.voice_session_id), [active]);
    await leave(login.access_token, room_id, active);
    assert.deepEqual(await snapshot(login.access_token), []);
  });
}
