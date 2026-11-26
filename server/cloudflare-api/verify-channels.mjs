import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { channels } from '../test/channels.mjs';

export async function verifyChannels(context) {
  await channels(context);
  const { run, request, account, connect } = context;

  await run('Worker sends a private room revocation and preserves the remaining members', async () => {
    const owner = await account();
    const moderator = await account();
    const member = await account();
    const { body: { channel } } = await request('POST', '/channels', {
      token: owner.token, body: { channel_name: 'Quyền riêng tư Workspace' }, status: 201,
    });
    assert(BigInt(channel.channel_id) > BigInt(Number.MAX_SAFE_INTEGER));
    const channelParams = { channel_id: channel.channel_id };
    await request('PATCH', '/channels/{channel_id}', {
      token: owner.token, params: channelParams, body: { join_mode: 'PUBLIC_LINK' },
    });
    for (const user of [moderator, member]) {
      await request('POST', '/channel-links/{invite_code}/join', {
        token: user.token, params: { invite_code: channel.invite_code },
      });
    }
    await request('PATCH', '/channels/{channel_id}/members/{user_id}/role', {
      token: owner.token, params: { ...channelParams, user_id: moderator.id },
      body: { role: 'MODERATOR' },
    });

    const moderatorSocket = await connect(moderator.token);
    try {
      const { body: room } = await request('POST', '/channels/{channel_id}/rooms', {
        token: owner.token, params: channelParams, status: 201,
        body: { room_name: 'Phòng giới hạn', type: 'TEXT', is_private: true, member_ids: [member.id] },
      });
      const roomParams = { ...channelParams, room_id: room.room_id };
      const isTargetRoom = data => data?.room_id === room.room_id;
      const created = await moderatorSocket.wait('ROOM_CREATE', isTargetRoom);
      assert.equal(created.stream, `room:${room.room_id}`);
      assert.match(created.revision, /^[1-9]\d*$/);
      await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
        token: moderator.token, params: roomParams, status: 403, code: 'FORBIDDEN',
        body: { denied_moderator_ids: [moderator.id] },
      });
      await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
        token: owner.token, params: roomParams, body: { denied_moderator_ids: [moderator.id] },
      });
      const revoked = await moderatorSocket.wait('ROOM_DELETE', isTargetRoom);
      assert.equal(revoked.stream, `user:${moderator.id}`);
      assert.deepEqual(revoked.data, { channel_id: channel.channel_id, room_id: room.room_id });
      await request('PATCH', '/channels/{channel_id}/rooms/{room_id}', {
        token: moderator.token, params: roomParams, status: 404, code: 'NOT_FOUND',
        body: { room_name: 'Tên trái quyền' },
      });

      async function hasRoom(user) {
        const { body } = await request('GET', '/bootstrap', { token: user.token });
        return body.rooms.some(isTargetRoom);
      }
      assert.equal(await hasRoom(moderator), false);
      assert.equal(await hasRoom(member), true);
    } finally {
      moderatorSocket.close();
      await Promise.race([moderatorSocket.closed, delay(3000, undefined, { ref: false })]);
    }
  });
}
