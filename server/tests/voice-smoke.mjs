import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash, createHmac, randomBytes, randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

const [binary, address = '127.0.0.1:58082'] = process.argv.slice(2);
assert(binary && process.env.DATABASE_URL, 'Pass the server binary and set DATABASE_URL to a test database');
const secret = randomBytes(32).toString('hex');
const issuer = 'local-livekit-protocol-fixture';
const removals = [];
const provider = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString() || '{}');
    response.setHeader('content-type', 'application/json');
    if (request.url.endsWith('/ListRooms')) response.end('{"rooms":[]}');
    else if (request.url.endsWith('/RemoveParticipant')) {
        removals.push(body);
        response.end('{}');
    } else {
        response.statusCode = 404;
        response.end('{}');
    }
});
provider.listen(0, '127.0.0.1');
await once(provider, 'listening');

const child = spawn(binary, [], {
    windowsHide: true,
    stdio: 'inherit',
    env: { ...process.env, SERVER_MODE: 'api', SERVER_BIND: address,
        APP_JWT_SECRET: randomBytes(48).toString('hex'), LIVEKIT_URL: `ws://127.0.0.1:${provider.address().port}`,
        LIVEKIT_API_KEY: issuer, LIVEKIT_API_SECRET: secret, LIVEKIT_CLOUD_REVOCATION: 'true' },
});
const exited = once(child, 'exit');
const api = `http://${address}/api/v1`;

async function request(method, path, bearer, body, status = 200) {
    const headers = bearer ? { authorization: `Bearer ${bearer}` } : {};
    if (body !== undefined) headers['content-type'] = 'application/json';
    const response = await fetch(api + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await response.text();
    assert.equal(response.status, status, `${method} ${path}: ${text}`);
    return text ? JSON.parse(text) : undefined;
}

async function webhook(event, room, session, sid, track, status = 204, corrupt = false) {
    const raw = JSON.stringify({ id: randomUUID(), event, room: { name: `room-${room}` },
        participant: { identity: `voice-${session}`, sid }, track });
    const sha256 = createHash('sha256').update(corrupt ? `${raw} ` : raw).digest('base64');
    const head = Buffer.from('{"alg":"HS256","typ":"JWT"}').toString('base64url');
    const claims = Buffer.from(JSON.stringify({ iss: issuer, exp: Math.floor(Date.now() / 1000) + 60, sha256 })).toString('base64url');
    const signature = createHmac('sha256', secret).update(`${head}.${claims}`).digest('base64url');
    const response = await fetch(`${api}/webhooks/livekit`, { method: 'POST', body: raw,
        headers: { 'content-type': 'application/webhook+json', authorization: `${head}.${claims}.${signature}` } });
    assert.equal(response.status, status, await response.text());
}

try {
    for (let attempt = 0; attempt < 60; attempt += 1) {
        const ready = await fetch(`${api}/users/me`).catch(() => null);
        if (ready?.status === 401) break;
        assert(child.exitCode === null && attempt < 59, 'API did not become ready');
        await delay(200);
    }
    const username = `voice${randomBytes(6).toString('hex')}`;
    const password = 'integration-only password';
    await request('POST', '/auth/register', null, { username, password, full_name: username,
        date_of_birth: '1990-01-01', email: `${username}@example.test`, phone: '+84900000000' }, 201);
    const { access_token: oldToken } = await request('POST', '/auth/login', null, { username, password });
    const { access_token: newToken } = await request('POST', '/auth/login', null, { username, password });
    const { channel: { channel_id: channel } } = await request('POST', '/channels', oldToken, { channel_name: username }, 201);
    const { room_id: room } = await request('POST', `/channels/${channel}/rooms`, oldToken,
        { room_name: 'Voice', type: 'VOICE', is_private: false }, 201);
    const voice = `/rooms/${room}/voice`;
    const old = (await request('POST', `${voice}/join`, oldToken)).voice_session_id;
    await webhook('participant_joined', room, old, 'PA_old');
    const current = (await request('POST', `${voice}/join`, newToken)).voice_session_id;
    assert.notEqual(old, current);
    assert(removals.some(({ identity, revoke_token_ts }) => identity === `voice-${old}` && Number(revoke_token_ts) > 0));
    await webhook('participant_joined', room, current, 'PA_current', undefined, 401, true);
    await webhook('participant_joined', room, current, 'PA_current');
    await webhook('participant_left', room, old, 'PA_old');
    await request('DELETE', `${voice}/leave?voice_session_id=${current}`, oldToken, undefined, 204);
    await webhook('track_published', room, current, 'PA_current', { sid: 'TR_screen', source: 'SCREEN_SHARE' });
    await webhook('track_unpublished', room, current, 'PA_current', { sid: 'TR_stale', source: 'SCREEN_SHARE' });
    let snapshot = await request('GET', '/bootstrap', newToken);
    assert.deepEqual(snapshot.voice_states.map(({ voice_session_id, state, screen_video_sid }) =>
        [voice_session_id, state, screen_video_sid]), [[current, 'CONNECTED', 'TR_screen']]);
    await webhook('track_published', room, current, 'PA_current', { sid: 'TR_extra', source: 'SCREEN_SHARE' });
    assert(removals.some(({ identity }) => identity === `voice-${current}`));
    snapshot = await request('GET', '/bootstrap', newToken);
    assert.deepEqual(snapshot.voice_states, []);
    console.log('PASS: signed webhooks, generation and owner checks, track SID fencing, and revocation requests to the protocol fixture');
} finally {
    if (child.exitCode === null) child.kill();
    await exited;
    provider.close();
}
