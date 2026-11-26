import assert from 'node:assert/strict';
import { argon2Sync, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const require = createRequire(import.meta.url);
const contents = Buffer.from('SpaceLessApp staging R2 file fixture only.\n');
const fileName = 'cf-r2-smoke.txt';
const mime = 'text/plain';

function required(name) {
  const value = process.env[name];
  if (!value) throw Object.assign(new Error('Missing configuration'), { setting: name });
  return value;
}

function failure(error) {
  return {
    code: /^[A-Z_a-z0-9]{2,40}$/.test(error.code ?? '') ? error.code : error.name,
    setting: error.setting,
    status: Number.isInteger(error.actual) ? error.actual : undefined,
    r2Code: error.r2Code,
  };
}

function stagingConfig() {
  const api = new URL(required('SPACELESS_STAGING_API_URL'));
  assert.ok(/^https:\/\/spaceless-staging-api\.[a-z0-9-]+\.workers\.dev\/$/.test(api.href));
  const database = new URL(required('SPACELESS_STAGING_DATABASE_URL'));
  assert.ok(['postgres:', 'postgresql:'].includes(database.protocol));
  assert.ok(database.hostname.endsWith('.pooler.supabase.com') && database.port === '5432');
  assert.match(database.username, /^postgres\.[a-z]{20}$/);
  const pg = required('SPACELESS_STAGING_PG_MODULE');
  assert.ok(isAbsolute(pg));
  const gate = required('STAGING_GATE_TOKEN');
  assert.ok(gate.length >= 32);
  const { Client } = require(pg);
  const connect = () => new Client({
    host: database.hostname, port: Number(database.port),
    user: decodeURIComponent(database.username), password: decodeURIComponent(database.password),
    database: decodeURIComponent(database.pathname.slice(1)),
    ssl: { ca: readFileSync(required('SPACELESS_STAGING_CA'), 'utf8'), rejectUnauthorized: true },
    connectionTimeoutMillis: 12000, statement_timeout: 15000, query_timeout: 18000,
  });
  return { api, gate, connect };
}

function fixture() {
  const marker = `cfr2_${randomBytes(6).toString('hex')}`;
  const base = (randomBytes(8).readBigUInt64BE() & ((1n << 60n) - 1n)) | (1n << 61n);
  const id = offset => String(base + BigInt(offset));
  const users = ['owner', 'reader', 'outsider'].map((kind, offset) => ({
    id: id(offset), username: `${marker}_${kind}`,
  }));
  return { marker, users, channel: id(3), room: id(4) };
}

async function transaction(db, run) {
  await db.query('BEGIN');
  try {
    const result = await run();
    await db.query('COMMIT');
    return result;
  } catch (error) {
    await db.query('ROLLBACK').catch(() => {});
    throw error;
  }
}

function passwordHash(password) {
  const salt = randomBytes(16);
  const bytes = argon2Sync('argon2id', {
    message: Buffer.from(password), nonce: salt, memory: 19456, passes: 2, parallelism: 1, tagLength: 32,
  });
  const b64 = value => value.toString('base64').replace(/=+$/, '');
  return `$argon2id$v=19$m=19456,t=2,p=1$${b64(salt)}$${b64(bytes)}`;
}

async function seed(db, { marker, users, channel, room }, password) {
  const hash = passwordHash(password);
  const [owner, reader, outsider] = users;
  await transaction(db, async () => {
    for (const user of users) {
      await db.query('INSERT INTO public.users (id,username,full_name,password_hash) VALUES($1,$2,$3,$4)',
        [user.id, user.username, 'SpaceLessApp staging R2 smoke', hash]);
      await db.query("INSERT INTO public.user_details(user_id,date_of_birth,phone,email) VALUES($1,'2000-01-01','0900000000',$2)",
        [user.id, `${user.username}@example.invalid`]);
    }
    await db.query('INSERT INTO public.channels(id,creator_id,name,invite_code) VALUES($1,$2,$3::text,$3::text)',
      [channel, owner.id, marker]);
    await db.query('INSERT INTO public.channel_members(channel_id,user_id,role_id) VALUES($1,$2,1),($1,$3,3),($1,$4,3)',
      [channel, owner.id, reader.id, outsider.id]);
    await db.query("INSERT INTO public.rooms(id,channel_id,name,kind,private,position) VALUES($1,$2,$3,'TEXT',true,0)",
      [room, channel, marker]);
    await db.query('INSERT INTO public.room_members(room_id,user_id) VALUES($1,$2)', [room, reader.id]);
  });
}

function requestApi(api, gate) {
  return async (path, status, { token, body, method = 'GET', range, responseType = 'json' } = {}) => {
    const target = new URL(path, api);
    assert.equal(target.origin, api.origin);
    const headers = { 'X-SpaceLess-Staging-Token': gate };
    if (token) headers.Authorization = `Bearer ${token}`;
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (range) headers.Range = range;
    const response = await fetch(target, {
      method, headers, body: body === undefined ? undefined : JSON.stringify(body), redirect: 'error',
      signal: AbortSignal.timeout(20000),
    });
    assert.equal(response.status, status);
    if (responseType === 'bytes') {
      return { bytes: Buffer.from(await response.arrayBuffer()), headers: response.headers };
    }
    return response.status === 204 ? null : response.json();
  };
}

async function put(url, requiredHeaders, status) {
  const response = await fetch(url, {
    method: 'PUT', headers: requiredHeaders, body: contents, redirect: 'error', signal: AbortSignal.timeout(20000),
  });
  const body = await response.text();
  if (response.status !== status) {
    const r2Code = body.match(/<Code>([A-Za-z0-9]{1,40})<\/Code>/)?.[1];
    throw Object.assign(new Error('Unexpected conditional R2 PUT response'), { actual: response.status, r2Code });
  }
}

function downloadUrl(api, value, id) {
  const url = new URL(value);
  assert.equal(url.origin, api.origin);
  assert.equal(url.pathname, `/files/${id}`);
  assert.ok(url.searchParams.has('token'));
  return url;
}

let step = 'staging_configuration';
async function exercise(config, inputs, password) {
  const { api, gate } = config;
  const { users, marker, channel, room } = inputs;
  const request = requestApi(api, gate);
  step = 'login';
  for (const user of users) {
    const { access_token } = await request('/api/v1/auth/login', 200, {
      method: 'POST', body: { username: user.username, password },
    });
    assert.ok(typeof access_token === 'string' && access_token.split('.').length === 3);
    user.token = access_token;
  }
  const [owner, reader, outsider] = users;
  const roomPath = `/api/v1/rooms/${room}`;
  const uploadPath = `${roomPath}/uploads/presign`;
  const metadata = { file_name: fileName, mime_type: mime, file_size_bytes: contents.length };
  step = 'private_room_upload_denial';
  await request(uploadPath, 404, { method: 'POST', token: outsider.token, body: metadata });

  step = 'presign';
  const signed = await request(uploadPath, 200, { method: 'POST', token: owner.token, body: metadata });
  assert.match(signed.upload_id, /^[1-9]\d*$/);
  assert.equal(signed.required_headers['if-none-match'], '*');
  assert.equal(signed.required_headers['content-length'], String(contents.length));
  const url = new URL(signed.put_url);
  assert.ok(/^https:\/\/[0-9a-f]{32}\.r2\.cloudflarestorage\.com$/.test(url.origin));
  assert.match(url.pathname, /^\/spaceless-staging-files\/uploads\/worker\/staging\//);
  assert.ok(url.searchParams.has('X-Amz-Signature'));
  step = 'signed_put';
  await put(url, signed.required_headers, 200);
  step = 'r2_overwrite_denial';
  await delay(1200);
  await put(url, signed.required_headers, 412);
  step = 'anonymous_r2_object_read_denial';
  const anonymous = await fetch(new URL(url.pathname, url.origin), {
    method: 'GET', redirect: 'error', signal: AbortSignal.timeout(20000),
  });
  assert.ok([400, 401, 403, 404].includes(anonymous.status), `Unexpected unsigned R2 status ${anonymous.status}`);
  await anonymous.body?.cancel();

  step = 'attach_and_retry';
  const messageBody = { client_id: marker, content: '', upload_ids: [signed.upload_id] };
  const send = status => request(`${roomPath}/messages`, status, { method: 'POST', token: owner.token, body: messageBody });
  const message = await send(201);
  assert.equal((await send(200)).message_id, message.message_id);
  assert.equal(message.attachments.length, 1);
  const [attachment] = message.attachments;
  assert.equal(attachment.file_name, fileName);
  assert.equal(attachment.file_size_bytes, contents.length);
  const attachmentPath = `/api/v1/attachments/${attachment.attachment_id}/url`;

  step = 'authorized_file_read';
  const grant = await request(attachmentPath, 200, { token: reader.token });
  const download = downloadUrl(api, grant.url, signed.upload_id);
  const full = await request(download, 200, { token: reader.token, responseType: 'bytes' });
  assert.deepEqual(full.bytes, contents);
  assert.equal(full.headers.get('content-type'), mime);
  const part = await request(download, 206, { token: reader.token, range: 'bytes=0-3', responseType: 'bytes' });
  assert.deepEqual(part.bytes, contents.subarray(0, 4));
  assert.equal(part.headers.get('content-range'), `bytes 0-3/${contents.length}`);

  step = 'download_token_and_access_revocation';
  await request(attachmentPath, 404, { token: outsider.token });
  await request(download, 401, { token: outsider.token });
  await request(`/api/v1/channels/${channel}/members/${reader.id}`, 204, { method: 'DELETE', token: owner.token });
  await request(download, 404, { token: reader.token });
  console.log(JSON.stringify({ outcome: 'r2_flow_passed', checks: [
    'signed_put', 'overwrite_denied', 'direct_object_private', 'attachment_replay', 'authorized_range', 'access_revocation',
  ] }));
}

async function removeFixture(db, { marker, users, channel, room }) {
  const ids = users.map(user => user.id);
  await transaction(db, async () => {
    const owned = await db.query('SELECT id::text,username FROM public.users WHERE id=ANY($1::bigint[]) ORDER BY id FOR UPDATE', [ids]);
    assert.deepEqual(owned.rows, users.map(({ id, username }) => ({ id, username })));
    const ownedRoom = await db.query(
      `SELECT r.id FROM public.rooms r JOIN public.channels c ON c.id=r.channel_id
       WHERE r.id=$1 AND r.name=$2::text AND r.kind='TEXT' AND r.private AND c.id=$3 AND c.creator_id=$4 AND c.invite_code=$2::text FOR UPDATE OF r,c`,
      [room, marker, channel, ids[0]],
    );
    assert.equal(ownedRoom.rowCount, 1);
    await db.query('DELETE FROM public.message_send_receipts WHERE room_id=$1 AND sender_id=$2 AND client_id=$3', [room, ids[0], marker]);
    await db.query('DELETE FROM public.messages WHERE room_id=$1 AND sender_id=$2 AND client_id=$3', [room, ids[0], marker]);
    await db.query(
      `UPDATE public.file_uploads SET status='DELETE_PENDING'
       WHERE room_id=$1 AND owner_id=$2 AND purpose='ATTACHMENT' AND file_name=$3 AND storage_key LIKE 'uploads/worker/final/%'`,
      [room, ids[0], fileName],
    );
  });

  const deadline = Date.now() + 400000;
  for (;;) {
    const remaining = await db.query('SELECT id FROM public.file_uploads WHERE room_id=$1', [room]);
    if (!remaining.rowCount) break;
    assert.ok(Date.now() < deadline, 'Fixture upload still awaits Worker Cron cleanup');
    await delay(6000);
  }
  await transaction(db, async () => {
    const streams = [`channel:${channel}`, `room:${room}`, ...ids.map(id => `user:${id}`)];
    await db.query('DELETE FROM public.outbox_events WHERE stream=ANY($1::text[])', [streams]);
    await db.query('DELETE FROM public.stream_revisions WHERE stream=ANY($1::text[])', [streams]);
    await db.query('DELETE FROM public.message_reads WHERE room_id=$1 AND user_id=ANY($2::bigint[])', [room, ids]);
    await db.query('DELETE FROM public.room_members WHERE room_id=$1 AND user_id=ANY($2::bigint[])', [room, ids]);
    const roomResult = await db.query('DELETE FROM public.rooms WHERE id=$1 AND channel_id=$2 AND name=$3', [room, channel, marker]);
    assert.equal(roomResult.rowCount, 1);
    await db.query('DELETE FROM public.channel_members WHERE channel_id=$1 AND user_id=ANY($2::bigint[])', [channel, ids]);
    const channelResult = await db.query('DELETE FROM public.channels WHERE id=$1 AND creator_id=$2 AND invite_code=$3', [channel, ids[0], marker]);
    assert.equal(channelResult.rowCount, 1);
    for (const table of ['revoked_tokens', 'auth_sessions', 'user_details']) {
      await db.query(`DELETE FROM public.${table} WHERE user_id=ANY($1::bigint[])`, [ids]);
    }
    const userResult = await db.query('DELETE FROM public.users WHERE id=ANY($1::bigint[]) AND username=ANY($2::text[])',
      [ids, users.map(user => user.username)]);
    assert.equal(userResult.rowCount, users.length);
  });
}

async function resumeCleanup(config, marker) {
  assert.match(marker, /^cfr2_[0-9a-f]{12}$/);
  const names = ['owner', 'reader', 'outsider'].map(role => `${marker}_${role}`);
  const db = config.connect();
  db.on('error', () => {});
  try {
    await db.connect();
    const { rows: users } = await db.query(
      'SELECT id::text,username FROM public.users WHERE username=ANY($1::text[]) ORDER BY id', [names],
    );
    if (users.length === 0) {
      console.log(JSON.stringify({ syntheticR2FixtureCleanup: 'already_absent' }));
      return;
    }
    assert.deepEqual(users.map(user => user.username), names);
    const { rows } = await db.query(
      `SELECT c.id::text AS channel,r.id::text AS room FROM public.channels c
       JOIN public.rooms r ON r.channel_id=c.id WHERE c.creator_id=$1 AND c.invite_code=$2::text
         AND r.name=$2::text AND r.kind='TEXT' AND r.private`,
      [users[0].id, marker],
    );
    assert.equal(rows.length, 1);
    await removeFixture(db, { marker, users, ...rows[0] });
    console.log(JSON.stringify({ syntheticR2FixtureCleanup: 'passed' }));
  } finally {
    await db.end().catch(() => {});
  }
}

async function runFixture(config) {
  const inputs = fixture();
  const password = randomBytes(24).toString('base64url');
  let seeded = false;
  const db = config.connect();
  db.on('error', () => {});
  try {
    step = 'seed_owned_fixture';
    await db.connect();
    await seed(db, inputs, password);
    seeded = true;
    await exercise(config, inputs, password);
  } catch (error) {
    console.error(JSON.stringify({ outcome: 'flow_failed', step, ...failure(error) }));
    process.exitCode = 1;
  } finally {
    await db.end().catch(() => {});
    if (seeded) {
      const cleanup = config.connect();
      cleanup.on('error', () => {});
      try {
        await cleanup.connect();
        await removeFixture(cleanup, inputs);
        console.log(JSON.stringify({ syntheticR2FixtureCleanup: 'passed' }));
      } catch (error) {
        console.error(JSON.stringify({ syntheticR2FixtureCleanup: 'pending', fixture: inputs.marker, ...failure(error) }));
        process.exitCode = 1;
      } finally {
        await cleanup.end().catch(() => {});
      }
    }
  }
}

try {
  const config = stagingConfig();
  if (process.argv[2] === '--cleanup-fixture') {
    assert.equal(process.argv.length, 4);
    step = 'resume_owned_fixture_cleanup';
    await resumeCleanup(config, process.argv[3]);
  } else {
    await runFixture(config);
  }
} catch (error) {
  console.error(JSON.stringify({ outcome: 'failed', step, ...failure(error) }));
  process.exitCode = 1;
}
