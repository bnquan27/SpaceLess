import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const [surface, feature, width = '1280'] = process.argv.slice(2);
const routes = { 'sign-in': '/', 'sign-up': '/', recovery: '/', design: '/design' };
assert(['desktop', 'web'].includes(surface) && Object.hasOwn(routes, feature),
  'Usage: node verify-ui.mjs <desktop|web> <sign-in|sign-up|recovery|design> [web width]');
assert(surface !== 'desktop' || (feature !== 'design' && width === '1280'), 'Design and custom viewports use web mode');
assert(/^\d+$/.test(width) && Number(width) >= 320 && Number(width) <= 2400, 'Web width must be 320 to 2400 pixels');

const { chromium } = createRequire(import.meta.url)(process.env.SPACELESS_PLAYWRIGHT || 'playwright');
const executable = join(root, surface === 'desktop' ? 'src-tauri/target/release/spaceless-app.exe' : 'node_modules/vite/bin/vite.js');
await access(executable);
if (surface === 'web') await access(join(root, 'dist/index.html'));
const evidence = join(root, 'docs/qa', `${surface}-${feature}-${new Date().toISOString().replaceAll(':', '-')}`);
await mkdir(evidence, { recursive: true });

const reservePort = createServer();
const port = await new Promise((resolve, reject) => {
  reservePort.once('error', reject).listen(0, '127.0.0.1', () => resolve(reservePort.address().port));
});
await new Promise((resolve, reject) => reservePort.close(error => error ? reject(error) : resolve()));
const origin = `http://127.0.0.1:${port}`;
const probe = surface === 'desktop' ? '/json/version' : '/';
const scratch = surface === 'desktop' ? await mkdtemp(join(await realpath(tmpdir()), 'spaceless-verify-')) : null;
const result = { surface, feature, port, steps: [], network: [], pageErrors: [] };
const log = createWriteStream(join(evidence, 'process.log'));
let browser;
let page;
let child;
let exited;
let failure;

const button = name => page.getByRole('button', { name, exact: true });
const field = name => page.getByLabel(name, { exact: true });
const textVisible = text => page.getByText(text, { exact: true }).waitFor({ state: 'visible' });
const capture = async name => {
  await page.screenshot({ path: join(evidence, `${name}.png`), animations: 'disabled', fullPage: surface === 'web' });
  await writeFile(join(evidence, `${name}.aria.txt`), await page.locator('body').ariaSnapshot());
};

async function doctor() {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    assert(child.exitCode === null && child.signalCode === null, 'App exited before becoming ready. Read process.log.');
    const response = await fetch(origin + probe, { signal: AbortSignal.timeout(500) }).catch(() => null);
    if (response?.ok) return;
    await sleep(250);
  }
  throw new Error(`No response from ${origin}${probe}. Read process.log.`);
}

try {
  if (surface === 'desktop') {
    child = spawn(executable, [], { cwd: root, windowsHide: true, env: {
      ...process.env, WEBVIEW2_USER_DATA_FOLDER: scratch,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
    } });
  } else {
    child = spawn(process.execPath, [executable, 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'],
      { cwd: root, windowsHide: true });
  }
  exited = new Promise(resolve => child.once('close', resolve));
  child.on('error', error => log.write(`${error.message}\n`));
  child.stdout?.pipe(log, { end: false });
  child.stderr?.pipe(log, { end: false });
  result.pid = child.pid;
  await doctor();
  if (surface === 'desktop') {
    result.buildSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
    browser = await chromium.connectOverCDP(origin, { timeout: 10_000 });
    const [context] = browser.contexts();
    page = context.pages()[0] || await context.waitForEvent('page', { timeout: 10_000 });
  } else {
    browser = await chromium.launch();
    page = await browser.newPage({ viewport: { width: Number(width), height: 820 }, reducedMotion: 'reduce' });
    await page.goto(origin + routes[feature], { waitUntil: 'domcontentloaded', timeout: 12_000 });
  }
  page.setDefaultTimeout(8_000);
  const heading = feature === 'design' ? 'Chọn một cách kết nối.' : 'Kết nối lại nhé.';
  await page.getByRole('heading', { name: heading, exact: true }).waitFor({ state: 'visible' });
  assert(surface === 'web' || new URL(page.url()).hostname === 'tauri.localhost', `Unexpected WebView2 document ${page.url()}`);
  result.steps.push('Doctor saw the expected heading on the owned instance.');
  page.on('pageerror', error => result.pageErrors.push(error.message));
  page.on('request', request => {
    if (['fetch', 'xhr'].includes(request.resourceType()) || !['GET', 'HEAD'].includes(request.method())) {
      const url = new URL(request.url());
      result.network.push({ method: request.method(), path: `${url.origin}${url.pathname}` });
    }
  });
  await capture('before');

  if (feature === 'sign-in') {
    await button('Đăng nhập').click();
    await textVisible('Vui lòng nhập tên đăng nhập.');
    await textVisible('Vui lòng nhập mật khẩu.');
    assert(await field('Tên đăng nhập').evaluate(input => input === document.activeElement), 'Username should take focus after empty submit');
    await capture('validation');
    await field('Tên đăng nhập').fill('orbit_probe');
    await field('Mật khẩu').fill('synthetic-password');
    await button('Hiện mật khẩu').click();
    assert.equal(await field('Mật khẩu').getAttribute('type'), 'text');
    await button('Ẩn mật khẩu').click();
    assert.equal(await field('Mật khẩu').getAttribute('type'), 'password');
    await page.getByRole('checkbox', { name: 'Ghi nhớ đăng nhập' }).check();
    await field('Mật khẩu').press('Enter');
    await textVisible('Thông tin đã hợp lệ. Đây là bản xem thử giao diện, chưa kết nối đăng nhập.');
    result.steps.push('Empty submit focused username. Password toggle, checkbox and keyboard submit worked.');
  } else if (feature === 'sign-up') {
    await button('Tạo tài khoản').click();
    await button('Tạo tài khoản').click();
    await textVisible('Vui lòng nhập họ và tên.');
    await capture('validation');
    for (const [label, value] of Object.entries({ 'Tên đăng nhập': 'orbit_probe', 'Họ và tên': 'Orbit Probe',
      Email: 'orbit@example.test', 'Số điện thoại': '+84900000000', 'Ngày sinh': '1990-01-01', 'Mật khẩu': 'synthetic-password' })) {
      await field(label).fill(value);
    }
    await button('Tạo tài khoản').click();
    await textVisible('Thông tin đã hợp lệ. Đây là bản xem thử, tài khoản chưa được tạo.');
    result.steps.push('Registration validated six fields and displayed its preview status.');
  } else if (feature === 'recovery') {
    await button('Quên mật khẩu?').click();
    await field('Email').fill('invalid');
    await button('Gửi yêu cầu khôi phục').click();
    await textVisible('Vui lòng nhập email hợp lệ, ví dụ ban@example.com.');
    await capture('validation');
    await field('Email').fill('orbit@example.test');
    await button('Gửi yêu cầu khôi phục').click();
    await textVisible('Bản xem thử: yêu cầu khôi phục chưa được gửi. Khi kết nối máy chủ, bạn sẽ nhận được email nếu đã đăng ký.');
    result.steps.push('Recovery rejected invalid email and displayed its preview status for valid email.');
  } else {
    await page.getByRole('button', { name: /^Signal/ }).click();
    await button('Xem trên điện thoại').click();
    assert.equal(await button('Xem trên điện thoại').getAttribute('aria-pressed'), 'true');
    await button('Chọn hướng này').click();
    await textVisible('Đã lưu hướng Signal trên thiết bị này.');
    const downloadEvent = page.waitForEvent('download');
    await button('Tải ghi chú').click();
    const download = await downloadEvent;
    assert.equal(download.suggestedFilename(), 'spaceless-signal.md');
    await download.saveAs(join(evidence, download.suggestedFilename()));
    assert.match(await readFile(join(evidence, download.suggestedFilename()), 'utf8'), /Hướng thiết kế Signal/);
    await capture('selection');
    await page.reload();
    await page.getByRole('heading', { name: 'Signal', exact: true }).waitFor();
    await page.goto(origin);
    await page.getByRole('heading', { name: 'Kết nối lại nhé.', exact: true }).waitFor();
    assert(await page.locator('.space-login.direction-orbit').isVisible(), 'Saving a comparison must leave / on Orbit');
    result.steps.push('Signal persisted in /design, exported a note, and / still used Orbit.');
  }
  await capture('after');
  assert.deepEqual(result.network, [], 'Preview must not submit network requests');
  assert.deepEqual(result.pageErrors, []);
  if (feature !== 'design') {
    assert.deepEqual(await page.evaluate(() => Object.keys(localStorage)), [], 'Auth preview must not store credentials or tokens');
  }
} catch (error) {
  failure = error;
  if (page && !page.isClosed()) await capture('failure').catch(() => {});
} finally {
  try {
    await browser?.close();
  } catch (error) { failure ??= error; }
  try {
    if (child?.pid && child.exitCode === null && child.signalCode === null) child.kill();
    if (exited) await exited;
    if (scratch) {
      const resolved = await realpath(scratch);
      assert.equal(dirname(resolved), await realpath(tmpdir()));
      assert(basename(resolved).startsWith('spaceless-verify-'));
      await rm(resolved, { recursive: true, maxRetries: 12, retryDelay: 250 });
    }
    const response = await fetch(origin + probe, { signal: AbortSignal.timeout(500) }).catch(() => null);
    assert.equal(response, null, 'Verification port remains occupied after cleanup');
    result.cleanup = 'Owned process exited and its port closed.';
  } catch (error) { failure ??= error; }
  await new Promise(resolve => log.end(resolve));
}

await writeFile(join(evidence, 'result.json'), JSON.stringify({ ...result, passed: !failure, error: failure?.message }, null, 2));
if (failure) throw new Error(`Verification failed. Evidence: ${evidence}`, { cause: failure });
await access(join(evidence, 'before.png'));
await access(join(evidence, 'after.png'));
console.log(`PASS ${surface} ${feature}. Evidence: ${evidence}`);
