import { createHash, createHmac } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';

async function listen(handler) {
  const server = createServer(handler);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    close: () => new Promise(resolve => server.close(resolve)),
  };
}

async function read(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return Buffer.concat(chunks);
}

function storageError(response, status, code, sendBody = true) {
  response.writeHead(status, { 'content-type': 'application/xml' });
  response.end(sendBody ? `<Error><Code>${code}</Code></Error>` : undefined);
}

export async function objectStore() {
  const objects = new Map();
  return listen(async (request, response) => {
    const path = new URL(request.url, 'http://127.0.0.1').pathname;
    const object = objects.get(path);
    if (request.method === 'PUT') {
      const bytes = await read(request);
      if (object && request.headers['if-none-match'] === '*') {
        return storageError(response, 412, 'PreconditionFailed');
      }
      const etag = '"' + createHash('md5').update(bytes).digest('hex') + '"';
      objects.set(path, { bytes, etag, mime: request.headers['content-type'] });
      response.setHeader('etag', etag);
      return response.end();
    }
    if (request.method === 'DELETE') {
      objects.delete(path);
      response.statusCode = 204;
      return response.end();
    }
    if (!object) return storageError(response, 404, 'NoSuchKey', request.method !== 'HEAD');
    if (request.headers['if-match'] && request.headers['if-match'] !== object.etag) {
      return storageError(response, 412, 'PreconditionFailed', request.method !== 'HEAD');
    }
    let bytes = object.bytes;
    const range = /^bytes=(\d+)-(\d+)$/.exec(request.headers.range ?? '');
    if (range) {
      const start = Number(range[1]);
      const end = Math.min(Number(range[2]), bytes.length - 1);
      if (start > end) return storageError(response, 416, 'InvalidRange');
      response.statusCode = 206;
      response.setHeader('content-range', `bytes ${start}-${end}/${bytes.length}`);
      bytes = bytes.subarray(start, end + 1);
    }
    response.setHeader('etag', object.etag);
    response.setHeader('content-type', object.mime);
    response.setHeader('content-length', bytes.length);
    return response.end(request.method === 'HEAD' ? undefined : bytes);
  });
}

export async function liveKit(secret) {
  const key = 'local-livekit-fixture';
  const removals = [];
  const service = await listen(async (request, response) => {
    const body = await read(request);
    response.setHeader('content-type', 'application/json');
    if (request.url === '/twirp/livekit.RoomService/ListRooms') return response.end('{"rooms":[]}');
    if (request.url === '/twirp/livekit.RoomService/RemoveParticipant') {
      removals.push(JSON.parse(body));
      return response.end('{}');
    }
    response.statusCode = 404;
    return response.end('{}');
  });
  return {
    ...service, removals,
    env: {
      LIVEKIT_URL: service.origin.replace('http:', 'ws:'),
      LIVEKIT_API_KEY: key, LIVEKIT_API_SECRET: secret, LIVEKIT_CLOUD_REVOCATION: 'true',
    },
    sign(raw) {
      const sha256 = createHash('sha256').update(raw).digest('base64');
      const encode = value => Buffer.from(JSON.stringify(value)).toString('base64url');
      const jwt = `${encode({ alg: 'HS256', typ: 'JWT' })}.${encode({ iss: key, exp: Math.floor(Date.now() / 1000) + 60, sha256 })}`;
      return `${jwt}.${createHmac('sha256', secret).update(jwt).digest('base64url')}`;
    },
  };
}
