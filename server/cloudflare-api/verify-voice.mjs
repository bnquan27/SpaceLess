import assert from 'node:assert/strict';
import { argon2Sync, createHash, createHmac, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { isAbsolute } from 'node:path';

const local = new Set(['localhost', '127.0.0.1', '[::1]']);
let step = 'configuration';

function required(key) {
  const value = process.env[key];
  if (!value) throw new Error('Missing environment variable ' + key);
  return value;
}

function localHttp(key) {
  const url = new URL(required(key));
  assert.ok(url.protocol === 'http:' && local.has(url.hostname) && url.port);
  assert.equal(url.pathname, '/');
  assert.ok(!url.username && !url.password && !url.search && !url.hash);
  return url;
}

function token(secret, claims) {
  const head = Buffer.from('{"alg":"HS256","typ":"JWT"}').toString('base64url');
  const payload = Buffer.from(JSON.stringify(claims)).toString('base64url');
  const signature = createHmac('sha256', secret).update(head + '.' + payload).digest('base64url');
  return [head, payload, signature].join('.');
}

function decode(secret, value) {
  const [header, payload, supplied, extra] = value.split('.');
  assert.equal(extra, undefined);
  const expected = createHmac('sha256', secret).update(header + '.' + payload).digest();
  const signature = Buffer.from(supplied, 'base64url');
  assert.ok(signature.length === expected.length && timingSafeEqual(signature, expected));
  assert.equal(JSON.parse(Buffer.from(header, 'base64url')).alg, 'HS256');
  return JSON.parse(Buffer.from(payload, 'base64url'));
}

const now = () => Math.floor(Date.now() / 1000);
const key = () => required('SPACELESS_TEST_LIVEKIT_KEY');
const secret = () => required('SPACELESS_TEST_LIVEKIT_SECRET');

async function runProvider() {
  const address = localHttp('SPACELESS_TEST_LIVEKIT_HTTP');
  const signingKey = key();
  const signingSecret = secret();
  const removals = [];
  const participants = new Map();
  let rejectRemoval = false;
  const provider = createServer(async (request, response) => {
    const reply = (status, body) => {
      response.writeHead(status, { 'content-type': 'application/json' });
      response.end(JSON.stringify(body));
    };
    try {
      const chunks = [];
      let bytes = 0;
      for await (const chunk of request) {
        bytes += chunk.length;
        if (bytes > 65536) {
          reply(413, {});
          return;
        }
        chunks.push(chunk);
      }
      const input = JSON.parse(Buffer.concat(chunks).toString() || '{}');
      if (request.url === '/_fixture') {
        if (request.headers['x-fixture-secret'] !== signingSecret) {
          reply(403, {});
          return;
        }
        if (input.rejectRemoval) rejectRemoval = true;
        if (input.room && input.participant) {
          const room = participants.get(input.room) ?? new Map();
          room.set(input.participant.identity, input.participant);
          participants.set(input.room, room);
        }
        reply(200, { removals });
        return;
      }
      const claims = decode(signingSecret, String(request.headers.authorization).replace(/^Bearer /, ''));
      assert.equal(claims.iss, signingKey);
      assert.ok(claims.exp > now());
      if (request.url === '/twirp/livekit.RoomService/ListRooms') {
        assert.equal(claims.video.roomList, true);
        reply(200, { rooms: [...participants.keys()].map(name => ({ name })) });
      } else if (request.url === '/twirp/livekit.RoomService/ListParticipants') {
        assert.equal(claims.video.roomAdmin, true);
        assert.equal(claims.video.room, input.room);
        reply(200, { participants: [...(participants.get(input.room)?.values() ?? [])] });
      } else if (request.url === '/twirp/livekit.RoomService/RemoveParticipant') {
        assert.equal(claims.video.roomAdmin, true);
        assert.equal(claims.video.room, input.room);
        assert.match(input.room, /^room-[1-9][0-9]*$/);
        assert.match(input.identity, /^voice-[1-9][0-9]*$/);
        assert.ok(Number(input.revoke_token_ts) >= now());
        removals.push({ room: input.room, identity: input.identity, rejected: rejectRemoval });
        if (rejectRemoval) {
          rejectRemoval = false;
          reply(503, {});
        } else {
          participants.get(input.room)?.delete(input.identity);
          reply(200, {});
        }
      } else {
        reply(404, {});
      }
    } catch {
      reply(401, {});
    }
  });
  provider.listen(Number(address.port), address.hostname.replace(/^\[|\]$/g, ''));
  await once(provider, 'listening');
  console.log(JSON.stringify({ fixtureProvider: 'ready', url: address.href.replace(/^http:/, 'ws:') }));
}

function passwordHash(password) {
  const salt = randomBytes(16);
  const digest = argon2Sync('argon2id', {
    message: Buffer.from(password), nonce: salt, parallelism: 1, tagLength: 32, memory: 19456, passes: 2,
  });
  return '$argon2id$v=19$m=19456,t=2,p=1$' +
    [salt, digest].map(value => value.toString('base64').replace(/=+$/, '')).join('$');
}

async function transaction(db, run) {
  await db.query('BEGIN');
  try {
    await run();
    await db.query('COMMIT');
  } catch (error) {
    await db.query('ROLLBACK').catch(() => {});
    throw error;
  }
}

async function verify() {
  const origin = localHttp('SPACELESS_TEST_WORKER_URL');
  const providerUrl = localHttp('SPACELESS_TEST_LIVEKIT_HTTP');
  const gate = required('STAGING_GATE_TOKEN');
  const signingKey = key();
  const signingSecret = secret();
  const connectionString = required('SPACELESS_TEST_DATABASE_URL');
  const database = new URL(connectionString);
  assert.ok(['postgresql:', 'postgres:'].includes(database.protocol));
  assert.ok(local.has(database.hostname) && /^\/spaceless_test_[a-z0-9_]+$/.test(database.pathname));
  const pgPath = required('SPACELESS_STAGING_PG_MODULE');
  assert.ok(isAbsolute(pgPath));
  const { Client } = createRequire(import.meta.url)(pgPath);
  const db = new Client({ connectionString });
  const stamp = 'cfv_' + randomBytes(7).toString('hex');
  const id = (randomBytes(8).readBigUInt64BE() & ((1n << 60n) - 1n)) + (1n << 61n);
  const [user, outsider, channel, room, textRoom] = [0n, 1n, 2n, 3n, 4n].map(offset => (id + offset).toString());
  const rooms = [room, textRoom];
  const usernames = [stamp + '_owner', stamp + '_outside'];
  const password = randomBytes(24).toString('hex');
  const hash = passwordHash(password);
  const sessions = [];
  const webhooks = [];
  const voice = '/rooms/' + room + '/voice';

  async function request(method, path, status, bearer, body, staged = true) {
    const headers = new Headers();
    if (staged) headers.set('X-SpaceLess-Staging-Token', gate);
    if (bearer) headers.set('Authorization', 'Bearer ' + bearer);
    if (body !== undefined) headers.set('Content-Type', 'application/json');
    const result = await fetch(new URL('/api/v1' + path, origin), {
      method, headers, body: body === undefined ? undefined : JSON.stringify(body), redirect: 'error',
      signal: AbortSignal.timeout(15000),
    });
    assert.equal(result.status, status);
    return result.status === 204 ? undefined : await result.json();
  }

  async function fixture(body) {
    const response = await fetch(new URL('/_fixture', providerUrl), {
      method: 'POST', body: JSON.stringify(body), headers: { 'X-Fixture-Secret': signingSecret },
      redirect: 'error', signal: AbortSignal.timeout(10000),
    });
    assert.equal(response.status, 200);
    return await response.json();
  }

  async function webhook(event, generation, sid, track, { eventId = randomUUID(), invalid = false } = {}) {
    webhooks.push(eventId);
    const payload = JSON.stringify({ id: eventId, event, room: { name: 'room-' + room },
      participant: { identity: 'voice-' + generation, sid }, track });
    const sha256 = createHash('sha256').update(payload + (invalid ? ' ' : '')).digest('base64');
    const auth = token(signingSecret, { iss: signingKey, exp: now() + 60, sha256 });
    const response = await fetch(new URL('/api/v1/webhooks/livekit', origin), {
      method: 'POST', body: payload, headers: { 'content-type': 'application/webhook+json', authorization: auth },
      redirect: 'error', signal: AbortSignal.timeout(15000),
    });
    assert.equal(response.status, invalid ? 401 : 204);
    return eventId;
  }

  const login = async username =>
    (await request('POST', '/auth/login', 200, null, { username, password })).access_token;

  async function join(bearer) {
    const joined = await request('POST', voice + '/join', 200, bearer);
    assert.equal(typeof joined.voice_session_id, 'string');
    assert.ok(BigInt(joined.voice_session_id) > BigInt(Number.MAX_SAFE_INTEGER));
    assert.equal(new URL(joined.livekit_url).origin, providerUrl.origin.replace(/^http:/, 'ws:'));
    const claims = decode(signingSecret, joined.livekit_token);
    assert.equal(claims.iss, signingKey);
    assert.equal(claims.sub, 'voice-' + joined.voice_session_id);
    assert.equal(claims.video.room, 'room-' + room);
    assert.equal(claims.video.roomJoin, true);
    assert.equal(claims.video.canPublishData, false);
    assert.equal(claims.video.roomAdmin, undefined);
    assert.deepEqual(claims.video.canPublishSources, ['microphone', 'camera', 'screen_share', 'screen_share_audio']);
    assert.equal(Date.parse(joined.expires_at) / 1000, claims.exp);
    sessions.push(joined.voice_session_id);
    return joined.voice_session_id;
  }

  async function state(bearer) {
    const { voice_states } = await request('GET', '/bootstrap', 200, bearer);
    return voice_states.filter(value => value.room_id === room)
      .map(value => [value.voice_session_id, value.state, value.screen_video_sid]);
  }

  let seeded = false;
  step = 'fixture_database';
  await db.connect();
  try {
    await transaction(db, async () => {
      for (const [userId, username] of [[user, usernames[0]], [outsider, usernames[1]]]) {
        await db.query('INSERT INTO users (id,username,full_name,password_hash) VALUES ($1,$2::text,$2::text,$3)',
          [userId, username, hash]);
        await db.query("INSERT INTO user_details (user_id,date_of_birth,phone,email) VALUES ($1,'2000-01-01','0900000000',$2)",
          [userId, username + '@example.invalid']);
      }
      await db.query('INSERT INTO channels (id,creator_id,name,invite_code) VALUES ($1,$2,$3::text,$3::text)', [channel,user,stamp]);
      await db.query('INSERT INTO channel_members (channel_id,user_id,role_id) VALUES ($1,$2,1)', [channel,user]);
      await db.query("INSERT INTO rooms (id,channel_id,name,kind,private,position) VALUES ($1,$3,$4,'VOICE',true,0),($2,$3,$5,'TEXT',false,1)",
        [room,textRoom,channel,'Voice','Text']);
    });
    seeded = true;
    const previousBearer = await login(usernames[0]);
    const nextBearer = await login(usernames[0]);
    const noAccess = await login(usernames[1]);

    step = 'voice_room_permissions';
    await request('POST', voice + '/join', 403, previousBearer, undefined, false);
    await request('POST', voice + '/join', 401);
    const denied = await request('POST', voice + '/join', 404, noAccess);
    assert.equal(denied.error.code, 'NOT_FOUND');
    const wrongRoom = await request('POST', '/rooms/' + textRoom + '/voice/join', 400, previousBearer);
    assert.equal(wrongRoom.error.code, 'VOICE_ROOM_REQUIRED');
    assert.equal((await request('DELETE', voice + '/leave?voice_session_id=0001', 400, previousBearer)).error.code, 'INVALID_QUERY');
    await request('POST', '/webhooks/livekit', 401, undefined, {}, false);

    step = 'signed_and_idempotent_webhook';
    const first = await join(previousBearer);
    assert.deepEqual(await state(previousBearer), [[first, 'PENDING', null]]);
    await webhook('participant_joined', first, 'PA_previous', undefined, { invalid: true });
    assert.deepEqual(await state(previousBearer), [[first, 'PENDING', null]]);
    await fixture({ room: 'room-' + room, participant: { identity: 'voice-' + first, sid: 'PA_previous' } });
    const receipt = await webhook('participant_joined', first, 'PA_previous');
    await webhook('participant_left', first, 'PA_previous', undefined, { eventId: receipt });
    assert.deepEqual(await state(previousBearer), [[first, 'CONNECTED', null]]);

    step = 'device_switch_and_provider_retry';
    await fixture({ rejectRemoval: true });
    await request('POST', voice + '/join', 503, nextBearer);
    assert.deepEqual(await state(nextBearer), []);
    const current = await join(nextBearer);
    assert.notEqual(first, current);
    const { removals } = await fixture({});
    assert.deepEqual(removals.filter(call => call.identity === 'voice-' + first).map(call => call.rejected),
      [true, false]);

    step = 'old_generation_and_screen_tracks';
    await fixture({ room: 'room-' + room,
      participant: { identity: 'voice-' + current, sid: 'PA_current', tracks: [{ sid: 'TR_live', source: 'SCREEN_SHARE' }] } });
    await webhook('participant_joined', current, 'PA_current');
    await webhook('participant_left', first, 'PA_previous');
    await webhook('participant_left', current, 'PA_previous');
    await request('DELETE', voice + '/leave?voice_session_id=' + current, 204, previousBearer);
    await webhook('track_published', current, 'PA_current', { sid: 'TR_live', source: 'SCREEN_SHARE' });
    await webhook('track_unpublished', current, 'PA_current', { sid: 'TR_previous', source: 3 });
    assert.deepEqual(await state(nextBearer), [[current, 'CONNECTED', 'TR_live']]);
    const refreshed = (await request('POST', '/auth/refresh', 200, nextBearer)).access_token;
    await request('GET', '/bootstrap', 401, nextBearer);
    assert.deepEqual(await state(refreshed), [[current, 'CONNECTED', 'TR_live']]);
    await webhook('track_published', current, 'PA_current', { sid: 'TR_duplicate', source: 3 });
    assert.deepEqual(await state(refreshed), []);
    assert.ok((await fixture({})).removals.some(call => call.identity === 'voice-' + current && !call.rejected));

    step = 'leave_and_logout';
    const next = await join(refreshed);
    await request('DELETE', voice + '/leave?voice_session_id=' + current, 204, refreshed);
    assert.deepEqual(await state(refreshed), [[next, 'PENDING', null]]);
    await request('DELETE', voice + '/leave?voice_session_id=' + next, 204, refreshed);
    await request('DELETE', voice + '/leave?voice_session_id=' + next, 204, refreshed);
    assert.deepEqual(await state(refreshed), []);
    const logoutVoice = await join(refreshed);
    await request('POST', '/auth/logout', 204, refreshed);
    assert.ok((await fixture({})).removals.some(call => call.identity === 'voice-' + logoutVoice && !call.rejected));
    assert.deepEqual(await state(previousBearer), []);

    step = 'room_deletion_revocation';
    const removedRoomVoice = await join(previousBearer);
    await fixture({ room: 'room-' + room,
      participant: { identity: 'voice-' + removedRoomVoice, sid: 'PA_deleted_room' } });
    await webhook('participant_joined', removedRoomVoice, 'PA_deleted_room');
    await request('DELETE', '/channels/' + channel + '/rooms/' + room, 204, previousBearer);
    assert.ok((await fixture({})).removals.some(call => call.identity === 'voice-' + removedRoomVoice && !call.rejected));
    assert.equal((await request('POST', voice + '/join', 404, previousBearer)).error.code, 'NOT_FOUND');
  } finally {
    try {
      if (seeded) await transaction(db, async () => {
        const result = await db.query('SELECT creator_id::text,name FROM channels WHERE id=$1 FOR UPDATE', [channel]);
        assert.deepEqual(result.rows, [{ creator_id: user, name: stamp }]);
        const streams = ['channel:' + channel, 'user:' + user, 'user:' + outsider,
          ...rooms.map(value => 'room:' + value), ...sessions.map(value => 'voice-session:' + value)];
        await db.query('DELETE FROM outbox_events WHERE stream=ANY($1)', [streams]);
        await db.query('DELETE FROM stream_revisions WHERE stream=ANY($1)', [streams]);
        await db.query('DELETE FROM media_jobs WHERE room_id=ANY($1)', [rooms]);
        await db.query('DELETE FROM livekit_webhook_receipts WHERE event_id=ANY($1)', [webhooks]);
        await db.query('DELETE FROM voice_sessions WHERE room_id=ANY($1)', [rooms]);
        await db.query('DELETE FROM rooms WHERE id=ANY($1) AND channel_id=$2', [rooms,channel]);
        await db.query('DELETE FROM channel_members WHERE channel_id=$1', [channel]);
        await db.query('DELETE FROM channels WHERE id=$1 AND creator_id=$2 AND name=$3', [channel,user,stamp]);
        for (const table of ['revoked_tokens', 'auth_sessions', 'user_details']) {
          await db.query('DELETE FROM ' + table + ' WHERE user_id=ANY($1)', [[user,outsider]]);
        }
        const deleted = await db.query('DELETE FROM users WHERE id=ANY($1) AND username=ANY($2)', [[user,outsider],usernames]);
        assert.equal(deleted.rowCount, 2);
      });
    } finally {
      await db.end();
    }
  }
  console.log(JSON.stringify({ outcome: 'passed', checks: [
    'auth_and_voice_room_permissions', 'scoped_provider_join_token', 'signed_webhook_and_duplicate_delivery',
    'revocation_retry_and_device_switch', 'stale_participant_and_track', 'refresh_keeps_voice',
    'idempotent_leave', 'logout_revokes_media', 'room_deletion_revokes_media',
  ] }));
}

try {
  if (process.argv[2] === '--fixture-provider') await runProvider();
  else await verify();
} catch (error) {
  console.error(JSON.stringify({ outcome: 'failed', step, code: error.code ?? error.name,
    status: Number.isInteger(error.actual) ? error.actual : undefined }));
  process.exitCode = 1;
}
