import assert from 'node:assert/strict';
import { argon2Sync, randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { isAbsolute, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const require = createRequire(import.meta.url);
const sampleRate = 48000;
const samples = 480;
const width = 640;
const height = 360;
const checks = [];
let step = 'configuration';

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`Missing ${name}`);
  return value;
}

function origin(name, pattern, protocol) {
  const url = new URL(required(name));
  assert.equal(url.protocol, protocol);
  assert.match(url.hostname, pattern);
  assert.equal(url.pathname, '/');
  assert.ok(!url.username && !url.password && !url.port && !url.search && !url.hash);
  return url;
}

function check(name, data = {}) {
  checks.push(name);
  console.log(JSON.stringify({ check: name, outcome: 'passed', ...data }));
}

async function bound(promise, timeoutMs) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Timed out')), timeoutMs);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

async function until(predicate, timeoutMs, intervalMs = 100) {
  const deadline = Date.now() + timeoutMs;
  do {
    const value = await predicate();
    if (value) return value;
    await delay(intervalMs);
  } while (Date.now() < deadline);
  throw new Error('Timed out waiting for expected state');
}

async function transaction(db, work) {
  await db.query('BEGIN');
  try {
    const result = await work();
    await db.query('COMMIT');
    return result;
  } catch (error) {
    await db.query('ROLLBACK').catch(() => {});
    throw error;
  }
}

function fixture() {
  const mark = 'cflk_' + randomBytes(7).toString('hex');
  const base = (randomBytes(8).readBigUInt64BE() & ((1n << 60n) - 1n)) + (1n << 61n);
  const [first, second, outside, channel, room] = [0n, 1n, 2n, 3n, 4n].map(offset => String(base + offset));
  const users = [first, second, outside].map((id, index) => ({ id, name: `${mark}_${index}` }));
  const password = randomBytes(24).toString('base64url');
  const nonce = randomBytes(16);
  const digest = argon2Sync('argon2id', {
    message: Buffer.from(password), nonce, parallelism: 1, tagLength: 32, memory: 19456, passes: 2,
  });
  const components = [nonce, digest].map(bytes => bytes.toString('base64').replace(/=+$/, ''));
  const passwordHash = '$argon2id$v=19$m=19456,t=2,p=1$' + components.join('$');
  return { mark, users, channel, room, password, passwordHash };
}

async function seed(db, { mark, users, channel, room, passwordHash }) {
  await transaction(db, async () => {
    for (const { id, name } of users) {
      await db.query('INSERT INTO public.users (id,username,full_name,password_hash) VALUES ($1,$2::text,$2::text,$3)',
        [id, name, passwordHash]);
      await db.query("INSERT INTO public.user_details (user_id,date_of_birth,phone,email) VALUES ($1,'2000-01-01','0900000000',$2)",
        [id, `${name}@example.invalid`]);
    }
    await db.query('INSERT INTO public.channels (id,creator_id,name,invite_code) VALUES ($1,$2,$3::text,$3::text)',
      [channel, users[0].id, mark]);
    await db.query('INSERT INTO public.channel_members (channel_id,user_id,role_id) VALUES ($1,$2,1),($1,$3,3)',
      [channel, users[0].id, users[1].id]);
    await db.query("INSERT INTO public.rooms (id,channel_id,name,kind,private,position) VALUES ($1,$2,$3,'VOICE',true,0)",
      [room, channel, mark]);
    await db.query('INSERT INTO public.room_members (room_id,user_id) VALUES ($1,$2)', [room, users[1].id]);
  });
}

async function cleanup(db, { mark, users, channel, room }, sessions) {
  await transaction(db, async () => {
    const locked = await db.query('SELECT creator_id::text,name FROM public.channels WHERE id=$1 FOR UPDATE', [channel]);
    assert.deepEqual(locked.rows, [{ creator_id: users[0].id, name: mark }]);
    const streams = [`channel:${channel}`, `room:${room}`, ...users.map(({ id }) => `user:${id}`),
      ...sessions.map(id => `voice-session:${id}`)];
    await db.query('DELETE FROM public.outbox_events WHERE stream=ANY($1::text[])', [streams]);
    await db.query('DELETE FROM public.stream_revisions WHERE stream=ANY($1::text[])', [streams]);
    await db.query('DELETE FROM public.media_jobs WHERE room_id=$1', [room]);
    await db.query('DELETE FROM public.voice_sessions WHERE room_id=$1 AND user_id=ANY($2::bigint[])',
      [room, users.map(({ id }) => id)]);
    await db.query('DELETE FROM public.room_members WHERE room_id=$1 AND user_id=$2', [room, users[1].id]);
    const deletedRoom = await db.query('DELETE FROM public.rooms WHERE id=$1 AND channel_id=$2 AND name=$3',
      [room, channel, mark]);
    assert.equal(deletedRoom.rowCount, 1);
    await db.query('DELETE FROM public.channel_members WHERE channel_id=$1 AND user_id=ANY($2::bigint[])',
      [channel, users.map(({ id }) => id)]);
    await db.query('DELETE FROM public.channels WHERE id=$1 AND creator_id=$2 AND name=$3',
      [channel, users[0].id, mark]);
    for (const { id, name } of users) {
      await db.query('DELETE FROM public.revoked_tokens WHERE user_id=$1', [id]);
      await db.query('DELETE FROM public.auth_sessions WHERE user_id=$1', [id]);
      await db.query('DELETE FROM public.user_details WHERE user_id=$1 AND email=$2', [id, `${name}@example.invalid`]);
      const deletedUser = await db.query('DELETE FROM public.users WHERE id=$1 AND username=$2', [id, name]);
      assert.equal(deletedUser.rowCount, 1);
    }
  });
}

async function consume(stream, goodFrame, needed) {
  const reader = stream.getReader();
  let matching = 0;
  try {
    await bound((async () => {
      while (matching < needed) {
        const { value, done } = await reader.read();
        assert.ok(!done, 'The track stopped before the expected decoded frames arrived');
        if (goodFrame(value)) matching++;
      }
    })(), 18000);
    return matching;
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

function audioMatch(frame, frequency, otherFrequency) {
  const amplitude = hz => {
    let sin = 0;
    let cos = 0;
    for (let i = 0; i < frame.data.length; i++) {
      const angle = i * 2 * Math.PI * hz / sampleRate;
      sin += frame.data[i] * Math.sin(angle);
      cos += frame.data[i] * Math.cos(angle);
    }
    return Math.hypot(sin, cos) / frame.data.length;
  };
  const wanted = amplitude(frequency);
  return wanted > 450 && wanted > 1.6 * amplitude(otherFrequency);
}

function screenMatch({ frame }, seen, bufferType) {
  if (frame.width !== width || frame.height !== height) return false;
  const pixels = (frame.type === bufferType.I420 ? frame : frame.convert(bufferType.I420)).data;
  const row = Math.floor(height / 2) * width;
  const difference = pixels[row + width / 4] - pixels[row + 3 * width / 4];
  if (Math.abs(difference) < 80) return false;
  seen.add(Math.sign(difference));
  return true;
}

async function sendAudio(rtc, source, hz, signal) {
  let offset = 0;
  while (!signal.aborted) {
    const tone = Int16Array.from({ length: samples }, (_, i) =>
      Math.round(10000 * Math.sin((offset + i) * 2 * Math.PI * hz / sampleRate)));
    await source.captureFrame(new rtc.AudioFrame(tone, sampleRate, 1, samples));
    offset += samples;
    await delay(10);
  }
}

async function sendScreen(rtc, source, signal) {
  let index = 0;
  while (!signal.aborted) {
    const pixels = new Uint8Array(width * height * 3 / 2).fill(128);
    const [left, right] = Math.floor(index++ / 3) % 2 ? [215, 35] : [35, 215];
    for (let y = 0; y < height; y++) {
      pixels.fill(left, y * width, y * width + width / 2);
      pixels.fill(right, y * width + width / 2, (y + 1) * width);
    }
    source.captureFrame(new rtc.VideoFrame(pixels, width, height, rtc.VideoBufferType.I420));
    await delay(67);
  }
}

async function main() {
  const api = origin('SPACELESS_STAGING_API_URL', /^spaceless-staging-api\.[a-z0-9-]+\.workers\.dev$/, 'https:');
  const provider = origin('LIVEKIT_URL', /^[a-z0-9-]+\.livekit\.cloud$/, 'wss:');
  const supabase = origin('SUPABASE_URL', /^[a-z0-9]+\.supabase\.co$/, 'https:');
  const gate = required('STAGING_GATE_TOKEN');
  assert.ok(gate.length >= 32);
  const database = new URL(process.env.SPACELESS_STAGING_DATABASE_URL ?? required('DATABASE_URL'));
  assert.ok(['postgres:', 'postgresql:'].includes(database.protocol));
  assert.match(database.hostname, /\.pooler\.supabase\.com$/);
  assert.equal(database.port, '5432');
  assert.equal(database.pathname, '/postgres');
  assert.equal(decodeURIComponent(database.username), 'postgres.' + supabase.hostname.split('.')[0]);
  const moduleRoot = required('SPACELESS_LIVEKIT_SMOKE_MODULES');
  assert.ok(isAbsolute(moduleRoot));
  const rtc = require(join(moduleRoot, '@livekit/rtc-node'));
  const { Client } = require(join(moduleRoot, 'pg'));
  const { WebSocket } = require(join(moduleRoot, 'ws'));
  const db = new Client({
    host: database.hostname, port: Number(database.port), user: decodeURIComponent(database.username),
    password: decodeURIComponent(database.password), database: 'postgres', connectionTimeoutMillis: 12000,
    ssl: { ca: readFileSync(required('SPACELESS_STAGING_CA'), 'utf8'), rejectUnauthorized: true },
    statement_timeout: 20000,
  });

  const example = fixture();
  const endpoint = `/rooms/${example.room}/voice`;
  const clients = [];
  const sessions = [];
  const sources = [];
  const tracks = [];
  const pumps = [];
  const controller = new AbortController();
  let dbConnected = false;
  let seeded = false;
  let socket;

  async function request(method, path, status, token, body) {
    const headers = new Headers({ 'X-SpaceLess-Staging-Token': gate });
    if (token) headers.set('Authorization', `Bearer ${token}`);
    if (body !== undefined) headers.set('Content-Type', 'application/json');
    const response = await fetch(new URL('/api/v1' + path, api), {
      method, headers, body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(20000), redirect: 'error',
    });
    if (response.status !== status) {
      throw Object.assign(new Error('Unexpected API response'), { status: response.status, expected: status });
    }
    return response.status === 204 ? undefined : await response.json();
  }

  async function publish(client, track, source) {
    tracks.push(track);
    const options = new rtc.TrackPublishOptions();
    options.source = source;
    options.simulcast = false;
    const result = await bound(client.localParticipant.publishTrack(track, options), 12000);
    assert.ok(result.sid);
    return result;
  }

  function pump(fn) {
    const promise = fn();
    promise.catch(() => {});
    pumps.push(promise);
    return promise;
  }

  try {
    step = 'fixture_database';
    await db.connect();
    dbConnected = true;
    await seed(db, example);
    seeded = true;

    step = 'join_authorization';
    const tokens = await Promise.all(example.users.map(async ({ name }) => {
      const { access_token } = await request('POST', '/auth/login', 200, null,
        { username: name, password: example.password });
      assert.equal(typeof access_token, 'string');
      return access_token;
    }));
    const denied = await request('POST', `${endpoint}/join`, 404, tokens[2]);
    assert.equal(denied.error?.code, 'NOT_FOUND');
    check('private_room_denies_outsider');

    const events = [];
    const socketUrl = new URL('/api/v1/ws', api);
    socketUrl.protocol = 'wss:';
    socket = new WebSocket(socketUrl, { handshakeTimeout: 12000, maxPayload: 64 * 1024,
      headers: { 'X-SpaceLess-Staging-Token': gate, Authorization: `Bearer ${tokens[1]}` } });
    socket.on('message', buffer => {
      try {
        const event = JSON.parse(buffer);
        if (event.data?.room_id === example.room) events.push(event);
      } catch { return; }
    });
    await bound(once(socket, 'open'), 12000);
    socket.on('error', () => {});

    step = 'cloud_signaling';
    const grants = await Promise.all(tokens.slice(0, 2).map(async token => {
      const grant = await request('POST', `${endpoint}/join`, 200, token);
      assert.match(grant.voice_session_id, /^[1-9]\d*$/);
      assert.equal(new URL(grant.livekit_url).origin, provider.origin);
      assert.equal(typeof grant.livekit_token, 'string');
      const stored = await db.query('SELECT 1 FROM public.voice_sessions WHERE id=$1 AND room_id=$2',
        [grant.voice_session_id, example.room]);
      assert.equal(stored.rowCount, 1, 'The session must belong to the synthetic room');
      sessions.push(grant.voice_session_id);
      return grant;
    }));
    const remote = [new Map(), new Map()];
    const disconnected = new Set();
    for (let i = 0; i < 2; i++) {
      const room = new rtc.Room();
      room.on(rtc.RoomEvent.TrackSubscribed, (track, publication, participant) => {
        remote[i].set(`${participant.identity}:${publication.source}`, { track, sid: publication.sid });
      });
      room.on(rtc.RoomEvent.ParticipantDisconnected, participant => disconnected.add(participant.identity));
      clients.push(room);
    }
    await bound(Promise.all(clients.map((room, i) =>
      room.connect(grants[i].livekit_url, grants[i].livekit_token, { autoSubscribe: true }))), 20000);
    const identities = grants.map(grant => 'voice-' + grant.voice_session_id);
    clients.forEach((room, i) => {
      assert.equal(room.name, `room-${example.room}`);
      assert.equal(room.localParticipant.identity, identities[i]);
    });
    check('two_cloud_clients_connected');

    if (!process.argv.includes('--media-only')) {
      step = 'worker_participant_webhooks';
      await until(async () => {
        const snapshot = await request('GET', '/bootstrap', 200, tokens[1]);
        return grants.every(grant => snapshot.voice_states.some(state =>
          state.voice_session_id === grant.voice_session_id && state.state === 'CONNECTED'));
      }, 12000, 400);
      check('worker_reports_both_participants_connected');
    }

    step = 'bidirectional_voice';
    const audio = clients.map(() => new rtc.AudioSource(sampleRate, 1, 120));
    sources.push(...audio);
    await Promise.all(clients.map((client, i) => publish(client,
      rtc.LocalAudioTrack.createAudioTrack(`microphone-${i}`, audio[i]), rtc.TrackSource.SOURCE_MICROPHONE)));
    const frequencies = [440, 730];
    const voicePumps = audio.map((source, i) => pump(() => sendAudio(rtc, source, frequencies[i], controller.signal)));
    const inboundAudio = await Promise.race([
      Promise.all(remote.map(async (subscriptions, i) => {
        const { track } = await until(() => subscriptions.get(
          `${identities[1 - i]}:${rtc.TrackSource.SOURCE_MICROPHONE}`), 15000);
        return consume(new rtc.AudioStream(track, sampleRate, 1),
          frame => audioMatch(frame, frequencies[1 - i], frequencies[i]), 8);
      })),
      ...voicePumps,
    ]);
    assert.deepEqual(inboundAudio, [8, 8]);
    check('live_voice_two_directions', { decodedAudioFramesPerClient: inboundAudio });

    step = 'live_screen_video';
    const videoSource = new rtc.VideoSource(width, height);
    sources.push(videoSource);
    const publication = await publish(clients[0], rtc.LocalVideoTrack.createVideoTrack('synthetic-screen', videoSource),
      rtc.TrackSource.SOURCE_SCREENSHARE);
    const seen = new Set();
    const videoPump = pump(() => sendScreen(rtc, videoSource, controller.signal));
    const videoFrames = await Promise.race([
      (async () => {
        const { track, sid } = await until(() => remote[1].get(`${identities[0]}:${rtc.TrackSource.SOURCE_SCREENSHARE}`), 15000);
        assert.equal(sid, publication.sid);
        return consume(new rtc.VideoStream(track),
          frame => screenMatch(frame, seen, rtc.VideoBufferType) && seen.size === 2, 5);
      })(),
      videoPump,
    ]);
    assert.equal(videoFrames, 5);
    check('live_screen_share', { decodedVideoFrames: videoFrames, width, height, observedPatterns: seen.size });

    const expectEvent = (type, id, match) => until(() => events.find(event =>
      event.type === type && event.data.voice_session_id === id && match(event.data)), 17000);
    if (!process.argv.includes('--media-only')) {
      step = 'worker_screen_started';
      try {
        await expectEvent('SCREEN_SHARE_START', grants[0].voice_session_id, data => data.track_sid === publication.sid);
      } catch (error) {
        const { rows } = await db.query(
          `SELECT kind, dispatched_at IS NOT NULL AS dispatched,
             data->'payload'->>'track_sid'=$2 AS track_matches FROM public.outbox_events
           WHERE stream=$1 AND kind LIKE 'SCREEN_SHARE_%' ORDER BY id`,
          [`room:${example.room}`, publication.sid],
        );
        const snapshot = await request('GET', '/bootstrap', 200, tokens[1]);
        const current = snapshot.voice_states.find(state => state.voice_session_id === grants[0].voice_session_id);
        console.error(JSON.stringify({ screenEventDiagnostic: true, workerState: current?.state,
          expectedTrackInSnapshot: current?.screen_video_sid === publication.sid,
          roomEventTypes: events.map(event => event.type), outbox: rows }));
        throw error;
      }
      const snapshot = await request('GET', '/bootstrap', 200, tokens[1]);
      assert.ok(snapshot.voice_states.some(state => state.voice_session_id === grants[0].voice_session_id
        && state.state === 'CONNECTED' && state.screen_video_sid === publication.sid));
      check('worker_screen_share_start_event_and_state');
    }

    step = 'unpublish_and_leave';
    controller.abort();
    await bound(Promise.all(pumps), 6000);
    await bound(clients[0].localParticipant.unpublishTrack(publication.sid, false), 12000);
    if (!process.argv.includes('--media-only')) {
      await expectEvent('SCREEN_SHARE_STOP', grants[0].voice_session_id, data => data.track_sid === publication.sid);
      check('worker_screen_share_stop_event');
    }
    await request('DELETE', `${endpoint}/leave?voice_session_id=${grants[0].voice_session_id}`, 204, tokens[0]);
    await until(() => disconnected.has(identities[0]), 12000);
    await request('DELETE', `${endpoint}/leave?voice_session_id=${grants[1].voice_session_id}`, 204, tokens[1]);
    await until(async () => {
      const snapshot = await request('GET', '/bootstrap', 200, tokens[1]);
      return !snapshot.voice_states.some(state => state.room_id === example.room);
    }, 14000, 500);
    check('leave_disconnects_cloud_participant_and_clears_worker_state');
  } finally {
    controller.abort();
    await bound(Promise.allSettled(pumps), 6000).catch(() => {});
    socket?.terminate();
    try {
      await bound(Promise.allSettled(clients.map(room => room.disconnect())), 10000);
      await bound(Promise.allSettled([...tracks.map(track => track.close(false)),
        ...sources.map(source => source.close())]), 6000);
    } finally {
      try {
        if (seeded) {
          await cleanup(db, example, sessions);
          check('synthetic_rows_removed');
        }
      } catch (error) {
        console.error(JSON.stringify({ outcome: 'cleanup_failed', fixture: example.mark, code: error.code ?? error.name }));
        throw error;
      } finally {
        if (dbConnected) await db.end();
        await bound(rtc.dispose(), 8000).catch(() => {});
      }
    }
  }
  console.log(JSON.stringify({ outcome: 'passed', checks }));
}

main().catch(error => {
  console.error(JSON.stringify({ outcome: 'failed', step, code: error.code ?? error.name,
    status: error.status, expected: error.expected, checks }));
  process.exitCode = 1;
});
