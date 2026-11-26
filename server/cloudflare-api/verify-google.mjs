import assert from 'node:assert/strict';
import { createPrivateKey, randomBytes, sign } from 'node:crypto';
import { readFileSync } from 'node:fs';

const TOKEN_URL = 'https://oauth2.googleapis.com/token';
const JWKS_URL = 'https://www.googleapis.com/oauth2/v3/certs';
const CLIENT_ID = 'fixture.apps.googleusercontent.com';
const CLIENT_SECRET = 'google-test-client-secret';
const VERIFIER = 'abcd-_.~'.repeat(8);
const NONCE = 'nonce_0123456789'.repeat(3);
const PORT = 49152;
const key = createPrivateKey(readFileSync(new URL('../test/fixtures/google-private-key.pem', import.meta.url)));
const jwks = JSON.parse(readFileSync(new URL('../test/fixtures/google-jwks.json', import.meta.url), 'utf8'));
const encode = value => Buffer.from(JSON.stringify(value)).toString('base64url');

export function googleFixture(ResponseType = Response) {
  const codes = new Map();
  let exchanges = 0;
  return {
    bindings: {
      GOOGLE_OAUTH_CLIENT_ID: CLIENT_ID,
      GOOGLE_OAUTH_CLIENT_SECRET: CLIENT_SECRET,
    },
    get exchanges() {
      return exchanges;
    },
    handles(request) {
      return request.url === TOKEN_URL || request.url === JWKS_URL;
    },
    issue(claims, { corruptSignature = false, redirect = false } = {}) {
      const code = 'fixture+/=' + randomBytes(12).toString('hex');
      codes.set(code, { claims, corruptSignature, redirect });
      return { code, code_verifier: VERIFIER, nonce: NONCE, loopback_port: PORT };
    },
    async fetch(request) {
      if (request.url === JWKS_URL) {
        assert.equal(request.method, 'GET');
        return ResponseType.json(jwks, { headers: { 'Cache-Control': 'public, max-age=60' } });
      }
      assert.equal(request.url, TOKEN_URL);
      assert.equal(request.method, 'POST');
      assert.equal(request.headers.get('content-type'), 'application/x-www-form-urlencoded');
      exchanges += 1;
      const form = new URLSearchParams(await request.text());
      assert.equal(form.get('client_id'), CLIENT_ID);
      assert.equal(form.get('client_secret'), CLIENT_SECRET);
      assert.equal(form.get('redirect_uri'), `http://127.0.0.1:${PORT}`);
      assert.equal(form.get('grant_type'), 'authorization_code');
      const code = form.get('code');
      const entry = codes.get(code);
      if (!entry || form.get('code_verifier') !== VERIFIER) {
        return ResponseType.json({ error: 'invalid_grant' }, { status: 400 });
      }
      codes.delete(code);
      if (entry.redirect) return ResponseType.redirect('https://unexpected.example.test', 307);
      const now = Math.floor(Date.now() / 1000);
      const header = { alg: 'RS256', kid: 'google-test-key' };
      const payload = {
        iss: 'https://accounts.google.com',
        aud: CLIENT_ID,
        nonce: NONCE,
        exp: now + 300,
        email_verified: true,
        name: 'Người dùng thử',
        ...entry.claims,
      };
      const signed = `${encode(header)}.${encode(payload)}`;
      const signature = sign('RSA-SHA256', Buffer.from(signed), key);
      if (entry.corruptSignature) signature[0] ^= 1;
      return ResponseType.json({ id_token: `${signed}.${signature.toString('base64url')}` });
    },
  };
}

export async function verifyGoogle({ fixture, fetch, gate, query }) {
  const random = randomBytes(8).toString('hex');
  const address = `google-${random}@example.test`;
  const subject = `google-fixture-${random}`;
  const claims = { sub: subject, email: address };
  const exchange = '/api/v1/auth/google/exchange';

  async function request(path, { status = 200, code, body, token } = {}) {
    const headers = { 'X-SpaceLess-Staging-Token': gate };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (token) headers.Authorization = `Bearer ${token}`;
    const response = await fetch(path, {
      method: body === undefined ? 'GET' : 'POST',
      body: body === undefined ? undefined : JSON.stringify(body),
      headers,
    });
    const data = await response.json();
    assert.equal(response.status, status, `${path} returned ${data.error?.code ?? 'success'}`);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    if (code) assert.equal(data.error?.code, code);
    return data;
  }

  async function rejectsGoogle(body) {
    await request(exchange, { body, status: 401, code: 'INVALID_GOOGLE_LOGIN' });
  }

  const before = fixture.exchanges;
  await request(exchange, {
    body: { ...fixture.issue(claims), code_verifier: 'short' },
    status: 400, code: 'INVALID_GOOGLE_EXCHANGE',
  });
  assert.equal(fixture.exchanges, before);
  await rejectsGoogle({ ...fixture.issue(claims), code_verifier: 'x'.repeat(43) });
  await rejectsGoogle(fixture.issue(claims, { corruptSignature: true }));
  await request(exchange, {
    body: fixture.issue(claims, { redirect: true }), status: 503, code: 'UNAVAILABLE',
  });
  for (const change of [
    { iss: 'https://example.test' },
    { aud: 'another.apps.googleusercontent.com' },
    { exp: 1 },
    { nonce: 'different_nonce' },
    { email_verified: false },
  ]) {
    await rejectsGoogle(fixture.issue({ ...claims, ...change }));
  }

  const firstCode = fixture.issue(claims);
  const [first, second] = await Promise.all([
    request(exchange, { body: firstCode }),
    request(exchange, { body: fixture.issue(claims) }),
  ]);
  assert.notEqual(first.access_token, second.access_token);
  assert.ok(Date.parse(first.expires_at) > Date.now());
  const [profile, same] = await Promise.all([
    request('/api/v1/users/me', { token: first.access_token }),
    request('/api/v1/users/me', { token: second.access_token }),
  ]);
  assert.equal(typeof profile.user_id, 'string');
  assert.match(profile.user_id, /^[1-9][0-9]+$/);
  assert.ok(BigInt(profile.user_id) > BigInt(Number.MAX_SAFE_INTEGER));
  assert.equal(profile.user_id, same.user_id);
  assert.equal(profile.email, address);
  assert.equal(profile.full_name, 'Người dùng thử');
  assert.equal(profile.phone, null);
  assert.equal(profile.date_of_birth, null);
  await rejectsGoogle(firstCode);

  await request(exchange, {
    body: fixture.issue({ sub: subject + '-other', email: address }),
    status: 409, code: 'GOOGLE_EMAIL_CONFLICT',
  });
  await request('/api/v1/auth/login', {
    body: { username: profile.username, password: 'not-a-local-password' },
    status: 401, code: 'INVALID_CREDENTIALS',
  });

  const updatedEmail = `changed-${address}`;
  await query('UPDATE user_details SET email = $1 WHERE user_id = $2', [updatedEmail, profile.user_id]);
  await query("UPDATE users SET full_name = 'Hồ sơ đã sửa' WHERE id = $1", [profile.user_id]);
  const again = await request(exchange, {
    body: fixture.issue({ ...claims, email: `provider-${address}` }),
  });
  const edited = await request('/api/v1/users/me', { token: again.access_token });
  assert.equal(edited.user_id, profile.user_id);
  assert.equal(edited.email, updatedEmail);
  assert.equal(edited.full_name, 'Hồ sơ đã sửa');

  await query('UPDATE users SET is_available = false WHERE id = $1', [profile.user_id]);
  await request(exchange, { body: fixture.issue(claims), status: 403, code: 'ACCOUNT_LOCKED' });
  await request('/api/v1/users/me', { token: first.access_token, status: 401, code: 'UNAUTHORIZED' });
}
