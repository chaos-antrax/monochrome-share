// Browser UI checks with the installed headless Edge; no npm/browser downloads.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const output = resolve(root, 'artifacts');
mkdirSync(output, { recursive: true });
const profile = mkdtempSync(resolve(output, 'viewer-browser-test-'));
const host = spawn(process.env.SCREEN_SHARE_TEST_HOST || resolve(root, 'target/release/monochrome-capture.exe'), ['--desktop-child', '--viewer-only', '--bind', '127.0.0.1:0'], {
  windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  env: { ...process.env, SCREEN_SHARE_TOKEN: 'synthetic-viewer-test-token-123456' },
});
const hostExit = once(host, 'exit');
let browser, ws;
function endpoint(process, pipe, pattern) {
  return new Promise((resolve, reject) => {
    let text = '';
    const timeout = setTimeout(() => reject(new Error('Test process startup timed out')), 15000);
    process.once('error', error => { clearTimeout(timeout); reject(error); });
    process.once('exit', () => { clearTimeout(timeout); reject(new Error('Test process exited early')); });
    pipe.on('data', chunk => {
      text += chunk;
      const match = text.match(pattern);
      if (match) { clearTimeout(timeout); resolve(match[1]); }
    });
  });
}
try {
  host.stderr.resume();
  const base = await endpoint(host, host.stdout, /Viewer: (http:\/\/127\.0\.0\.1:\d+)\//);
  browser = spawn(process.env.SCREEN_SHARE_TEST_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', [
    '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
  ], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
  const address = await endpoint(browser, browser.stderr, /DevTools listening on (ws:\/\/[^\s]+)/);
  ws = new WebSocket(address);
  await once(ws, 'open');
  let id = 0;
  const pending = new Map();
  ws.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    const task = pending.get(message.id);
    if (!task) return;
    pending.delete(message.id);
    clearTimeout(task.timeout);
    if (message.error) task.reject(new Error(message.error.message)); else task.resolve(message.result);
  });
  function call(method, params = {}, sessionId) {
    return new Promise((resolve, reject) => {
      const requestId = ++id;
      const timeout = setTimeout(() => { pending.delete(requestId); reject(new Error(`${method} timed out`)); }, 15000);
      pending.set(requestId, { resolve, reject, timeout });
      ws.send(JSON.stringify({ id: requestId, method, params, sessionId }));
    });
  }
  const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
  const page = (method, params) => call(method, params, sessionId);
  const evaluate = async expression => {
    const result = await page('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text + ': ' + result.exceptionDetails.exception?.description);
    return result.result.value;
  };
  await page('Page.enable');
  for (const [name, width, height] of [['desktop', 1280, 800], ['mobile', 390, 844], ['small-mobile', 320, 568], ['landscape', 844, 390]]) {
    await page('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: width <= 600 });
    await page('Page.navigate', { url: base });
    await evaluate(`new Promise(resolve => { const poll = () => document.readyState === 'complete' && document.querySelector('#controls') ? resolve(true) : setTimeout(poll, 50); poll(); })`);
    const layout = await evaluate(`(() => {
      const dock = document.querySelector('#controls').getBoundingClientRect();
      const overlay = document.querySelector('#overlay').getBoundingClientRect();
      return { title: document.title, logo: document.querySelector('.brand-logo').naturalWidth,
        fullscreenText: document.querySelector('#fullscreen').textContent.trim(),
        bars: document.querySelectorAll('header, footer').length,
        width: innerWidth, dockLeft: dock.left, dockRight: dock.right, dockTop: dock.top,
        overlayBottom: overlay.bottom, controlsInStage: document.querySelector('#stage').contains(document.querySelector('#controls')) };
    })()`);
    assert.equal(layout.title, 'Monochrome - Share');
    assert.ok(layout.logo > 0);
    assert.equal(layout.fullscreenText, '');
    assert.equal(layout.bars, 0);
    assert.ok(layout.controlsInStage);
    assert.ok(layout.dockLeft >= 0 && layout.dockRight <= layout.width, `${name}: controls overflow`);
    assert.ok(layout.overlayBottom <= layout.dockTop, `${name}: overlay overlaps controls`);
    if (name === 'desktop' || name === 'mobile') {
      const screenshot = await page('Page.captureScreenshot', { format: 'png' });
      writeFileSync(resolve(output, `viewer-${name}-preview.png`), Buffer.from(screenshot.data, 'base64'));
    }
  }
  assert.deepEqual(await evaluate(`(() => {
    const v = document.querySelector('#volume'); v.value = '0'; v.dispatchEvent(new Event('input'));
    const silent = document.querySelector('#mute').getAttribute('aria-pressed');
    v.value = '.5'; v.dispatchEvent(new Event('input'));
    return [silent, document.querySelector('#mute').getAttribute('aria-pressed'), document.querySelector('#screen').volume,
      document.querySelector('#mute').querySelectorAll('svg').length];
  })()`), ['true', 'false', 0.5, 2]);
  await evaluate(`document.querySelector('#stage').requestFullscreen()`);
  assert.equal(await evaluate(`document.fullscreenElement.id`), 'stage');
  await evaluate(`new Promise(resolve => { const poll = () => document.querySelector('#fullscreen').getAttribute('aria-label') === 'Exit fullscreen' ? resolve(true) : setTimeout(poll, 25); poll(); })`);
  assert.equal(await evaluate(`document.querySelector('#fullscreen').getAttribute('aria-label')`), 'Exit fullscreen');
  await evaluate(`document.exitFullscreen()`);
  await evaluate(`(() => {
    const video = document.querySelector('#screen');
    Object.defineProperty(video, 'paused', { configurable: true, get: () => false });
    video.dispatchEvent(new Event('playing'));
  })()`);
  await evaluate(`new Promise(resolve => setTimeout(resolve, 3800))`);
  assert.equal(await evaluate(`document.querySelector('#controls').classList.contains('is-hidden')`), true);
  await evaluate(`document.querySelector('#stage').dispatchEvent(new PointerEvent('pointerdown'))`);
  assert.equal(await evaluate(`document.querySelector('#controls').classList.contains('is-hidden')`), false);
  await evaluate(`document.querySelector('#stream-details').open = true`);
  await evaluate(`new Promise(resolve => setTimeout(resolve, 3800))`);
  assert.equal(await evaluate(`document.querySelector('#controls').classList.contains('is-hidden')`), false);
  console.log('PASS: viewer branding, four responsive sizes, icon-only fullscreen, volume/mute, fullscreen labels, auto-hide and reveal');
} finally {
  if (ws) ws.close();
  if (browser) browser.kill();
  host.stdin.end();
  const timeout = setTimeout(() => host.kill(), 5000);
  await hostExit;
  clearTimeout(timeout);
}
