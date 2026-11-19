import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';
import { accounts } from './accounts.mjs';
import { channels } from './channels.mjs';
import { messaging } from './messaging.mjs';
import { files, voice } from './media.mjs';
import { objectStore, liveKit } from './providers.mjs';

const [binary, psqlPath, evidenceDir] = process.argv.slice(2);
assert(binary && psqlPath && evidenceDir,
  'Run from the repository root: node server/test/api-integration.mjs <binary> <psql> <evidence directory>');
const database = new URL(process.env.DATABASE_URL);
assert(database.protocol === 'postgresql:' || database.protocol === 'postgres:');
assert.equal(database.hostname, '127.0.0.1', 'Use an isolated IPv4 loopback database');
assert.match(database.pathname, /^\/spaceless_test_[a-z0-9_]+$/, 'Use a disposable spaceless_test_* database');
const workDir = resolve(evidenceDir);
await mkdir(workDir, { recursive: true });

const exec = promisify(execFile);
async function psql(sql) {
  const args = ['--dbname', database.href, '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-c', sql];
  const { stdout } = await exec(psqlPath, args,
    { timeout: 10_000, env: { ...process.env, PGCONNECT_TIMEOUT: '5' },
      windowsHide: true, encoding: 'utf8' }).catch(() => {
    throw new Error('psql connection or command failed');
  });
  return stdout.trim();
}

const secret = () => randomBytes(32).toString('hex');
const startedAt = new Date().toISOString();
const cases = [];
let current;
let api;
let gateway;
let setupError;

async function run(name, check) {
  current = { name, status: 'passed', requests: [] };
  try {
    await check();
  } catch (error) {
    current.status = 'failed';
    current.error = String(error.message).slice(0, 1000);
    current.location = error.stack?.split('\n').find(line => /[\\/]api-suite[\\/]|[\\/]server[\\/]test[\\/]/.test(line))?.trim();
  }
  cases.push(current);
  console.log(`${current.status.toUpperCase()} ${name}${current.error ? ' | ' + current.error : ''}`);
}

function observe(name, value) {
  current.observations ??= {};
  current.observations[name] = value;
}

function loopbackUrl(value, origin) {
  const url = new URL(value);
  assert.equal(url.origin, origin, 'Fixture URL points to the expected loopback service');
  return url;
}

async function recordResponse(method, route, url, options) {
  const { token, body, raw, headers = {}, status = 200, code, asBytes = false } = options;
  const attempt = { route: `${method} ${route}`, expected: status };
  current.requests.push(attempt);
  const response = await fetch(url, {
    method, redirect: 'error', signal: AbortSignal.timeout(15_000),
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body !== undefined ? { 'content-type': 'application/json' } : {}),
      ...headers,
    },
    body: raw ?? (body === undefined ? undefined : JSON.stringify(body)),
  });
  attempt.observed = response.status;
  const bytes = Buffer.from(await response.arrayBuffer());
  const parsed = bytes.length && response.headers.get('content-type')?.includes('application/json')
    ? JSON.parse(bytes.toString()) : undefined;
  if (response.status >= 400 && parsed?.error?.code) attempt.code = parsed.error.code;
  assert.equal(response.status, status, `${attempt.route}${attempt.code ? ' ' + attempt.code : ''}`);
  if (code) assert.equal(attempt.code, code, `${attempt.route} error code`);
  if (status === 204) assert.equal(bytes.length, 0, `${attempt.route} empty response`);
  return { status: response.status, headers: response.headers, body: asBytes ? bytes : parsed };
}

async function request(method, template, options = {}) {
  const path = template.replaceAll(/\{(\w+)\}/g, (_, key) => {
    assert(options.params?.[key] !== undefined, `Missing ${key} for ${template}`);
    return encodeURIComponent(options.params[key]);
  });
  const url = new URL(api + path);
  for (const [key, value] of Object.entries(options.query ?? {})) url.searchParams.set(key, value);
  const route = path.endsWith('/members/me') ? template.replace('{user_id}', 'me') : template;
  return recordResponse(method, route, url, options);
}

async function download(url, options) {
  const target = loopbackUrl(url, gateway);
  assert.match(target.pathname, /^\/files\/\d+$/);
  return recordResponse('GET', '/files/{upload_id}', target, { ...options, asBytes: true });
}

async function account(overrides = {}) {
  const username = overrides.username ?? `qa${randomBytes(7).toString('hex')}`;
  const input = {
    username, email: `${username}@example.test`, password: 'local integration password',
    full_name: 'Test account', date_of_birth: '1990-01-01', phone: '+84900000000', ...overrides,
  };
  const { body: registration } = await request('POST', '/auth/register', { body: input, status: 201 });
  assert.match(registration.user_id, /^[1-9]\d*$/);
  assert(BigInt(registration.user_id) > BigInt(Number.MAX_SAFE_INTEGER), 'The Snowflake fixture exceeds JavaScript integer precision');
  const { body } = await request('POST', '/auth/login', {
    body: { username: input.username, password: input.password },
  });
  assert(typeof body.access_token === 'string' && body.access_token.length > 0, 'Login returns an access token');
  return { id: registration.user_id, username: input.username, email: input.email,
    password: input.password, registration, token: body.access_token };
}

async function connect(token) {
  const attempt = { route: 'GET /ws', expected: 101 };
  current.requests.push(attempt);
  const socket = new WebSocket(api.replace('http:', 'ws:') + '/ws', {
    headers: { authorization: `Bearer ${token}` },
  });
  const events = [];
  let closed;
  try {
    closed = new Promise(resolve => socket.addEventListener('close', resolve, { once: true }));
    socket.addEventListener('message', ({ data }) => events.push(JSON.parse(data)));
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('WebSocket upgrade timed out')), 5_000);
      socket.addEventListener('open', () => { clearTimeout(timer); resolve(); }, { once: true });
      socket.addEventListener('error', () => { clearTimeout(timer); reject(new Error('WebSocket upgrade failed')); }, { once: true });
    });
  } catch (error) {
    socket.close();
    throw error;
  }
  attempt.observed = 101;
  return {
    closed,
    close: () => socket.close(),
    async wait(type, predicate) {
      for (let count = 0; count < 100 && socket.readyState === WebSocket.OPEN; count += 1) {
        const index = events.findIndex(event => event.type === type && predicate(event.data));
        if (index !== -1) return events.splice(index, 1)[0];
        await delay(50);
      }
      throw new Error(`WebSocket did not deliver ${type}`);
    },
  };
}

async function freePort() {
  const listener = createServer().listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const { port } = listener.address();
  await new Promise(resolve => listener.close(resolve));
  return port;
}

const children = [];
function startServer(name, env) {
  const child = spawn(resolve(binary), [], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const log = [];
  child.stdout.on('data', chunk => log.push(chunk));
  child.stderr.on('data', chunk => log.push(chunk));
  const finished = new Promise(resolve => {
    child.once('error', error => { log.push(Buffer.from(error.message)); resolve(); });
    child.once('close', resolve);
  });
  children.push({ name, child, finished, log });
}

async function ready(url) {
  for (let count = 0; count < 80; count += 1) {
    const response = await fetch(url, { signal: AbortSignal.timeout(1_000) }).catch(() => undefined);
    if (response?.status === 401 && (await response.json()).error?.code === 'UNAUTHORIZED') return;
    await delay(125);
  }
  throw new Error(`Server did not return UNAUTHORIZED at ${new URL(url).pathname}`);
}

async function registeredRoutes() {
  const sourceDir = resolve('server/src');
  const routes = [];
  for (const name of (await readdir(sourceDir)).filter(name => name.endsWith('.rs'))) {
    const source = await readFile(join(sourceDir, name), 'utf8');
    const registrations = [...source.matchAll(/\.route\(\s*"([^"]+)",\s*((?:[a-z]+\(\w+\)\.?)+),?\s*\)/g)];
    assert.equal(registrations.length, [...source.matchAll(/\.route\(/g)].length,
      `Update the route inventory parser for ${name}`);
    for (const [, path, handlers] of registrations) {
      for (const [, method] of handlers.matchAll(/([a-z]+)\(\w+\)/g)) routes.push(`${method.toUpperCase()} ${path}`);
    }
  }
  assert(routes.includes('DELETE /channels/{channel_id}/members/{user_id}'));
  return routes.sort();
}

let store;
let provider;
try {
  assert.equal(await psql("SELECT to_regclass('public.users') IS NULL"), 't', 'Use an empty database');
  assert.equal(await psql("SELECT count(*) FROM pg_collation WHERE collname='und-x-icu'"), '1', 'ICU collation is required');
  store = await objectStore();
  provider = await liveKit(secret());
  const apiPort = await freePort();
  const gatewayPort = await freePort();
  const smtpPort = await freePort();
  api = `http://127.0.0.1:${apiPort}/api/v1`;
  gateway = `http://127.0.0.1:${gatewayPort}`;
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) =>
    !/^(AWS_|S3_|SMTP_|LIVEKIT_|FILE_GATEWAY_|SERVER_|SNOWFLAKE_|GOOGLE_OAUTH_)/.test(key.toUpperCase())));
  Object.assign(env, {
    DATABASE_URL: database.href, APP_JWT_SECRET: secret(), RUST_LOG: 'info', RUST_LOG_STYLE: 'never',
    SERVER_BIND: `127.0.0.1:${apiPort}`, FILE_GATEWAY_BIND: `127.0.0.1:${gatewayPort}`,
    FILE_GATEWAY_PUBLIC_URL: gateway, FILE_GATEWAY_SECRET: secret(), S3_ENDPOINT: store.origin,
    S3_BUCKET: 'spaceless-fixture', AWS_REGION: 'us-east-1', AWS_ACCESS_KEY_ID: 'local-fixture',
    AWS_SECRET_ACCESS_KEY: secret(), AWS_EC2_METADATA_DISABLED: 'true',
    SMTP_HOST: '127.0.0.1', SMTP_PORT: String(smtpPort), SMTP_FROM: 'recovery@example.test',
    PASSWORD_RESET_URL: 'https://example.test/reset',
    ...provider.env,
  });
  startServer('api', { ...env, SERVER_MODE: 'api' });
  await ready(api + '/users/me');
  startServer('gateway', { ...env, SERVER_MODE: 'gateway' });
  await ready(gateway + '/files/1');
  const context = { run, observe, request, account, psql, connect, download, store, provider, loopbackUrl };
  for (const suite of [accounts, channels, messaging, files, voice]) await suite(context);
} catch (error) {
  setupError = String(error.message);
  console.error(setupError);
} finally {
  for (const { name, child, finished, log } of children.reverse()) {
    child.kill();
    await finished;
    await writeFile(join(workDir, `${name}.log`), Buffer.concat(log));
  }
  await store?.close();
  await provider?.close();
}

const routes = await registeredRoutes();
const externalSuccessNotTested = {
  'POST /auth/google/exchange': 'Requires a live Google authorization code. Disabled-provider handling runs here. Signed-token and provisioning fixtures run in the Rust auth::google tests.',
};
const coverage = Object.fromEntries([...routes, 'DELETE /channels/{channel_id}/members/me'].sort().map(route => [
  route, {
    attempted: cases.filter(test => test.requests.some(request => request.route === route)).map(test => test.name),
    successful: cases.filter(test => test.status === 'passed' && test.requests.some(request =>
      request.route === route && request.observed >= 101 && request.observed < 300)).map(test => test.name),
    ...(externalSuccessNotTested[route] ? { externalSuccessNotTested: externalSuccessNotTested[route] } : {}),
  },
]));
const uncovered = Object.entries(coverage).filter(([, row]) => row.successful.length === 0 && !row.externalSuccessNotTested).map(([route]) => route);
const summary = { startedAt, finishedAt: new Date().toISOString(), node: process.version,
  registeredRoutes: routes.length, aliases: 1, passed: cases.filter(test => test.status === 'passed').length,
  failed: cases.filter(test => test.status === 'failed').length, requests: cases.reduce((count, test) => count + test.requests.length, 0),
  covered: Object.values(coverage).filter(row => row.successful.length > 0).length,
  externalSuccessNotTested, uncovered, setupError };
console.log(JSON.stringify(summary));
await writeFile(join(workDir, 'result.json'), JSON.stringify({ summary, coverage, cases }, null, 2) + '\n');
if (setupError || summary.failed || uncovered.length) process.exitCode = 1;
