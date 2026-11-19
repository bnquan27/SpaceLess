import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';

export async function accounts({ run, observe, request, account, psql }) {
    const login = ({ username, password }) => request('POST', '/auth/login', { body: { username, password } });
    const requestFriend = async (sender, recipient) => (await request('POST', '/friends/requests', {
        token: sender.token, body: { target_user_id: recipient.id }, status: 201,
    })).body;
    const resolveFriend = async (recipient, friendRequest, decision) => (await request('PATCH', '/friends/requests/{request_id}', {
        token: recipient.token, params: { request_id: friendRequest.request_id }, body: { decision },
    })).body;

    async function resetFixture(userId, minutes = 15) {
        assert.match(userId, /^[1-9]\d*$/);
        const bytes = randomBytes(32);
        const hash = createHash('sha256').update(bytes).digest('hex');
        await psql(`INSERT INTO password_reset_tokens (token_hash, user_id, expires_at)
            VALUES (decode('${hash}', 'hex'), ${userId}, now() + (${minutes} * interval '1 minute'))`);
        return bytes.toString('hex');
    }

    await run('Google exchange reports a disabled provider and rejects malformed input', async () => {
        await request('POST', '/auth/google/exchange', {
            body: { code: 'unused-fixture-code', code_verifier: 'a'.repeat(43), nonce: 'b'.repeat(43), loopback_port: 49152 },
            status: 503, code: 'GOOGLE_NOT_CONFIGURED',
        });
        await request('POST', '/auth/google/exchange', {
            body: { code: 'unused', code_verifier: 'short', nonce: 'b'.repeat(43), loopback_port: 49152 },
            status: 400, code: 'INVALID_GOOGLE_EXCHANGE',
        });
        await request('POST', '/auth/google/exchange', { body: { id_token: 'untrusted' }, status: 400, code: 'INVALID_BODY' });
    });

    await run('Registration returns a profile and username sign-in grants access to it', async () => {
        const owner = await account({ full_name: 'API Profile', date_of_birth: '1992-06-03', phone: '+84901234567' });
        assert.match(owner.id, /^[1-9]\d*$/);
        const profile = {
            user_id: owner.id, username: owner.username, email: owner.email,
            full_name: 'API Profile', date_of_birth: '1992-06-03', phone: '+84901234567',
            address: null, avatar_url: null,
        };
        assert.deepEqual(owner.registration, profile);
        assert.deepEqual((await request('GET', '/users/me', { token: owner.token })).body, profile);
        await request('GET', '/users/me', { status: 401, code: 'UNAUTHORIZED' });
        const wrongPassword = await request('POST', '/auth/login', {
            body: { username: owner.username, password: 'a different fixture password' },
            status: 401, code: 'INVALID_CREDENTIALS',
        });
        const emailLogin = await request('POST', '/auth/login', {
            body: { username: owner.email, password: owner.password }, status: 401, code: 'INVALID_CREDENTIALS',
        });
        assert.deepEqual(emailLogin.body.error, wrongPassword.body.error);
    });

    await run('Registration validates required fields and rejects duplicate identities', async () => {
        const existing = await account();
        const username = `account_${randomBytes(8).toString('hex')}`;
        const required = {
            username, password: existing.password, email: `${username}@example.test`,
            full_name: 'Registration fixture', date_of_birth: '1990-01-01',
        };
        const phone = '+84901234567';
        await request('POST', '/auth/register', { body: required, status: 400, code: 'INVALID_BODY' });
        for (const invalid of [{ phone: 'call123' }, { date_of_birth: '2999-01-01' }, { password: 'short' }]) {
            await request('POST', '/auth/register', { body: { ...required, phone, ...invalid }, status: 400 });
        }
        for (const duplicate of [{ username: existing.username }, { email: existing.email }]) {
            await request('POST', '/auth/register', { body: { ...required, phone, ...duplicate }, status: 409 });
        }
        const created = await request('POST', '/auth/register', { body: { ...required, phone }, status: 201 });
        assert.equal(created.body.username, username);
        assert.equal(created.body.phone, phone);
    });

    await run('Profile updates persist, reject duplicate email, and retire recovery for the old address', async () => {
        const owner = await account();
        const other = await account();
        const token = owner.token;
        const priorReset = await resetFixture(owner.id);
        const changes = {
            full_name: 'Updated API Profile', date_of_birth: '1994-07-15',
            phone: '0901234567', email: `${owner.username}.updated@example.test`, address: 'Test address',
        };
        const updated = await request('PATCH', '/users/me', { token, body: changes });
        const expected = { ...owner.registration, ...changes };
        assert.deepEqual(updated.body, expected);
        await request('PATCH', '/users/me', {
            token, body: { full_name: 'Rejected change', email: other.email }, status: 409,
        });
        await request('PATCH', '/users/me', {
            token, body: { username: 'cannot-change' }, status: 400, code: 'INVALID_BODY',
        });
        assert.deepEqual((await request('GET', '/users/me', { token })).body, expected);
        const cleared = await request('PATCH', '/users/me', { token, body: { address: null } });
        assert.deepEqual(cleared.body, { ...expected, address: null });
        await request('POST', '/auth/reset-password', {
            body: { token: priorReset, new_password: 'replacement test password' },
            status: 400, code: 'INVALID_RESET_TOKEN',
        });
    });

    await run('Refresh replaces the current token and logout ends only its sign-in', async () => {
        const owner = await account();
        const { body: separate } = await login(owner);
        const { body: refreshed } = await request('POST', '/auth/refresh', { token: owner.token });
        assert(typeof refreshed.access_token === 'string' && refreshed.access_token !== owner.token, 'Refresh returns a distinct token');
        assert(Date.parse(refreshed.expires_at) > Date.now(), 'Refresh returns a future expiration');
        const claims = token => JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString());
        const previous = claims(owner.token);
        const next = claims(refreshed.access_token);
        assert(typeof previous.jti === 'string' && typeof next.jti === 'string');
        assert(typeof previous.sid === 'string' && typeof next.sid === 'string');
        observe('jwtRefresh', { jtiChanged: previous.jti !== next.jti, sidPreserved: previous.sid === next.sid });
        assert(previous.jti !== next.jti, 'Refresh rotates the token ID');
        assert(previous.sid === next.sid, 'Refresh retains the sign-in session ID');
        await request('GET', '/users/me', { token: owner.token, status: 401, code: 'UNAUTHORIZED' });
        assert.equal((await request('GET', '/users/me', { token: refreshed.access_token })).body.user_id, owner.id);
        await request('POST', '/auth/logout', { token: refreshed.access_token, status: 204 });
        await request('GET', '/users/me', { token: refreshed.access_token, status: 401, code: 'UNAUTHORIZED' });
        assert.equal((await request('GET', '/users/me', { token: separate.access_token })).body.user_id, owner.id);
    });

    await run('Recovery returns the same message for known and unknown addresses and enforces its quota', async () => {
        const owner = await account();
        const body = { email: `missing_${randomBytes(8).toString('hex')}@example.test` };
        const known = await request('POST', '/auth/forgot-password', { body: { email: owner.email } });
        const neutral = { message: 'Đã tiếp nhận yêu cầu, bạn sẽ nhận được email nếu đã đăng ký' };
        assert.deepEqual(known.body, neutral);
        for (let admitted = 0; admitted < 3; admitted += 1) {
            assert.deepEqual((await request('POST', '/auth/forgot-password', { body })).body, neutral);
        }
        const limited = await request('POST', '/auth/forgot-password', { body, status: 429, code: 'RATE_LIMITED' });
        assert.match(limited.headers.get('retry-after') ?? '', /^[1-9]\d*$/);
    });

    await run('Password reset accepts a valid token once, replaces the password, and ends all prior sign-ins', async () => {
        const owner = await account();
        const { body: separate } = await login(owner);
        const valid = await resetFixture(owner.id);
        const outstanding = await resetFixture(owner.id);
        const expired = await resetFixture(owner.id, -1);
        const new_password = 'replacement for test only';
        const invalid = { status: 400, code: 'INVALID_RESET_TOKEN' };
        await request('POST', '/auth/reset-password', { body: { token: expired, new_password }, ...invalid });
        await request('POST', '/auth/reset-password', { body: { token: valid, new_password }, status: 204 });
        for (const token of [valid, outstanding]) {
            await request('POST', '/auth/reset-password', { body: { token, new_password }, ...invalid });
        }
        for (const token of [owner.token, separate.access_token]) {
            await request('GET', '/users/me', { token, status: 401, code: 'UNAUTHORIZED' });
        }
        const { username, password } = owner;
        await request('POST', '/auth/login', { body: { username, password }, status: 401, code: 'INVALID_CREDENTIALS' });
        const { body: signedIn } = await login({ username, password: new_password });
        assert.equal((await request('GET', '/users/me', { token: signedIn.access_token })).body.user_id, owner.id);
    });

    await run('User search paginates public profiles using decimal string cursors', async () => {
        const name = `Search_${randomBytes(8).toString('hex')}`;
        const viewer = await account({ full_name: `${name} Viewer` });
        const first = await account({ full_name: `${name} First` });
        const second = await account({ full_name: `${name} Second` });
        const query = { q: name.toUpperCase(), limit: 1 };
        const firstPage = await request('GET', '/users/search', { token: viewer.token, query });
        assert.strictEqual(firstPage.body.has_more, true);
        assert.strictEqual(firstPage.body.next_cursor, first.id);
        const secondPage = await request('GET', '/users/search', {
            token: viewer.token, query: { ...query, cursor: firstPage.body.next_cursor },
        });
        assert.strictEqual(secondPage.body.has_more, false);
        assert.strictEqual(secondPage.body.next_cursor, null);
        assert.deepEqual([firstPage.body.items, secondPage.body.items], [first, second].map((user) => [{
            user_id: user.id, username: user.username, full_name: user.registration.full_name,
            avatar_url: null, relationship: 'NONE',
        }]));
        await request('GET', '/users/search', { token: viewer.token, query: { q: name, limit: 0 }, status: 400 });
    });

    await run('Accepted friendship opens one DM and removing friendship makes it read-only', async () => {
        const sender = await account();
        const recipient = await account();
        const dmRequest = { token: sender.token, body: { target_user_id: recipient.id } };
        await request('POST', '/dms', { ...dmRequest, status: 409 });
        const pending = await requestFriend(sender, recipient);
        assert.match(pending.request_id, /^[1-9]\d*$/);
        assert.equal(pending.requester_id, sender.id);
        assert.equal(pending.target_user_id, recipient.id);
        assert.equal(pending.status, 'PENDING');
        await request('POST', '/friends/requests', { ...dmRequest, status: 409 });
        const relationship = async (viewer, peer) => (await request('GET', '/users/search', {
            token: viewer.token, query: { q: peer.username },
        })).body.items.find((item) => item.user_id === peer.id)?.relationship;
        assert.equal(await relationship(sender, recipient), 'PENDING_OUTGOING');
        assert.equal(await relationship(recipient, sender), 'PENDING_INCOMING');
        assert.equal((await resolveFriend(recipient, pending, 'ACCEPT')).status, 'FRIEND');

        const { body: dm } = await request('POST', '/dms', { ...dmRequest, status: 201 });
        assert.match(dm.room_id, /^[1-9]\d*$/);
        assert.equal(dm.type, 'DM');
        assert.equal(dm.is_private, true);
        assert.equal(dm.is_read_only, false);
        assert.deepEqual(new Set(dm.member_ids), new Set([sender.id, recipient.id]));
        const existing = await request('POST', '/dms', { token: recipient.token, body: { target_user_id: sender.id } });
        assert.equal(existing.body.room_id, dm.room_id);

        const remove = { token: sender.token, params: { target_user_id: recipient.id }, status: 204 };
        await request('DELETE', '/friends/{target_user_id}', remove);
        await request('DELETE', '/friends/{target_user_id}', remove);
        for (const user of [sender, recipient]) {
            const { body: snapshot } = await request('GET', '/bootstrap', { token: user.token });
            assert.equal(snapshot.dms.find((room) => room.room_id === dm.room_id)?.is_read_only, true);
        }
        await request('POST', '/dms', { ...dmRequest, status: 409 });
        await resolveFriend(recipient, await requestFriend(sender, recipient), 'ACCEPT');
        const reopened = await request('POST', '/dms', dmRequest);
        assert.equal(reopened.body.room_id, dm.room_id);
        assert.equal(reopened.body.is_read_only, false);
    });

    await run('Declining clears a request and each person controls their own block', async () => {
        const first = await account();
        const second = await account();
        const pending = await requestFriend(first, second);
        assert.equal((await resolveFriend(second, pending, 'DECLINE')).status, 'DECLINED');
        assert.equal((await request('GET', '/bootstrap', { token: first.token })).body.friend_requests.length, 0);
        assert.equal((await requestFriend(first, second)).status, 'PENDING');

        const block = (user, peer) => request('POST', '/blocks', { token: user.token, body: { target_user_id: peer.id }, status: 204 });
        const unblock = (user, peer) => request('DELETE', '/blocks/{target_user_id}', {
            token: user.token, params: { target_user_id: peer.id }, status: 204,
        });
        await block(first, second);
        await block(first, second);
        const { body: snapshot } = await request('GET', '/bootstrap', { token: first.token });
        assert.deepEqual(snapshot.blocks, [second.id]);
        assert.deepEqual(snapshot.friend_requests, []);
        await block(second, first);
        await unblock(first, second);
        await request('POST', '/friends/requests', { token: first.token, body: { target_user_id: second.id }, status: 409 });
        await unblock(second, first);
        assert.equal((await requestFriend(first, second)).status, 'PENDING');
    });
}
