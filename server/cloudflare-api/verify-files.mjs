import assert from 'node:assert/strict';

const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMw0Qj4DwAC7AGs0MTr5gAAAABJRU5ErkJggg==', 'base64');
const uploadPath = '/rooms/{room_id}/uploads/presign';
const messagePath = '/rooms/{room_id}/messages';
const attachmentPath = '/attachments/{attachment_id}/url';
const imagePath = '/images/{upload_id}/url';
const metadata = (bytes, mime_type, file_name) => ({ file_size_bytes: bytes.length, mime_type, file_name });

export async function verifyFiles({ run, request, account, uploadSigned, download }) {
  async function channel(owner, channel_name) {
    return (await request('POST', '/channels', {
      token: owner.token, body: { channel_name }, status: 201,
    })).body.channel;
  }

  async function room(owner, channel_id, type) {
    return (await request('POST', '/channels/{channel_id}/rooms', {
      token: owner.token, params: { channel_id }, status: 201,
      body: { room_name: `${type} file room`, type, is_private: false },
    })).body.room_id;
  }

  async function store(signed, bytes, status = 200) {
    assert.match(signed.upload_id, /^[1-9]\d*$/);
    assert(BigInt(signed.upload_id) > BigInt(Number.MAX_SAFE_INTEGER));
    assert.equal(signed.required_headers['if-none-match'], '*');
    assert.equal(signed.required_headers['content-length'], String(bytes.length));
    assert(Date.parse(signed.staged_expires_at) > Date.parse(signed.expires_at));
    assert(Date.parse(signed.expires_at) > Date.now());
    const url = new URL(signed.put_url);
    assert.equal(url.protocol, 'https:');
    assert.equal(url.searchParams.get('X-Amz-SignedHeaders'), 'content-length;content-type;host;if-none-match');
    assert.match(url.searchParams.get('X-Amz-Signature'), /^[a-f0-9]{64}$/);
    const response = await uploadSigned(signed, bytes);
    assert.equal(response.status, status, 'conditional storage PUT status');
    if (response.body) await response.arrayBuffer();
  }

  async function imageLink(token, upload_id, status = 200) {
    return (await request('GET', imagePath, { token, status, params: { upload_id } })).body?.url;
  }

  await run('Worker attachment upload, replay, range and immediate access revocation', async () => {
    const owner = await account();
    const reader = await account();
    const outsider = await account();
    const { channel_id, invite_code } = await channel(owner, 'Tệp phòng thử');
    await request('PATCH', '/channels/{channel_id}', {
      token: owner.token, params: { channel_id }, body: { join_mode: 'PUBLIC_LINK' },
    });
    await request('POST', '/channel-links/{invite_code}/join', { token: reader.token, params: { invite_code } });
    const room_id = await room(owner, channel_id, 'TEXT');
    const voice = await room(owner, channel_id, 'VOICE');
    const contents = Buffer.from('Tệp SpaceLessApp hợp lệ.');
    const body = metadata(contents, 'text/plain', 'fixture.txt');
    const params = { room_id };
    await request('POST', uploadPath, { token: outsider.token, params, body, status: 404 });
    await request('POST', uploadPath, { token: owner.token, params: { room_id: voice }, body, status: 403 });
    await request('POST', uploadPath, { token: owner.token, params, body: { ...body, file_size_bytes: 25000001 }, status: 400 });
    const { body: signed } = await request('POST', uploadPath, { token: owner.token, params, body });
    await store(signed, contents);
    await store(signed, contents, 412);
    await request('POST', messagePath, {
      token: reader.token, params, body: { client_id: 'others-file', content: '', upload_ids: [signed.upload_id] }, status: 409,
    });
    const input = { client_id: 'signed-file', content: '', upload_ids: [signed.upload_id] };
    const { body: message } = await request('POST', messagePath, { token: owner.token, params, body: input, status: 201 });
    const { body: retry } = await request('POST', messagePath, { token: owner.token, params, body: input });
    assert.equal(retry.message_id, message.message_id);
    assert.deepEqual(retry.attachments, message.attachments);
    assert.equal(message.attachments.length, 1);
    const [attachment] = message.attachments;
    assert.equal(attachment.file_name, body.file_name);
    assert.equal(attachment.mime_type, body.mime_type);
    assert.equal(attachment.file_size_bytes, contents.length);
    const attachmentParams = { attachment_id: attachment.attachment_id };
    await request('GET', attachmentPath, { token: outsider.token, params: attachmentParams, status: 404 });
    const { body: { url } } = await request('GET', attachmentPath, { token: reader.token, params: attachmentParams });
    const full = await download(url, { token: reader.token });
    assert.deepEqual(full.body, contents);
    assert.equal(full.headers.get('content-type'), 'text/plain');
    const partial = await download(url, { token: reader.token, headers: { range: 'bytes=0-3' }, status: 206 });
    assert.deepEqual(partial.body, contents.subarray(0, 4));
    assert.equal(partial.headers.get('content-range'), `bytes 0-3/${contents.length}`);
    const tail = await download(url, { token: reader.token, headers: { range: 'bytes=-3' }, status: 206 });
    assert.deepEqual(tail.body, contents.subarray(-3));
    await download(url, { token: owner.token, status: 401 });
    await request('DELETE', '/channels/{channel_id}/members/{user_id}', {
      token: owner.token, params: { channel_id, user_id: reader.id }, status: 204,
    });
    await download(url, { token: reader.token, status: 404 });
  });

  await run('Worker rejects mislabeled content before an attachment can become visible', async () => {
    const owner = await account();
    const { channel_id } = await channel(owner, 'Tệp sai định dạng');
    const room_id = await room(owner, channel_id, 'TEXT');
    const params = { room_id };
    const contents = Buffer.from('this is text, not an image');
    const { body: signed } = await request('POST', uploadPath, {
      token: owner.token, params, body: metadata(contents, 'image/png', 'not-a-png.png'),
    });
    await store(signed, contents);
    await request('POST', messagePath, {
      token: owner.token, params, body: { client_id: 'mislabeled', upload_ids: [signed.upload_id] }, status: 400,
    });
    const { body: history } = await request('GET', messagePath, { token: owner.token, params });
    assert.deepEqual(history.items, []);
    await imageLink(owner.token, signed.upload_id, 404);
  });

  await run('Worker attaches profile and channel images without exposing the private channel', async () => {
    const owner = await account();
    const outsider = await account();
    const { channel_id } = await channel(owner, 'Ảnh hồ sơ và kênh');
    const params = { channel_id };
    const body = metadata(png, 'image/png', 'fixture.png');
    await request('POST', '/users/me/avatar/uploads/presign', { token: owner.token, body: { ...body, mime_type: 'text/plain' }, status: 400 });
    const { body: avatar } = await request('POST', '/users/me/avatar/uploads/presign', { token: owner.token, body });
    await store(avatar, png);
    const { body: profile } = await request('PATCH', '/users/me', { token: owner.token, body: { avatar_upload_id: avatar.upload_id } });
    assert.equal(profile.avatar_url, `/api/v1/images/${avatar.upload_id}/url`);
    const images = {};
    for (const purpose of ['ICON', 'BACKGROUND']) {
      await request('POST', '/channels/{channel_id}/images/uploads/presign', {
        token: outsider.token, params, body: { ...body, purpose }, status: 404,
      });
      const { body: signed } = await request('POST', '/channels/{channel_id}/images/uploads/presign', {
        token: owner.token, params, body: { ...body, purpose },
      });
      await store(signed, png);
      images[purpose.toLowerCase() + '_upload_id'] = signed.upload_id;
    }
    const { body: patched } = await request('PATCH', '/channels/{channel_id}', { token: owner.token, params, body: images });
    for (const [field, id] of Object.entries(images)) {
      assert.equal(patched[field.replace('_upload_id', '_url')], `/api/v1/images/${id}/url`);
      await imageLink(outsider.token, id, 404);
    }
    for (const id of [avatar.upload_id, ...Object.values(images)]) {
      const url = await imageLink(owner.token, id);
      const response = await download(url, { token: owner.token });
      assert.deepEqual(response.body, png);
      assert.equal(response.headers.get('content-type'), 'image/png');
    }
  });
}
