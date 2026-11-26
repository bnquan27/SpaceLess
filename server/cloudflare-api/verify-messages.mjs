import assert from 'node:assert/strict';
import { argon2Sync, randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

function required(name) {
  const value = process.env[name];
  if (!value) throw Object.assign(new Error('Missing environment variable'), { code: 'MISSING_ENV', setting: name });
  return value;
}

const require = createRequire(import.meta.url);
function stagingModule(name) {
  const path = required(name);
  assert.ok(isAbsolute(path));
  return require(path);
}

function fixturePasswordHash(password) {
  const salt = randomBytes(16);
  const hash = argon2Sync('argon2id', {
    message: Buffer.from(password), nonce: salt, parallelism: 1, tagLength: 32,
    memory: 19456, passes: 2,
  });
  const encode = bytes => bytes.toString('base64').replace(/=+$/, '');
  return '$argon2id$v=19$m=19456,t=2,p=1$' + encode(salt) + '$' + encode(hash);
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

function failure(error) {
  return {
    code: typeof error.code === 'string' && /^[A-Z0-9_]{2,40}$/.test(error.code) ? error.code : 'ERROR',
    setting: error.code === 'MISSING_ENV' ? error.setting : undefined,
    status: Number.isInteger(error.actual) && error.actual >= 100 && error.actual <= 599 ? error.actual : undefined,
    handshakeStatus: Number(error.message?.match(/^Unexpected server response: (\d+)$/)?.[1]) || undefined,
  };
}

let step = 'load_staging_configuration';
async function verify() {
  const api = new URL(required('SPACELESS_STAGING_API_URL'));
  assert.ok(api.protocol === 'https:' && /^spaceless-staging-api\.[a-z0-9-]+\.workers\.dev$/.test(api.hostname));
  assert.ok(api.pathname === '/' && !api.port && !api.username && !api.password && !api.search && !api.hash);
  const database = new URL(required('SPACELESS_STAGING_DATABASE_URL'));
  assert.ok(['postgres:', 'postgresql:'].includes(database.protocol));
  assert.ok(/\.pooler\.supabase\.com$/.test(database.hostname) && database.port === '5432');
  const { Client } = stagingModule('SPACELESS_STAGING_PG_MODULE');
  const { WebSocket } = stagingModule('SPACELESS_STAGING_WS_MODULE');
  const dbConfig = {
    host: database.hostname,
    port: Number(database.port),
    user: decodeURIComponent(database.username),
    password: decodeURIComponent(database.password),
    database: decodeURIComponent(database.pathname.slice(1)),
    ssl: { ca: readFileSync(required('SPACELESS_STAGING_CA'), 'utf8'), rejectUnauthorized: true },
    connectionTimeoutMillis: 15000,
    statement_timeout: 15000,
    query_timeout: 20000,
  };
  const gate = required('STAGING_GATE_TOKEN');
  assert.ok(gate.length >= 32);

  const marker = 'cfmsg_' + randomBytes(6).toString('hex');
  const base = (randomBytes(8).readBigUInt64BE() & ((1n << 60n) - 1n)) | (1n << 61n);
  const id = offset => (base + BigInt(offset)).toString();
  const actors = ['sender', 'reader', 'outside'].map((role, index) => ({
    id: id(index), username: `${marker}_${role}`,
  }));
  const [sender, reader, outsider] = actors;
  const actorIds = actors.map(actor => actor.id);
  const channelId = id(3);
  const textRoom = id(4);
  const dmRoom = id(5);
  const roomIds = [textRoom, dmRoom];
  const streams = roomIds.map(room => `room:${room}`);
  const password = randomBytes(24).toString('base64url');
  const passwordHash = fixturePasswordHash(password);

  async function request(path, status, { actor, body, code } = {}) {
    const headers = new Headers({ 'X-SpaceLess-Staging-Token': gate });
    if (actor) headers.set('Authorization', 'Bearer ' + actor.token);
    if (body) headers.set('Content-Type', 'application/json');
    const response = await fetch(new URL('/api/v1' + path, api), {
      method: body ? 'POST' : 'GET', headers, body: body && JSON.stringify(body),
      redirect: 'error', signal: AbortSignal.timeout(25000),
    });
    const payload = await response.json().catch(() => null);
    assert.equal(response.status, status);
    if (code) assert.equal(payload?.error?.code, code);
    return payload;
  }

  const roomPath = room => `/rooms/${room}/messages`;
  const history = (actor, room, query = '') => request(roomPath(room) + query, 200, { actor });
  const send = (actor, room, body, status = 201, code) => request(roomPath(room), status, { actor, body, code });

  const sockets = [];
  async function openSocket(actor) {
    const endpoint = new URL('/api/v1/ws', api);
    endpoint.protocol = 'wss:';
    const socket = new WebSocket(endpoint, {
      headers: { 'X-SpaceLess-Staging-Token': gate, Authorization: 'Bearer ' + actor.token },
      handshakeTimeout: 15000, maxPayload: 64 * 1024,
    });
    const connection = { socket, events: [], closed: new Promise(resolve => socket.once('close', resolve)) };
    sockets.push(connection);
    socket.on('error', () => {});
    socket.on('message', (message, binary) => {
      if (binary) return;
      let event;
      try { event = JSON.parse(message.toString()); } catch { return; }
      if (streams.includes(event?.stream) || roomIds.includes(event?.data?.room_id)) connection.events.push(event);
    });
    await once(socket, 'open');
    return connection;
  }

  async function observed(connection, message, revision) {
    const deadline = Date.now() + 45000;
    while (Date.now() < deadline && !connection.events.some(event => event?.data?.message_id === message.message_id)) {
      assert.equal(connection.socket.readyState, WebSocket.OPEN);
      await delay(100);
    }
    const event = connection.events.find(candidate => candidate?.data?.message_id === message.message_id);
    assert.ok(event);
    assert.equal(event.type, 'MESSAGE_CREATE');
    assert.equal(event.stream, `room:${message.room_id}`);
    assert.equal(event.revision, revision);
    assert.ok(typeof event.event_id === 'string' && /^[1-9]\d*$/.test(event.event_id));
    assert.deepEqual(event.data, message);
    return event.event_id;
  }

  function checkMessage(message, room, actor, body, sequence) {
    assert.ok(typeof message?.message_id === 'string' && /^[1-9]\d*$/.test(message.message_id));
    assert.ok(BigInt(message.message_id) > BigInt(Number.MAX_SAFE_INTEGER));
    assert.equal(message.room_id, room);
    assert.equal(message.sender_id, actor.id);
    assert.equal(message.room_sequence, sequence);
    assert.equal(message.client_id, body.client_id);
    assert.equal(message.content, body.content);
  }

  const db = new Client(dbConfig);
  db.on('error', () => {});
  let seedAttempted = false;
  let passed = false;
  try {
    step = 'bootstrap_requires_session';
    await request('/bootstrap', 401, { code: 'UNAUTHORIZED' });
    step = 'connect_staging_database';
    await db.connect();
    step = 'seed_owned_fixture';
    seedAttempted = true;
    await transaction(db, async () => {
      for (const actor of actors) {
        await db.query(
          'INSERT INTO public.users (id, username, full_name, password_hash) VALUES ($1, $2, $3, $4)',
          [actor.id, actor.username, 'Cloudflare message staging smoke', passwordHash],
        );
        await db.query(
          'INSERT INTO public.user_details (user_id, date_of_birth, phone, email) VALUES ($1, $2, $3, $4)',
          [actor.id, '2000-01-01', '0900000000', actor.username + '@example.invalid'],
        );
      }
      await db.query('INSERT INTO public.channels (id, creator_id, name, invite_code) VALUES ($1, $2, $3, $3)',
        [channelId, sender.id, marker]);
      await db.query('INSERT INTO public.channel_members (channel_id, user_id, role_id) VALUES ($1,$2,1),($1,$3,3),($1,$4,3)',
        [channelId, ...actorIds]);
      await db.query("INSERT INTO public.rooms (id, channel_id, name, kind, private, position) VALUES ($1,$2,$3,'TEXT',true,0),($4,NULL,NULL,'DM',true,NULL)",
        [textRoom, channelId, marker, dmRoom]);
      await db.query('INSERT INTO public.room_members (room_id, user_id) VALUES ($1,$2)', [textRoom, reader.id]);
      await db.query('INSERT INTO public.dm_pairs (lo, hi, room_id) VALUES ($1,$2,$3)', [sender.id, reader.id, dmRoom]);
      await db.query("INSERT INTO public.relations (id, lo, hi, requester_id, status) VALUES ($1,$2,$3,$2,'FRIEND')",
        [id(6), sender.id, reader.id]);
    });

    step = 'login_and_websocket_upgrade';
    for (const actor of actors) {
      const login = await request('/auth/login', 200, { body: { username: actor.username, password } });
      assert.ok(typeof login?.access_token === 'string' && login.access_token.split('.').length === 3);
      actor.token = login.access_token;
    }
    const [senderSocket, readerSocket, outsideSocket] = await Promise.all(actors.map(openSocket));

    step = 'private_room_membership';
    const input = { client_id: marker + '_text', content: ' Xin chào\n' };
    const empty = await history(reader, textRoom);
    assert.deepEqual(empty.items, []);
    assert.equal(empty.last_message_sequence, '0');
    await request(roomPath(textRoom), 404, { actor: outsider, code: 'NOT_FOUND' });
    await send(outsider, textRoom, input, 404, 'NOT_FOUND');

    step = 'send_and_receive_first_message';
    const first = await send(sender, textRoom, input);
    checkMessage(first, textRoom, sender, input, '1');
    const [firstSenderEvent, firstReaderEvent] = await Promise.all([
      observed(senderSocket, first, '1'), observed(readerSocket, first, '1'),
    ]);
    assert.equal(firstSenderEvent, firstReaderEvent);

    step = 'same_client_id_replay_and_conflict';
    const replay = await send(sender, textRoom, input, 200);
    assert.deepEqual(replay, first);
    await send(sender, textRoom, { ...input, content: 'Nội dung đã đổi' }, 409, 'CLIENT_ID_REUSED');

    step = 'reply_and_paginated_room_history';
    const reply = { ...input, content: 'Đã nhận' };
    const second = await send(reader, textRoom, reply);
    checkMessage(second, textRoom, reader, reply, '2');
    assert.notEqual(first.message_id, second.message_id);
    const [secondSenderEvent, secondReaderEvent] = await Promise.all([
      observed(senderSocket, second, '2'), observed(readerSocket, second, '2'),
    ]);
    assert.equal(secondSenderEvent, secondReaderEvent);
    assert.ok(BigInt(secondSenderEvent) > BigInt(firstSenderEvent));
    const latest = await history(reader, textRoom, '?limit=1');
    assert.deepEqual(latest.items, [second]);
    assert.equal(latest.last_message_sequence, '2');
    assert.equal(latest.through_sequence, '2');
    assert.equal(latest.revision, '2');
    assert.equal(latest.next_cursor, '2');
    assert.equal(latest.has_more, true);
    const older = await history(reader, textRoom, '?before_sequence=2&limit=1');
    assert.deepEqual(older.items, [first]);
    assert.equal(older.has_more, false);
    const catchUp = await history(reader, textRoom, '?after_sequence=0&through_sequence=2');
    assert.deepEqual(catchUp.items, [first, second]);
    assert.equal(catchUp.next_cursor, '2');
    assert.equal(catchUp.has_more, false);

    step = 'bootstrap_snapshot_and_private_room_visibility';
    const readerSnapshot = await request('/bootstrap', 200, { actor: reader });
    const outsideSnapshot = await request('/bootstrap', 200, { actor: outsider });
    assert.equal(readerSnapshot.profile.user_id, reader.id);
    assert.equal(outsideSnapshot.profile.user_id, outsider.id);
    assert.deepEqual(readerSnapshot.rooms.map(room => room.room_id), [textRoom]);
    assert.equal(readerSnapshot.rooms[0].last_message_sequence, '2');
    assert.equal(readerSnapshot.revisions[`room:${textRoom}`], latest.revision);
    assert.deepEqual(readerSnapshot.dms.map(room => room.room_id), [dmRoom]);
    assert.deepEqual(outsideSnapshot.rooms, []);
    assert.deepEqual(outsideSnapshot.dms, []);
    assert.equal(outsideSnapshot.revisions[`room:${textRoom}`], undefined);

    step = 'blocked_dm_retains_history';
    const direct = await send(sender, dmRoom, input);
    checkMessage(direct, dmRoom, sender, input, '1');
    const [dmSenderEvent, dmReaderEvent] = await Promise.all([
      observed(senderSocket, direct, '1'), observed(readerSocket, direct, '1'),
    ]);
    assert.equal(dmSenderEvent, dmReaderEvent);
    await db.query('INSERT INTO public.user_blocks (blocker_id, blocked_id) VALUES ($1,$2)', [reader.id, sender.id]);
    const retained = await history(reader, dmRoom);
    assert.deepEqual(retained.items, [direct]);
    await send(sender, dmRoom, { ...input, client_id: marker + '_blocked' }, 403, 'FORBIDDEN');
    await request(roomPath(dmRoom), 404, { actor: outsider, code: 'NOT_FOUND' });

    step = 'websocket_isolation_and_replay';
    await delay(1000);
    for (const connection of sockets) assert.equal(connection.socket.readyState, WebSocket.OPEN);
    assert.deepEqual(outsideSocket.events, []);
    for (const connection of [senderSocket, readerSocket]) {
      const textEvents = connection.events.filter(event => event.stream === `room:${textRoom}`);
      assert.deepEqual(textEvents.map(event => event.event_id), [firstSenderEvent, secondSenderEvent]);
    }

    step = 'scheduled_outbox_retry';
    const requeued = await db.query(
      `UPDATE public.outbox_events SET dispatched_at = NULL
       WHERE id=$1 AND stream=$2 AND data @> '{"cloudflare_staging":true}'::jsonb
         AND COALESCE(data->'payload',data)->>'message_id' = $3 AND dispatched_at IS NOT NULL`,
      [firstSenderEvent, `room:${textRoom}`, first.message_id],
    );
    assert.equal(requeued.rowCount, 1);
    step = 'wait_for_scheduled_outbox_retry';
    const scheduledDeadline = Date.now() + 80000;
    const replayed = connection => connection.events.filter(event => event.event_id === firstSenderEvent);
    while (Date.now() < scheduledDeadline && replayed(readerSocket).length < 2) await delay(500);
    assert.equal(replayed(readerSocket).length, 2);
    assert.deepEqual(replayed(readerSocket)[0], replayed(readerSocket)[1]);
    assert.deepEqual(outsideSocket.events, []);
    passed = true;
  } finally {
    for (const { socket } of sockets) {
      if (socket.readyState === WebSocket.OPEN) socket.close();
      if (socket.readyState === WebSocket.CONNECTING) socket.terminate();
    }
    await Promise.all(sockets.map(async ({ socket, closed }) => {
      await Promise.race([closed, delay(2000, undefined, { ref: false })]);
      if (socket.readyState !== WebSocket.CLOSED) socket.terminate();
    }));
    await db.end().catch(() => {});
    if (seedAttempted) {
      const cleanup = new Client(dbConfig);
      cleanup.on('error', () => {});
      try {
        await cleanup.connect();
        await transaction(cleanup, async () => {
          const ownership = await cleanup.query(
            `SELECT id::text, username FROM public.users WHERE id = ANY($1::bigint[]) ORDER BY id FOR UPDATE`, [actorIds],
          );
          if (!ownership.rowCount) return;
          assert.deepEqual(ownership.rows, actors.map(({ id, username }) => ({ id, username })));
          const channel = await cleanup.query(
            'SELECT id FROM public.channels WHERE id=$1 AND creator_id=$2 AND invite_code=$3 FOR UPDATE',
            [channelId, sender.id, marker],
          );
          assert.equal(channel.rowCount, 1);
          const rooms = await cleanup.query(
            `SELECT r.id FROM public.rooms r WHERE (r.id=$1 AND r.channel_id=$2 AND r.name=$3 AND r.kind='TEXT')
             OR (r.id=$4 AND r.kind='DM' AND EXISTS
               (SELECT FROM public.dm_pairs p WHERE p.room_id=r.id AND p.lo=$5 AND p.hi=$6)) FOR UPDATE`,
            [textRoom, channelId, marker, dmRoom, sender.id, reader.id],
          );
          assert.equal(rooms.rowCount, 2);
          for (const table of ['outbox_events', 'stream_revisions']) {
            await cleanup.query(`DELETE FROM public.${table} WHERE stream = ANY($1::text[])`, [streams]);
          }
          for (const table of ['message_reads', 'message_send_receipts', 'messages', 'room_members', 'dm_pairs']) {
            await cleanup.query(`DELETE FROM public.${table} WHERE room_id = ANY($1::bigint[])`, [roomIds]);
          }
          await cleanup.query('DELETE FROM public.rooms WHERE id = ANY($1::bigint[])', [roomIds]);
          await cleanup.query('DELETE FROM public.channel_members WHERE channel_id=$1', [channelId]);
          await cleanup.query('DELETE FROM public.channels WHERE id=$1', [channelId]);
          await cleanup.query('DELETE FROM public.user_blocks WHERE blocker_id=$1 AND blocked_id=$2', [reader.id, sender.id]);
          await cleanup.query('DELETE FROM public.relations WHERE lo=$1 AND hi=$2', [sender.id, reader.id]);
          for (const table of ['revoked_tokens', 'auth_sessions', 'user_details']) {
            await cleanup.query(`DELETE FROM public.${table} WHERE user_id = ANY($1::bigint[])`, [actorIds]);
          }
          const deleted = await cleanup.query('DELETE FROM public.users WHERE id = ANY($1::bigint[])', [actorIds]);
          assert.equal(deleted.rowCount, actors.length);
        });
        console.log(JSON.stringify({ syntheticFixtureCleanup: 'passed' }));
      } catch (error) {
        console.error(JSON.stringify({ syntheticFixtureCleanup: 'failed', fixture: marker, ...failure(error) }));
        process.exitCode = 1;
      } finally {
        await cleanup.end().catch(() => {});
      }
    }
  }
  if (passed && !process.exitCode) {
    console.log(JSON.stringify({ worker: api.hostname, outcome: 'passed', checks: [
      'private_text_room_permissions', 'decimal_ids', 'sender_and_reader_websocket_delivery',
      'client_id_replay_and_conflict', 'cursor_history', 'blocked_dm_retains_history', 'websocket_isolation',
      'bootstrap_snapshot_and_private_room_visibility', 'scheduled_outbox_retry',
    ] }));
  }
}

try {
  await verify();
} catch (error) {
  console.error(JSON.stringify({ outcome: 'failed', step, ...failure(error) }));
  process.exitCode = 1;
}
