import assert from 'node:assert/strict';

export async function channels({ run, request, account }) {
    async function snapshot(user) {
        const { body } = await request('GET', '/bootstrap', { token: user.token });
        return body;
    }

    async function createChannel(owner, channelName, inviteeIds = []) {
        const { body } = await request('POST', '/channels', {
            token: owner.token, body: { channel_name: channelName, invitee_ids: inviteeIds }, status: 201,
        });
        assert.equal(typeof body.channel.channel_id, 'string');
        assert.match(body.channel.channel_id, /^[1-9]\d*$/);
        assert.equal(body.channel.creator_id, owner.id);
        assert.equal(body.channel.channel_name, channelName);
        assert.equal(body.channel.join_mode, 'INVITE_ONLY');
        return body;
    }

    async function befriend(sender, recipient) {
        const { body: pending } = await request('POST', '/friends/requests', {
            token: sender.token, body: { target_user_id: recipient.id }, status: 201,
        });
        await request('PATCH', '/friends/requests/{request_id}', {
            params: { request_id: pending.request_id }, token: recipient.token, body: { decision: 'ACCEPT' },
        });
    }

    async function openLink(owner, channelId) {
        const { body } = await request('PATCH', '/channels/{channel_id}', {
            params: { channel_id: channelId }, token: owner.token, body: { join_mode: 'PUBLIC_LINK' },
        });
        assert.equal(body.join_mode, 'PUBLIC_LINK');
        assert.ok(body.invite_code);
        return { invite_code: body.invite_code };
    }

    async function joinLink(user, params) {
        const { body } = await request('POST', '/channel-links/{invite_code}/join', { params, token: user.token });
        return body;
    }

    await run('Channel settings, public preview, idempotent join, and deletion', async () => {
        const owner = await account();
        const visitor = await account();
        await request('POST', '/channels', {
            token: owner.token, body: { channel_name: '   ' }, status: 400, code: 'INVALID_NAME',
        });
        const { channel } = await createChannel(owner, 'Kênh công khai');
        const params = { channel_id: channel.channel_id };
        const link = { invite_code: channel.invite_code };
        await request('GET', '/channel-links/{invite_code}', {
            params: link, token: visitor.token, status: 404, code: 'NOT_FOUND',
        });

        const { body: updated } = await request('PATCH', '/channels/{channel_id}', {
            params, token: owner.token,
            body: { channel_name: 'Kênh thử nghiệm', description: 'Mô tả công khai', join_mode: 'PUBLIC_LINK' },
        });
        assert.equal(updated.channel_name, 'Kênh thử nghiệm');
        assert.equal(updated.description, 'Mô tả công khai');
        assert.equal(updated.invite_code, link.invite_code);
        const { body: preview } = await request('GET', '/channel-links/{invite_code}', {
            params: link, token: visitor.token,
        });
        assert.deepEqual(preview, {
            channel_id: channel.channel_id, channel_name: 'Kênh thử nghiệm', description: 'Mô tả công khai',
            icon_url: null, background_url: null,
        });
        const joined = await joinLink(visitor, link);
        assert.equal(joined.channel_id, channel.channel_id);
        assert.equal(joined.join_mode, 'PUBLIC_LINK');
        assert.equal(Object.hasOwn(joined, 'invite_code'), false);
        assert.deepEqual(await joinLink(visitor, link), joined);
        const visitorChannel = (await snapshot(visitor)).channels.find((item) => item.channel_id === channel.channel_id);
        assert.equal(visitorChannel.role, 'MEMBER');
        assert.deepEqual(visitorChannel.members.filter((member) => member.user_id === visitor.id), [
            { user_id: visitor.id, role: 'MEMBER' },
        ]);

        const { body: cleared } = await request('PATCH', '/channels/{channel_id}', {
            params, token: owner.token, body: { description: null, join_mode: 'INVITE_ONLY' },
        });
        assert.equal(cleared.description, null);
        assert.equal(cleared.invite_code, link.invite_code);
        await request('POST', '/channel-links/{invite_code}/join', {
            params: link, token: visitor.token, status: 404, code: 'NOT_FOUND',
        });
        await openLink(owner, channel.channel_id);
        await request('DELETE', '/channels/{channel_id}', { params, token: owner.token, status: 204 });
        assert.equal((await snapshot(visitor)).channels.some((item) => item.channel_id === channel.channel_id), false);
        await request('GET', '/channel-links/{invite_code}', {
            params: link, token: visitor.token, status: 404, code: 'NOT_FOUND',
        });
    });

    await run('Channel invitations record individual outcomes, accept, decline, and cancel on deletion', async () => {
        const owner = await account();
        const friend = await account();
        const other = await account();
        await befriend(owner, friend);

        const { channel: first, invitations } = await createChannel(owner, 'Lời mời ban đầu', [friend.id, other.id]);
        const initial = invitations.find((item) => item.target_user_id === friend.id).invitation;
        assert.equal(initial.status, 'PENDING');
        assert.equal(initial.inviter_id, owner.id);
        assert.equal(initial.invitee_id, friend.id);
        assert.equal(initial.channel_id, first.channel_id);
        assert.deepEqual(invitations.find((item) => item.target_user_id === other.id), {
            target_user_id: other.id, error: { code: 'INVITATION_UNAVAILABLE' },
        });
        assert.ok((await snapshot(friend)).invitations.some((item) => item.invitation_id === initial.invitation_id));
        await openLink(owner, first.channel_id);
        const { body: accepted } = await request('PATCH', '/invitations/{invitation_id}', {
            params: { invitation_id: initial.invitation_id }, token: friend.token, body: { decision: 'ACCEPT' },
        });
        assert.equal(accepted.status, 'ACCEPTED');
        assert.equal(accepted.channel.channel_id, first.channel_id);
        assert.ok(accepted.resolved_at);
        const friendState = await snapshot(friend);
        assert.equal(friendState.channels.find((item) => item.channel_id === first.channel_id).role, 'MEMBER');
        assert.equal(friendState.invitations.some((item) => item.invitation_id === initial.invitation_id), false);

        const { channel: second } = await createChannel(owner, 'Lời mời tiếp theo');
        const invite = {
            params: { channel_id: second.channel_id }, token: owner.token, body: { target_user_id: friend.id },
        };
        const { body: pending } = await request('POST', '/channels/{channel_id}/invitations', { ...invite, status: 201 });
        assert.equal(pending.status, 'PENDING');
        await request('POST', '/channels/{channel_id}/invitations', {
            ...invite, status: 409, code: 'INVITATION_UNAVAILABLE',
        });
        const { body: declined } = await request('PATCH', '/invitations/{invitation_id}', {
            params: { invitation_id: pending.invitation_id }, token: friend.token, body: { decision: 'DECLINE' },
        });
        assert.equal(declined.status, 'DECLINED');
        assert.equal((await snapshot(friend)).channels.some((item) => item.channel_id === second.channel_id), false);

        const { body: waiting } = await request('POST', '/channels/{channel_id}/invitations', { ...invite, status: 201 });
        assert.notEqual(waiting.invitation_id, pending.invitation_id);
        await request('DELETE', '/channels/{channel_id}', { params: invite.params, token: owner.token, status: 204 });
        await request('PATCH', '/invitations/{invitation_id}', {
            params: { invitation_id: waiting.invitation_id }, token: friend.token,
            body: { decision: 'ACCEPT' }, status: 409, code: 'INVITATION_UNAVAILABLE',
        });
        assert.equal((await snapshot(friend)).invitations.some((item) => item.invitation_id === waiting.invitation_id), false);
    });

    await run('Creator changes member roles, transfers ownership, kicks a member, and leaves', async () => {
        const owner = await account();
        const successor = await account();
        const member = await account();
        const { channel } = await createChannel(owner, 'Quản lý thành viên');
        const params = { channel_id: channel.channel_id };
        const link = await openLink(owner, channel.channel_id);
        await joinLink(successor, link);
        await joinLink(member, link);

        await request('DELETE', '/channels/{channel_id}/members/{user_id}', {
            params: { ...params, user_id: 'me' }, token: owner.token, status: 409, code: 'CREATOR_MUST_TRANSFER',
        });
        const changeRole = {
            params: { ...params, user_id: successor.id }, token: owner.token, body: { role: 'MODERATOR' },
        };
        const { body: promoted } = await request('PATCH', '/channels/{channel_id}/members/{user_id}/role', changeRole);
        assert.deepEqual(promoted.roles, [{ user_id: successor.id, role: 'MODERATOR' }]);
        assert.equal((await snapshot(successor)).channels.find((item) => item.channel_id === channel.channel_id).role, 'MODERATOR');

        const { body: transferred } = await request('POST', '/channels/{channel_id}/transfer-ownership', {
            params, token: owner.token, body: { target_user_id: successor.id },
        });
        assert.deepEqual(Object.fromEntries(transferred.roles.map(({ user_id, role }) => [user_id, role])), {
            [owner.id]: 'MODERATOR', [successor.id]: 'CREATOR',
        });
        const newOwnerChannel = (await snapshot(successor)).channels.find((item) => item.channel_id === channel.channel_id);
        assert.equal(newOwnerChannel.creator_id, successor.id);
        assert.equal(newOwnerChannel.role, 'CREATOR');
        await request('DELETE', '/channels/{channel_id}/members/{user_id}', {
            params: { ...params, user_id: member.id }, token: successor.token, status: 204,
        });
        assert.equal((await snapshot(member)).channels.some((item) => item.channel_id === channel.channel_id), false);
        assert.equal((await joinLink(member, link)).channel_id, channel.channel_id);
        assert.equal((await snapshot(member)).channels.find((item) => item.channel_id === channel.channel_id).role, 'MEMBER');

        await request('DELETE', '/channels/{channel_id}/members/{user_id}', {
            params: { ...params, user_id: 'me' }, token: owner.token, status: 204,
        });
        assert.equal((await snapshot(owner)).channels.some((item) => item.channel_id === channel.channel_id), false);
        const remaining = (await snapshot(successor)).channels.find((item) => item.channel_id === channel.channel_id);
        assert.deepEqual(Object.fromEntries(remaining.members.map(({ user_id, role }) => [user_id, role])), {
            [successor.id]: 'CREATOR', [member.id]: 'MEMBER',
        });
    });

    await run('Category and room mutations persist order and preserve rooms when a category is deleted', async () => {
        const owner = await account();
        const { channel } = await createChannel(owner, 'Sắp xếp phòng');
        const params = { channel_id: channel.channel_id };
        const actor = { params, token: owner.token };
        const { body: firstCategory } = await request('POST', '/channels/{channel_id}/categories', {
            ...actor, body: { category_name: 'Nhóm thứ nhất' }, status: 201,
        });
        const { body: secondCategory } = await request('POST', '/channels/{channel_id}/categories', {
            ...actor, body: { category_name: 'Nhóm thứ hai' }, status: 201,
        });
        const categoryId = firstCategory.category_id;
        const categoryParams = { ...params, category_id: categoryId };
        const { body: renamed } = await request('PATCH', '/channels/{channel_id}/categories/{category_id}', {
            ...actor, params: categoryParams, body: { category_name: 'Nhóm đã đổi tên', position: 1 },
        });
        assert.equal(renamed.category_name, 'Nhóm đã đổi tên');
        assert.equal(renamed.position, 1);

        await request('PUT', '/channels/{channel_id}/categories/order', {
            ...actor, body: { category_ids: [categoryId, categoryId] }, status: 400, code: 'INVALID_ORDER',
        });
        await request('PUT', '/channels/{channel_id}/categories/order', {
            ...actor, body: { category_ids: [categoryId, secondCategory.category_id] }, status: 204,
        });
        const orderedCategories = (await snapshot(owner)).categories.filter((item) => item.channel_id === channel.channel_id);
        assert.deepEqual(orderedCategories.map(({ category_id, position }) => [category_id, position]), [
            [categoryId, 0], [secondCategory.category_id, 1],
        ]);

        const { body: lobby } = await request('POST', '/channels/{channel_id}/rooms', {
            ...actor, body: { room_name: 'Sảnh', type: 'TEXT', is_private: false }, status: 201,
        });
        const { body: voice } = await request('POST', '/channels/{channel_id}/rooms', {
            ...actor, body: { room_name: 'Họp', type: 'VOICE', is_private: false, category_id: categoryId }, status: 201,
        });
        const { body: notes } = await request('POST', '/channels/{channel_id}/rooms', {
            ...actor, body: { room_name: 'Ghi chú', type: 'TEXT', is_private: false }, status: 201,
        });
        assert.equal(lobby.last_message_sequence, '0');
        assert.equal(voice.type, 'VOICE');
        const { body: moved } = await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            ...actor, params: { ...params, room_id: notes.room_id },
            body: { room_name: 'Biên bản', category_id: categoryId, position: 1 },
        });
        assert.equal(moved.category_id, categoryId);
        assert.equal(moved.room_name, 'Biên bản');

        await request('PUT', '/channels/{channel_id}/rooms/order', {
            ...actor, body: { groups: [{ category_id: null, room_ids: [lobby.room_id] }] },
            status: 400, code: 'INVALID_ORDER',
        });
        await request('PUT', '/channels/{channel_id}/rooms/order', {
            ...actor, body: { groups: [
                { category_id: null, room_ids: [lobby.room_id] },
                { category_id: categoryId, room_ids: [notes.room_id, voice.room_id] },
                { category_id: secondCategory.category_id, room_ids: [] },
            ] }, status: 204,
        });
        const grouped = (await snapshot(owner)).rooms.filter((room) => room.category_id === categoryId);
        assert.deepEqual(grouped.map(({ room_id, position }) => [room_id, position]), [
            [notes.room_id, 0], [voice.room_id, 1],
        ]);

        await request('DELETE', '/channels/{channel_id}/categories/{category_id}', {
            ...actor, params: categoryParams, status: 204,
        });
        const ungrouped = await snapshot(owner);
        assert.deepEqual(ungrouped.categories.filter((item) => item.channel_id === channel.channel_id)
            .map(({ category_id, position }) => [category_id, position]), [[secondCategory.category_id, 0]]);
        assert.deepEqual(ungrouped.rooms.filter((room) => room.channel_id === channel.channel_id)
            .map(({ room_id, category_id, position }) => [room_id, category_id, position]), [
            [lobby.room_id, null, 0], [notes.room_id, null, 1], [voice.room_id, null, 2],
        ]);

        await request('DELETE', '/channels/{channel_id}/rooms/{room_id}', {
            ...actor, params: { ...params, room_id: notes.room_id }, status: 204,
        });
        assert.deepEqual((await snapshot(owner)).rooms.filter((room) => room.channel_id === channel.channel_id)
            .map(({ room_id, position }) => [room_id, position]), [[lobby.room_id, 0], [voice.room_id, 1]]);
    });

    await run('Private room visibility follows explicit membership and privacy changes', async () => {
        const owner = await account();
        const member = await account();
        const { channel } = await createChannel(owner, 'Quyền xem phòng');
        const params = { channel_id: channel.channel_id };
        await joinLink(member, await openLink(owner, channel.channel_id));
        const { body: room } = await request('POST', '/channels/{channel_id}/rooms', {
            params, token: owner.token, status: 201,
            body: { room_name: 'Phòng riêng', type: 'TEXT', is_private: true, member_ids: [] },
        });
        const roomParams = { ...params, room_id: room.room_id };
        const visibleRoom = async () => (await snapshot(member)).rooms.find((item) => item.room_id === room.room_id);
        assert.equal(await visibleRoom(), undefined);
        const update = { params: roomParams, token: owner.token };
        await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            ...update, body: { member_ids: [member.id], room_mod_ids: [member.id] },
        });
        const assigned = await visibleRoom();
        assert.equal(assigned.is_private, true);
        assert.equal(assigned.last_read_sequence, '0');

        await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            params: roomParams, token: member.token, body: { room_name: 'Tên do quản trị phòng đặt' },
        });
        assert.equal((await visibleRoom()).room_name, 'Tên do quản trị phòng đặt');
        await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            ...update, body: { is_private: false },
        });
        assert.equal((await visibleRoom()).is_private, false);
        await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            ...update, body: { is_private: true }, status: 400, code: 'ROOM_MEMBERS_REQUIRED',
        });
        assert.equal((await visibleRoom()).is_private, false);
        await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
            ...update, body: { is_private: true, member_ids: [] },
        });
        assert.equal(await visibleRoom(), undefined);
    });
}
