import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';

const messages = '/rooms/{room_id}/messages';
const message = `${messages}/{message_id}`;
const reactions = `${message}/reactions`;
const myReaction = `${reactions}/me`;
const readState = '/rooms/{room_id}/read-state';

async function textRoom(request, owner, member) {
    const { body: { channel } } = await request('POST', '/channels', {
        token: owner.token, body: { channel_name: 'Messaging test' }, status: 201,
    });
    const params = { channel_id: channel.channel_id };
    if (member) {
        const { body } = await request('PATCH', '/channels/{channel_id}', {
            params, token: owner.token, body: { join_mode: 'PUBLIC_LINK' },
        });
        await request('POST', '/channel-links/{invite_code}/join', {
            params: { invite_code: body.invite_code }, token: member.token,
        });
    }
    const { body } = await request('POST', '/channels/{channel_id}/rooms', {
        params, token: owner.token, status: 201,
        body: { room_name: 'Text room', type: 'TEXT', is_private: false },
    });
    return { room_id: body.room_id };
}

async function sendMessage(request, room, token, clientId, content = clientId) {
    const { body } = await request('POST', messages, {
        params: room, token, body: { client_id: clientId, content }, status: 201,
    });
    return body;
}

function waitForMessage(socket, type, messageId) {
    return socket.wait(type, data => data.message_id === messageId);
}

function closedWithin(socket) {
    return Promise.race([
        socket.closed,
        delay(5_000, undefined, { ref: false }).then(() => assert.fail('WebSocket did not close')),
    ]);
}

export async function messaging({ run, request, account, connect }) {
    await run('The sixth send across rooms is blocked until the sender window expires', async () => {
        const owner = await account();
        const member = await account();
        const first = await textRoom(request, owner, member);
        const second = await textRoom(request, owner);
        const burst = await Promise.all(Array.from({ length: 5 }, (_, index) =>
            sendMessage(request, index % 2 === 0 ? first : second, owner.token, `burst-${index}`)));
        const sixth = { params: second, token: owner.token, body: { client_id: 'sixth', content: 'sixth' } };
        const blocked = await request('POST', messages, {
            ...sixth, status: 429, code: 'MESSAGE_RATE_LIMITED',
        });
        assert.equal(blocked.body.error.message, 'NÀY BÌNH TĨNH, BẠN ĐANG GỬI TIN NHẮN QUÁ NHANH');
        assert.equal(blocked.headers.get('retry-after'), '2');
        const retry = await request('POST', messages, {
            params: first, token: owner.token, body: { client_id: 'burst-0', content: 'burst-0' },
        });
        assert.equal(retry.body.message_id, burst[0].message_id);
        await sendMessage(request, first, member.token, 'independent');
        await delay(2_000);
        const accepted = await request('POST', messages, { ...sixth, status: 201 });
        assert.equal(accepted.body.room_sequence, '3');
    });

    await run('Message retries preserve identity and edits, and deletion preserves the retry receipt', async () => {
        const { id, token } = await account();
        const room = await textRoom(request, { token });
        const socket = await connect(token);
        try {
            const original = { client_id: 'first', content: 'First version' };
            const sent = await sendMessage(request, room, token, original.client_id, original.content);
            assert.match(sent.message_id, /^[1-9]\d*$/);
            assert.equal(sent.sender_id, id);
            assert.equal(sent.room_id, room.room_id);
            assert.equal(sent.room_sequence, '1');
            assert.equal(sent.time_edited, null);

            const created = await waitForMessage(socket, 'MESSAGE_CREATE', sent.message_id);
            assert.match(created.event_id, /^[1-9]\d*$/);
            assert.match(created.revision, /^[1-9]\d*$/);
            assert.equal(created.stream, `room:${room.room_id}`);
            assert.equal(created.ordinal, 0);
            assert(Number.isFinite(Date.parse(created.occurred_at)));
            assert.deepEqual(created.data, sent);

            const { body: retry } = await request('POST', messages, { params: room, token, body: original });
            assert.deepEqual(retry, sent);
            const params = { ...room, message_id: sent.message_id };
            const { body: edited } = await request('PATCH', message, {
                params, token, body: { content: 'Edited version' },
            });
            assert.equal(edited.message_id, sent.message_id);
            assert.equal(edited.room_sequence, sent.room_sequence);
            assert.equal(edited.content, 'Edited version');
            assert(Number.isFinite(Date.parse(edited.time_edited)));
            const updated = await waitForMessage(socket, 'MESSAGE_UPDATE', sent.message_id);
            assert.deepEqual(updated.data, edited);
            assert.equal(BigInt(updated.revision), BigInt(created.revision) + 1n);

            const { body: editedRetry } = await request('POST', messages, { params: room, token, body: original });
            assert.deepEqual(editedRetry, edited);
            await request('POST', messages, {
                params: room, token, body: { ...original, content: edited.content }, status: 409,
                code: 'CLIENT_ID_REUSED',
            });
            await request('DELETE', message, { params, token, status: 204 });
            const removed = await waitForMessage(socket, 'MESSAGE_DELETE', sent.message_id);
            assert.deepEqual(removed.data, { ...params, room_sequence: sent.room_sequence });
            await request('POST', messages, {
                params: room, token, body: original, status: 409, code: 'MESSAGE_DELETED',
            });
            const { body: history } = await request('GET', messages, { params: room, token });
            assert.deepEqual(history.items, []);
            assert.equal(history.last_message_sequence, sent.room_sequence);
        } finally {
            socket.close();
        }
    });

    await run('Reconnect catches up through a fixed history snapshot and advances across deleted sequences', async () => {
        const owner = await account();
        const { token } = owner;
        const room = await textRoom(request, owner);
        const originalSocket = await connect(token);
        let first;
        try {
            first = await sendMessage(request, room, token, 'first');
            await waitForMessage(originalSocket, 'MESSAGE_CREATE', first.message_id);
        } finally {
            originalSocket.close();
        }
        await closedWithin(originalSocket);

        const second = await sendMessage(request, room, token, 'second');
        const third = await sendMessage(request, room, token, 'third');
        const fourth = await sendMessage(request, room, token, 'fourth');
        await request('DELETE', message, {
            params: { ...room, message_id: third.message_id }, token, status: 204,
        });

        const socket = await connect(token);
        try {
            const { body: snapshot } = await request('GET', '/bootstrap', { token });
            assert.equal(snapshot.profile.user_id, owner.id);
            assert.equal(snapshot.rooms.find(entry => entry.room_id === room.room_id)?.last_message_sequence, '4');
            const { body: page } = await request('GET', messages, {
                params: room, token, query: { after_sequence: first.room_sequence, limit: 1 },
            });
            assert.deepEqual(page.items.map(item => item.message_id), [second.message_id]);
            assert.equal(page.through_sequence, '4');
            assert.equal(page.next_cursor, '2');
            assert.equal(page.has_more, true);
            assert.equal(snapshot.revisions[`room:${room.room_id}`], page.revision);

            const fifth = await sendMessage(request, room, token, 'fifth');
            await waitForMessage(socket, 'MESSAGE_CREATE', fifth.message_id);
            const pinned = { after_sequence: page.next_cursor, through_sequence: page.through_sequence, limit: 1 };
            const { body: lastPage } = await request('GET', messages, { params: room, token, query: pinned });
            assert.deepEqual(lastPage.items.map(item => item.message_id), [fourth.message_id]);
            assert.equal(lastPage.last_message_sequence, '5');
            assert.equal(lastPage.next_cursor, '4');
            assert.equal(lastPage.has_more, false);

            await request('DELETE', message, {
                params: { ...room, message_id: fourth.message_id }, token, status: 204,
            });
            const { body: gap } = await request('GET', messages, { params: room, token, query: pinned });
            assert.deepEqual(gap.items, []);
            assert.equal(gap.next_cursor, '4');
            const { body: backward } = await request('GET', messages, {
                params: room, token, query: { before_sequence: fifth.room_sequence, limit: 2 },
            });
            assert.deepEqual(backward.items.map(item => item.message_id), [second.message_id, first.message_id]);
            await request('GET', messages, {
                params: room, token, query: { before_sequence: '2', after_sequence: '0' },
                status: 400, code: 'INVALID_CURSOR',
            });
        } finally {
            socket.close();
        }
    });

    await run('Reactions report caller-specific state and paginate, replace, and remove one reaction per user', async () => {
        const owner = await account();
        const member = await account();
        const room = await textRoom(request, owner, member);
        const sent = await sendMessage(request, room, owner.token, 'react here');
        const params = { ...room, message_id: sent.message_id };
        const ownerSocket = await connect(owner.token);
        let memberSocket;
        try {
            memberSocket = await connect(member.token);
            await request('PUT', myReaction, {
                params, token: member.token, body: { reaction_type: 'LIKE' }, status: 204,
            });
            const [memberEvent, ownerEvent] = await Promise.all([
                waitForMessage(memberSocket, 'MESSAGE_REACTION_UPDATE', sent.message_id),
                waitForMessage(ownerSocket, 'MESSAGE_REACTION_UPDATE', sent.message_id),
            ]);
            assert.equal(memberEvent.data.my_reaction, 'LIKE');
            assert.equal(ownerEvent.data.my_reaction, null);
            assert.deepEqual(ownerEvent.data.reaction_counts, { LIKE: 1 });
            assert.deepEqual(memberEvent.data.reaction_counts, ownerEvent.data.reaction_counts);

            await request('PUT', myReaction, {
                params, token: member.token, body: { reaction_type: 'STAR' }, status: 204,
            });
            await request('PUT', myReaction, {
                params, token: owner.token, body: { reaction_type: 'STAR' }, status: 204,
            });
            const query = { limit: 1, reaction_type: 'STAR' };
            const { body: page } = await request('GET', reactions, { params, token: owner.token, query });
            assert.equal(page.items.length, 1);
            assert.equal(page.items[0].reaction_type, 'STAR');
            assert.equal(page.has_more, true);
            assert.equal(page.next_cursor, page.items[0].user_id);
            const { body: next } = await request('GET', reactions, {
                params, token: owner.token, query: { ...query, cursor: page.next_cursor },
            });
            assert.equal(next.has_more, false);
            assert.equal(next.next_cursor, null);
            assert.deepEqual([...page.items, ...next.items].map(item => item.user_id).sort(), [owner.id, member.id].sort());

            await request('DELETE', myReaction, { params, token: member.token, status: 204 });
            await request('DELETE', myReaction, { params, token: member.token, status: 204 });
            const { body: history } = await request('GET', messages, { params: room, token: member.token });
            assert.deepEqual(history.items[0].reaction_counts, { STAR: 1 });
            assert.equal(history.items[0].my_reaction, null);
            const { body: remaining } = await request('GET', reactions, { params, token: member.token });
            assert.deepEqual(remaining.items.map(item => [item.user_id, item.reaction_type]), [[owner.id, 'STAR']]);
        } finally {
            memberSocket?.close();
            ownerSocket.close();
        }
    });

    await run('Read marks only advance and synchronize room history, unread counts, and socket events', async () => {
        const owner = await account();
        const member = await account();
        const room = await textRoom(request, owner, member);
        await sendMessage(request, room, owner.token, 'first');
        await sendMessage(request, room, owner.token, 'second');
        const { body: before } = await request('GET', '/bootstrap', { token: member.token });
        assert.equal(before.rooms.find(entry => entry.room_id === room.room_id)?.unread_count, 2);

        const socket = await connect(owner.token);
        try {
            const { body: read } = await request('PUT', readState, {
                params: room, token: member.token, body: { last_read_sequence: '1' },
            });
            assert.equal(read.user_id, member.id);
            assert.equal(read.last_read_sequence, '1');
            assert(Number.isFinite(Date.parse(read.last_read_at)));
            const event = await socket.wait('MESSAGE_READ_UPDATE', data => data.room_id === room.room_id);
            assert.deepEqual(event.data, read);
            const { body: backward } = await request('PUT', readState, {
                params: room, token: member.token, body: { last_read_sequence: '0' },
            });
            assert.deepEqual(backward, read);
            await request('PUT', readState, {
                params: room, token: member.token, body: { last_read_sequence: '3' },
                status: 400, code: 'INVALID_SEQUENCE',
            });

            const { body: history } = await request('GET', messages, { params: room, token: owner.token });
            assert.equal(read.room_id, room.room_id);
            const savedRead = history.read_state.find(entry => entry.user_id === member.id);
            assert.equal(savedRead.last_read_sequence, read.last_read_sequence);
            assert.equal(Date.parse(savedRead.last_read_at), Date.parse(read.last_read_at));
            assert.equal(history.revision, event.revision);
            const { body: after } = await request('GET', '/bootstrap', { token: member.token });
            const saved = after.rooms.find(entry => entry.room_id === room.room_id);
            assert.equal(saved.last_read_sequence, '1');
            assert.equal(saved.unread_count, 1);
        } finally {
            socket.close();
        }
    });

    await run('Refresh closes the old WebSocket and logout closes the replacement', async () => {
        const owner = await account();
        const room = await textRoom(request, owner);
        const old = await connect(owner.token);
        let replacement;
        try {
            const { body: { access_token: token } } = await request('POST', '/auth/refresh', { token: owner.token });
            await closedWithin(old);
            replacement = await connect(token);
            const sent = await sendMessage(request, room, token, 'after refresh');
            await waitForMessage(replacement, 'MESSAGE_CREATE', sent.message_id);
            await request('POST', '/auth/logout', { token, status: 204 });
            await closedWithin(replacement);
        } finally {
            replacement?.close();
            old.close();
        }
    });
}
