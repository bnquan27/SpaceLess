import assert from 'node:assert/strict';
import { argon2Sync, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { createRequire } from 'node:module';
import { isAbsolute } from 'node:path';

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error('Missing environment variable ' + name);
  return value;
}

const api = new URL(required('SPACELESS_STAGING_API_URL'));
assert.equal(api.protocol, 'https:');
assert.match(api.hostname, /^spaceless-staging-api\.[a-z0-9-]+\.workers\.dev$/);
assert.equal(api.pathname, '/');
assert.ok(!api.username && !api.password && !api.search && !api.hash);

const database = new URL(required('SPACELESS_STAGING_DATABASE_URL'));
assert.match(database.hostname, /\.pooler\.supabase\.com$/);
assert.equal(database.port, '5432');
const pgModule = required('SPACELESS_STAGING_PG_MODULE');
assert.ok(isAbsolute(pgModule), 'SPACELESS_STAGING_PG_MODULE must be absolute');
const require = createRequire(import.meta.url);
const { Client } = require(pgModule);
const wsModule = required('SPACELESS_STAGING_WS_MODULE');
assert.ok(isAbsolute(wsModule), 'SPACELESS_STAGING_WS_MODULE must be absolute');
const { WebSocket } = require(wsModule);
const owner = new Client({
  host: database.hostname,
  port: Number(database.port),
  user: decodeURIComponent(database.username),
  password: decodeURIComponent(database.password),
  database: decodeURIComponent(database.pathname.slice(1)),
  ssl: { ca: readFileSync(required('SPACELESS_STAGING_CA'), 'utf8'), rejectUnauthorized: true },
  connectionTimeoutMillis: 15000,
});

const gate = required('STAGING_GATE_TOKEN');
const jwtSecret = required('APP_JWT_SECRET');
assert.ok(gate.length >= 32);
const username = 'cfqa_' + randomBytes(6).toString('hex');
const userId = ((randomBytes(8).readBigUInt64BE() & ((1n << 61n) - 1n)) | (1n << 61n)).toString();
const password = randomBytes(24).toString('base64url');
const salt = randomBytes(16);
const digest = argon2Sync('argon2id', {
  message: Buffer.from(password), nonce: salt, parallelism: 1, tagLength: 32,
  memory: 19456, passes: 2,
});
const encode = bytes => bytes.toString('base64').replace(/=+$/, '');
const hash = '$argon2id$v=19$m=19456,t=2,p=1$' + encode(salt) + '$' + encode(digest);

async function request(path, { method = 'GET', token, body, gated = true } = {}) {
  const headers = new Headers();
  if (gated) headers.set('X-SpaceLess-Staging-Token', gate);
  if (token) headers.set('Authorization', 'Bearer ' + token);
  if (body) headers.set('Content-Type', 'application/json');
  const response = await fetch(new URL('/api/v1' + path, api), {
    method, headers, body: body ? JSON.stringify(body) : undefined,
    redirect: 'error', signal: AbortSignal.timeout(25000),
  });
  const payload = response.status === 204 ? null : await response.json().catch(() => null);
  return { status: response.status, payload };
}

function expectStatus(result, status, step) {
  assert.equal(result.status, status, step);
}

function expectError(result, status, code, step) {
  expectStatus(result, status, step);
  assert.equal(result.payload?.error?.code, code, step);
}

function issued(result, step) {
  expectStatus(result, 200, step);
  const { access_token: token, expires_at: expiresAt } = result.payload ?? {};
  assert.ok(typeof token === 'string' && token.split('.').length === 3, step);
  assert.ok(Date.parse(expiresAt) > Date.now(), step);
  const [header, payload, signature] = token.split('.');
  const expected = createHmac('sha256', jwtSecret).update(header + '.' + payload).digest();
  const actual = Buffer.from(signature, 'base64url');
  assert.ok(actual.length === expected.length && timingSafeEqual(actual, expected), step);
  const claims = JSON.parse(Buffer.from(payload, 'base64url'));
  assert.equal(JSON.parse(Buffer.from(header, 'base64url')).alg, 'HS256', step);
  assert.ok(claims.sub === userId && typeof claims.sid === 'string' && typeof claims.jti === 'string', step);
  assert.equal(claims.exp, Math.floor(Date.parse(expiresAt) / 1000), step);
  return token;
}

const login = (name = username, pass = password) =>
  request('/auth/login', { method: 'POST', body: { username: name, password: pass } });
const refresh = token => request('/auth/refresh', { method: 'POST', token });
const logout = token => request('/auth/logout', { method: 'POST', token });
const profile = token => request('/users/me', { token });

async function expectProfile(token, step) {
  const result = await profile(token);
  expectStatus(result, 200, step);
  assert.ok(typeof result.payload?.user_id === 'string' && result.payload.user_id === userId, step);
  assert.ok(result.payload?.username === username, step);
}

const sockets = [];
async function openSocket(token) {
  const endpoint = new URL('/api/v1/ws', api);
  endpoint.protocol = 'wss:';
  const socket = new WebSocket(endpoint, {
    headers: { 'X-SpaceLess-Staging-Token': gate, Authorization: 'Bearer ' + token },
    handshakeTimeout: 15000, maxPayload: 64 * 1024,
  });
  sockets.push(socket);
  const closed = new Promise(resolve => socket.once('close', resolve));
  await once(socket, 'open');
  return { socket, closed };
}
async function expectClosed(connection, label) {
  const closed = await Promise.race([connection.closed.then(() => true), delay(7500, false, { ref: false })]);
  assert.equal(closed, true, label);
}

let fixtureCommitted = false;
let step = 'connect_staging_database';
try {
  await owner.connect();
  step = 'seed_owned_fixture';
  await owner.query('BEGIN');
  await owner.query(
    'INSERT INTO public.users (id, username, full_name, password_hash) VALUES ($1, $2, $3, $4)',
    [userId, username, 'Cloudflare staging smoke', hash],
  );
  await owner.query(
    'INSERT INTO public.user_details (user_id, date_of_birth, phone, email) VALUES ($1, $2, $3, $4)',
    [userId, '2000-01-01', '0900000000', username + '@example.invalid'],
  );
  await owner.query('COMMIT');
  fixtureCommitted = true;

  step = 'stage_gate';
  const blocked = await request('/users/me', { gated: false });
  assert.ok([401, 403, 404].includes(blocked.status), step);
  step = 'missing_bearer';
  expectError(await profile(), 401, 'UNAUTHORIZED', step);
  step = 'invalid_credentials';
  const wrongPassword = await login(username, password + 'wrong');
  const unknownUser = await login(username + '_none', password);
  expectError(wrongPassword, 401, 'INVALID_CREDENTIALS', step);
  expectError(unknownUser, 401, 'INVALID_CREDENTIALS', step);
  assert.equal(wrongPassword.payload.error.message, unknownUser.payload.error.message, step);

  step = 'login_and_profile';
  const first = issued(await login(username.toUpperCase()), step);
  await expectProfile(first, step);
  const second = issued(await login(), step);

  step = 'websocket_upgrade';
  const firstSocket = await openSocket(first);
  const secondSocket = await openSocket(second);
  await delay(12000);

  step = 'concurrent_refresh';
  const attempts = await Promise.all([refresh(first), refresh(first)]);
  const success = attempts.filter(result => result.status === 200);
  assert.equal(success.length, 1, step);
  const rotated = issued(success[0], step);
  expectError(attempts.find(result => result.status !== 200), 401, 'UNAUTHORIZED', step);
  expectError(await profile(first), 401, 'UNAUTHORIZED', 'rotated JWT denied');
  await expectProfile(rotated, 'new JWT usable');

  step = 'refresh_closes_old_websocket';
  await expectClosed(firstSocket, step);
  assert.equal(secondSocket.socket.readyState, WebSocket.OPEN, 'refresh must preserve other sessions');
  const refreshedSocket = await openSocket(rotated);

  step = 'logout_and_independent_session';
  expectStatus(await logout(rotated), 204, step);
  expectError(await profile(rotated), 401, 'UNAUTHORIZED', step);
  await expectClosed(refreshedSocket, 'logout closes owning WebSocket');
  assert.equal(secondSocket.socket.readyState, WebSocket.OPEN, 'logout must preserve other sessions');
  await expectProfile(second, 'other session remains valid');

  step = 'account_locked';
  await owner.query('UPDATE public.users SET is_available = false WHERE id = $1', [userId]);
  expectError(await login(), 403, 'ACCOUNT_LOCKED', step);
  expectError(await profile(second), 401, 'UNAUTHORIZED', step);

  console.log(JSON.stringify({ worker: api.hostname, outcome: 'passed', checks: [
    'staging_gate', 'missing_bearer', 'invalid_credentials', 'login_and_decimal_profile',
    'concurrent_refresh', 'old_token_rejected', 'websocket_upgrade_and_refresh_close', 'logout_and_independent_session', 'account_locked',
  ] }));
} catch (error) {
  if (!fixtureCommitted) await owner.query('ROLLBACK').catch(() => {});
  console.error(JSON.stringify({
    outcome: 'failed', step, code: error.code ?? error.name,
    status: Number.isInteger(error.actual) ? error.actual : undefined,
    handshakeStatus: Number(error.message?.match(/^Unexpected server response: (\d+)$/)?.[1]) || undefined,
  }));
  process.exitCode = 1;
} finally {
  for (const socket of sockets) {
    socket.on('error', () => {});
    socket.terminate();
  }
  if (fixtureCommitted) {
    try {
      await owner.query('BEGIN');
      await owner.query('DELETE FROM public.revoked_tokens WHERE user_id = $1', [userId]);
      await owner.query('DELETE FROM public.auth_sessions WHERE user_id = $1', [userId]);
      await owner.query('DELETE FROM public.user_details WHERE user_id = $1', [userId]);
      const deletion = await owner.query('DELETE FROM public.users WHERE id = $1 AND username = $2', [userId, username]);
      assert.equal(deletion.rowCount, 1, 'expected cleanup of the owned synthetic account');
      await owner.query('COMMIT');
      console.log(JSON.stringify({ syntheticFixtureCleanup: 'passed' }));
    } catch (error) {
      await owner.query('ROLLBACK').catch(() => {});
      console.error(JSON.stringify({ syntheticFixtureCleanup: 'failed', code: error.code ?? error.name }));
      process.exitCode = 1;
    }
  }
  await owner.end().catch(() => {});
}
