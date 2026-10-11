import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { request } from 'node:http';

// Build first with cargo build. Node 22+; all traffic stays on loopback.
const executable = process.env.SCREEN_SHARE_TEST_HOST || fileURLToPath(new URL('../target/debug/monochrome-capture.exe', import.meta.url));
const token = 'integration-test-token-0123456789';
const providerSecret = 'long-lived-provider-secret-for-local-test';
const child = spawn(executable, ['--desktop-child', '--viewer-only', '--bind', '127.0.0.1:0'], {
  windowsHide: true,
  env: { ...process.env, SCREEN_SHARE_TOKEN: token, SCREEN_SHARE_TURN_KEY_ID: 'test-key', SCREEN_SHARE_TURN_API_TOKEN: providerSecret },
  stdio: ['pipe', 'pipe', 'pipe'],
});
const exit = once(child, 'exit');
let stderr = '';
child.stderr.on('data', chunk => { stderr += chunk; });
try {
  const base = await new Promise((resolve, reject) => {
    let output = '';
    const timeout = setTimeout(() => reject(new Error('Host startup timed out')), 10000);
    child.on('error', error => { clearTimeout(timeout); reject(error); });
    child.on('exit', () => { clearTimeout(timeout); reject(new Error(`Host exited: ${stderr}`)); });
    child.stdout.on('data', chunk => {
      output += chunk;
      const match = output.match(/Viewer: (http:\/\/127\.0\.0\.1:\d+)\//);
      if (match) { clearTimeout(timeout); resolve(match[1]); }
    });
  });
  for (const [path, contentType] of [['/', 'text/html'], ['/viewer.js', 'text/javascript'], ['/viewer.css', 'text/css']]) {
    const response = await fetch(base + path);
    assert.equal(response.status, 200);
    assert.ok(response.headers.get('content-type').startsWith(contentType));
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.equal(response.headers.get('referrer-policy'), 'no-referrer');
    const body = await response.text();
    assert.ok(body.length > 100);
    assert.ok(!body.includes(token), 'Static assets must not contain the host token');
    assert.ok(!body.includes(providerSecret), 'Static assets must not contain provider API credentials');
  }
  assert.equal(await (await fetch(base + '/health')).text(), 'ok');
  const logo = await fetch(base + '/logo.png');
  assert.equal(logo.status, 200);
  assert.equal(logo.headers.get('content-type'), 'image/png');
  assert.equal(logo.headers.get('cache-control'), 'no-store');
  assert.deepEqual([...new Uint8Array(await logo.arrayBuffer()).slice(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const html = await (await fetch(base + '/')).text();
  assert.ok(html.includes('<title>Monochrome - Share</title>'));
  assert.ok(html.includes('1-to-1 Private screen sharing'));
  assert.ok(!/<(?:header|footer)[\s>]/i.test(html), 'Viewer must not have framing header/footer bars');
  for (const suffix of ['', '?token=wrong']) {
    const code = await new Promise((resolve, reject) => {
      const req = request(base + '/ws' + suffix, { headers: {
        Connection: 'Upgrade', Upgrade: 'websocket',
        'Sec-WebSocket-Version': '13', 'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ==',
      } }, response => { response.resume(); resolve(response.statusCode); });
      req.on('upgrade', (_, socket) => { socket.destroy(); reject(new Error('Unauthorized upgrade accepted')); });
      req.on('error', reject);
      req.setTimeout(5000, () => req.destroy(new Error('Authorization check timeout')));
      req.end();
    });
    assert.equal(code, 401);
  }
  // Repeated sessions prove that the admission permit is returned after disconnect.
  for (let index = 0; index < 2; index += 1) {
    await new Promise((resolve, reject) => {
      const ws = new WebSocket(base.replace('http:', 'ws:') + '/ws?token=' + token);
      let received = false;
      const timeout = setTimeout(() => { ws.close(); reject(new Error('WebSocket timed out')); }, 5000);
      ws.onmessage = event => {
        const signal = JSON.parse(event.data);
        assert.equal(signal.type, 'error');
        assert.match(signal.message, /viewer-only/);
        received = true;
      };
      ws.onerror = () => { clearTimeout(timeout); reject(new Error('Authorized WebSocket failed')); };
      ws.onclose = () => {
        clearTimeout(timeout);
        if (received) resolve(); else reject(new Error('Host closed without viewer-only notice'));
      };
    });
  }
  console.log('PASS: static assets, security headers, token rejection, authorized signaling, and session cleanup');
} finally {
  child.stdin.end();
  const timeout = setTimeout(() => child.kill(), 5000);
  const [code] = await exit;
  clearTimeout(timeout);
  assert.equal(code, 0, 'Managed host must exit gracefully when desktop stdin closes');
}
