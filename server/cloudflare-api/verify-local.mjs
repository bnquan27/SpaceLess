import assert from 'node:assert/strict';
import { argon2Sync, createHmac, createHash, randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';
import { readdir, readFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { social } from '../test/accounts.mjs';
import { verifyChannels } from './verify-channels.mjs';
import { googleFixture, verifyGoogle } from './verify-google.mjs';

const require = createRequire(import.meta.url);
const tools = process.env.SPACELESS_TEST_TOOLS;
assert(tools, 'Set SPACELESS_TEST_TOOLS to a node_modules directory with miniflare, pg and ws');
const { Miniflare, Response: WorkerResponse } = require(`${tools}/miniflare`);
const { Client } = require(`${tools}/pg`);
const { WebSocket } = require(`${tools}/ws`);
const dbUrl = new URL(process.env.SPACELESS_TEST_POSTGRES ?? '');
assert.equal(dbUrl.protocol, 'postgresql:');
assert.equal(dbUrl.hostname, '127.0.0.1', 'PostgreSQL must be an isolated loopback cluster');
assert.equal(dbUrl.pathname, '/postgres', 'Connect to the PostgreSQL maintenance database');
assert(dbUrl.port, 'Set the port of the isolated test cluster');
assert.equal(dbUrl.search, '');
const dbName = `spaceless_test_worker_${randomBytes(6).toString('hex')}`;
const role = `${dbName}_api`;
const gate = randomBytes(32).toString('hex');
const jwt = randomBytes(32).toString('hex');
const voiceKey = 'spaceless-livekit-test';
const voiceSecret = randomBytes(32).toString('hex');
const fileKey = randomBytes(32).toString('hex');
const accessKey = randomBytes(16).toString('hex');
const accessSecret = randomBytes(32).toString('hex');
const bucket = 'spaceless-worker-fixture';
const fileHost = `${'1'.repeat(32)}.r2.cloudflarestorage.com`;
const google = googleFixture(WorkerResponse);
const checks = [];
const sockets = [];
const accountHash = passwordHash('spaceless-test-password');

function passwordHash(password) {
  const salt = randomBytes(16);
  const hash = argon2Sync('argon2id', { message: Buffer.from(password), nonce: salt, memory: 19456, passes: 2, parallelism: 1, tagLength: 32 });
  const encode = bytes => bytes.toString('base64').replace(/=+$/, '');
  return `$argon2id$v=19$m=19456,t=2,p=1$${encode(salt)}$${encode(hash)}`;
}

async function run(name, check) {
  try {
    await check();
    checks.push({ name, passed: true });
    console.log(`PASS ${name}`);
  } catch (error) {
    checks.push({ name, passed: false });
    console.error(`FAIL ${name}: ${error.message}`);
  }
}

async function fixturePort() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const { port } = server.address();
  server.close();
  await once(server, 'close');
  return port;
}

function voiceProcess(mode, env) {
  return spawn(process.execPath, [fileURLToPath(new URL('./verify-voice.mjs', import.meta.url)), ...mode], {
    env, stdio: ['ignore', 'pipe', 'inherit'], windowsHide: true,
  });
}

const maintenance = new Client({ connectionString: dbUrl.href, connectionTimeoutMillis: 5000 });
let db;
let mf;
let provider;
let providerLines;
let databaseCreated = false;
let roleCreated = false;
try {
  await maintenance.connect();
  await maintenance.query(`CREATE DATABASE "${dbName}"`);
  databaseCreated = true;
  dbUrl.pathname = `/${dbName}`;
  db = new Client({ connectionString: dbUrl.href, connectionTimeoutMillis: 5000, statement_timeout: 15000 });
  await db.connect();
  const migrations = new URL('../migrations/', import.meta.url);
  for (const migration of (await readdir(migrations)).filter(name => name.endsWith('.sql')).sort()) {
    await db.query(await readFile(new URL(migration, migrations), 'utf8'));
  }
  await db.query(`CREATE ROLE "${role}" LOGIN`);
  roleCreated = true;
  const grants = await readFile(new URL('./worker-role.sql',import.meta.url),'utf8');
  await db.query(grants.replaceAll(':"worker_role"',`"${role}"`));
  const workerDb = new URL(dbUrl);
  workerDb.username = role;

  const providerOrigin = `http://127.0.0.1:${await fixturePort()}`;
  const providerEnv = {
    ...process.env,
    SPACELESS_TEST_LIVEKIT_HTTP: providerOrigin,
    SPACELESS_TEST_LIVEKIT_KEY: voiceKey,
    SPACELESS_TEST_LIVEKIT_SECRET: voiceSecret,
    SPACELESS_TEST_DATABASE_URL: dbUrl.href,
    SPACELESS_STAGING_PG_MODULE: `${tools}/pg`,
    STAGING_GATE_TOKEN: gate,
  };
  provider = voiceProcess(['--fixture-provider'], providerEnv);
  providerLines = createInterface({ input: provider.stdout });
  const line = await Promise.race([once(providerLines,'line').then(([data]) => data), delay(5000, '', { ref: false })]);
  assert.equal(JSON.parse(line).fixtureProvider, 'ready');

  const bindings = {
    ...google.bindings,
    STAGING_GATE_TOKEN: gate, APP_JWT_SECRET: jwt, FILE_GATEWAY_SECRET: fileKey,
    R2_ACCOUNT_ID: '1'.repeat(32), R2_BUCKET: bucket, R2_ACCESS_KEY_ID: accessKey, R2_SECRET_ACCESS_KEY: accessSecret,
    LIVEKIT_URL: providerOrigin.replace(/^http:/, 'ws:'), LIVEKIT_API_KEY: voiceKey,
    LIVEKIT_API_SECRET: voiceSecret, LIVEKIT_CLOUD_REVOCATION: 'true',
  };
  const name = 'spaceless-test';
  const classes = { AUTH_API: 'AuthApi', USER_SOCKETS: 'UserSockets', MESSAGE_OUTBOX: 'MessageOutbox' };
  const configuration = {
    name, compatibilityDate: '2026-11-20',
    manifest: {
      mainModule: 'index.js',
      modulesRoot: fileURLToPath(new URL('./build/',import.meta.url)),
      modules: {
        'index.js': { type: 'esm', contents: await readFile(new URL('./build/index.js',import.meta.url),'utf8') },
        'index_bg.wasm': { type: 'wasm', contents: new Uint8Array(await readFile(new URL('./build/index_bg.wasm',import.meta.url))) },
      },
    },
    env: {
      ...Object.fromEntries(Object.entries(bindings).map(([key,value]) => [key,{ type: 'text', value }])),
      ...Object.fromEntries(Object.entries(classes).map(([key,exportName]) => [key,{ type:'durable-object', worker:name, exportName }])),
      DB: { type:'hyperdrive', id:'local-db', dev:{ connectionString: workerDb.href } },
      FILES: { type:'r2', name:bucket },
    },
    exports: Object.fromEntries(Object.values(classes).map(key => [key,{ type:'durable-object', storage:'sqlite' }])),
  };
  mf = new Miniflare({ cf:false, enableDevRegistry:false, host:'127.0.0.1', workers:[{
    config:configuration,
    dev:{ unsafeEphemeralDurableObjects:true, outboundService:{ type:'fetcher', handler: async request => {
      if (google.handles(request)) return google.fetch(request);
      if (new URL(request.url).origin === providerOrigin) {
        const result = await fetch(request.url, { method:request.method, headers:request.headers,
          body:request.body ? await request.arrayBuffer() : undefined, redirect:'error' });
        return new WorkerResponse(await result.arrayBuffer(), { status:result.status, headers:result.headers });
      }
      throw new Error('Unexpected provider request in the isolated Worker test');
    } } },
  }] });
  const api = await mf.ready;
  const r2 = await mf.getR2Bucket('FILES', name);
  async function call(method, url, options = {}) {
    const headers = new Headers(options.headers);
    if (options.token) headers.set('authorization',`Bearer ${options.token}`);
    if (options.body !== undefined) headers.set('content-type','application/json');
    headers.set('X-SpaceLess-Staging-Token',gate);
    const response = await fetch(url,{method,headers,redirect:'error',signal:AbortSignal.timeout(20000),
      body:options.body===undefined?undefined:JSON.stringify(options.body)});
    const bytes = Buffer.from(await response.arrayBuffer());
    const body = options.asBytes ? bytes : response.headers.get('content-type')?.includes('application/json') ? JSON.parse(bytes.toString()) : null;
    assert.equal(response.status, options.status ?? 200, `${method} ${url.pathname} ${body?.error?.code ?? ''}`);
    if (options.code) assert.equal(body?.error?.code, options.code);
    return {status:response.status,headers:response.headers,body};
  }
  async function request(method, template, options = {}) {
    const path=template.replaceAll(/\{(\w+)\}/g,(_,key)=>encodeURIComponent(options.params[key]));
    const target=new URL(`/api/v1${path}`,api);
    for (const [key,value] of Object.entries(options.query ?? {})) target.searchParams.set(key,value);
    return call(method,target,options);
  }
  const query = (sql,params) => db.query(sql,params);
  let accountId = 9_107_199_254_740_992n;
  async function account(registration = {}) {
    const id=String(++accountId);
    const username=`trial_${id}`;
    const password='spaceless-test-password';
    registration={full_name:username,...registration};
    await query('INSERT INTO users(id,username,full_name,password_hash) VALUES($1,$2,$3,$4)', [id,username,registration.full_name,accountHash]);
    await query('INSERT INTO user_details(user_id,email,date_of_birth,phone) VALUES($1,$2,$3,$4)', [id,`${username}@example.invalid`,'2000-01-01','0900000000']);
    const { body }=await request('POST','/auth/login',{body:{username,password}});
    return { id,username,password,registration,token:body.access_token };
  }
  async function connect(token) {
    const url=new URL('/api/v1/ws',api);
    url.protocol='ws:';
    const ws=new WebSocket(url,{headers:{Authorization:`Bearer ${token}`,'X-SpaceLess-Staging-Token':gate}});
    sockets.push(ws);
    ws.on('error',()=>{});
    const messages=[];
    ws.on('message',data=>messages.push(JSON.parse(data.toString())));
    const closed=once(ws,'close').catch(()=>{});
    await once(ws,'open');
    return { close:()=>ws.close(),closed,wait:async (type,predicate)=>{
      const end=Date.now()+10000;
      while(Date.now()<end){
        const index=messages.findIndex(event=>event.type===type && predicate(event.data));
        if(index!==-1) return messages.splice(index,1)[0];
        await delay(30);
      }
      throw new Error(`No ${type} WebSocket event`);
    } };
  }
  async function uploadSigned(signed, bytes) {
    const url=new URL(signed.put_url);
    assert.equal(url.origin,`https://${fileHost}`);
    const prefix=`/${bucket}/`;
    assert(url.pathname.startsWith(prefix));
    const fields=url.searchParams;
    const signature=fields.get('X-Amz-Signature');
    const [id,date,region,service,requestType]=fields.get('X-Amz-Credential').split('/');
    assert.deepEqual([id,region,service,requestType],[accessKey,'auto','s3','aws4_request']);
    const signedNames=fields.get('X-Amz-SignedHeaders');
    const names=signedNames.split(';');
    assert.deepEqual(names,['content-length','content-type','host','if-none-match']);
    const requestHeaders=new Headers(signed.required_headers);
    requestHeaders.set('host',url.host);
    const unsignedQuery=url.search.slice(1).split('&').filter(pair=>!pair.startsWith('X-Amz-Signature=')).sort().join('&');
    const canonical=['PUT',url.pathname,unsignedQuery,names.map(name=>`${name}:${requestHeaders.get(name)}\n`).join(''),signedNames,'UNSIGNED-PAYLOAD'].join('\n');
    const hash=createHash('sha256').update(canonical).digest('hex');
    const key=[date,region,service,requestType].reduce((key,part)=>createHmac('sha256',key).update(part).digest(),Buffer.from('AWS4'+accessSecret));
    const expected=createHmac('sha256',key).update(['AWS4-HMAC-SHA256',fields.get('X-Amz-Date'),[date,region,service,requestType].join('/'),hash].join('\n')).digest('hex');
    assert.equal(signature,expected,'Presigned URL signs all required headers');
    if (bytes.length!==Number(requestHeaders.get('content-length'))) return new Response(null,{status:400});
    const result=await r2.put(decodeURIComponent(url.pathname.slice(prefix.length)),new Uint8Array(bytes),{
      onlyIf:{etagDoesNotMatch:'*'},httpMetadata:{contentType:requestHeaders.get('content-type')},
    });
    return new Response(null,{status:result?200:412});
  }
  const context={run,request,account,connect,query,uploadSigned,download:(url,options)=>{
    const target=new URL(url);
    assert.equal(target.origin,api.origin);
    return call('GET',target,{...options,asBytes:true});
  }};
  await social(context);
  await verifyChannels(context);
  const { verifyFiles }=await import('./verify-files.mjs');
  await verifyFiles(context);
  await run('Worker Cron removes expired staged uploads from R2 and PostgreSQL', async () => {
    const user = await account();
    const payload = Buffer.from('unattached');
    const { body: upload } = await request('POST', '/users/me/avatar/uploads/presign', {
      token: user.token,
      body: { file_name: 'unattached.png', mime_type: 'image/png', file_size_bytes: payload.length },
    });
    assert.equal((await uploadSigned(upload, payload)).status, 200);
    const { rows: [object] } = await query(
      "UPDATE file_uploads SET expires_at=now()-interval '1 minute',put_expires_at=now()-interval '2 minutes' WHERE id=$1 RETURNING staging_key",
      [upload.upload_id],
    );
    assert(await r2.head(object.staging_key));
    const worker = await mf.getWorker(name);
    const scheduled = await worker.scheduled({ cron: '* * * * *' });
    assert.equal(scheduled.outcome, 'ok');
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      if (!(await query('SELECT id FROM file_uploads WHERE id=$1',[upload.upload_id])).rowCount) break;
      await delay(50);
    }
    assert.equal((await query('SELECT id FROM file_uploads WHERE id=$1',[upload.upload_id])).rowCount,0);
    assert.equal(await r2.head(object.staging_key),null);
  });
  await run('Google signed identity, provisioning and account state', () => verifyGoogle({
    fixture:google,fetch:(path,init)=>mf.dispatchFetch(new URL(path,api),init),gate,query,
  }));
  await run('LiveKit permissions, generation, signed webhooks and revocation', async () => {
    const voice=voiceProcess([], {...providerEnv,SPACELESS_TEST_WORKER_URL:api.href});
    voice.stdout.pipe(process.stdout,{end:false});
    const [status]=await once(voice,'close');
    assert.equal(status,0);
  });
} finally {
  for (const socket of sockets) socket.terminate();
  await mf?.dispose();
  providerLines?.close();
  provider?.kill();
  await db?.end();
  if (databaseCreated) await maintenance.query(`DROP DATABASE "${dbName}"`);
  if (roleCreated) await maintenance.query(`DROP ROLE "${role}"`);
  await maintenance.end();
}
assert(checks.length > 0 && checks.every(check=>check.passed));
console.log(JSON.stringify({ passed:checks.length }));
