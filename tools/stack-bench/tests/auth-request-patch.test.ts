import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import test from 'node:test';
import { chromium } from 'playwright';
import { installAuthWebSocketCapture, patchAuthRequest, withAuthRequestPatch } from '../src/actions/auth-request-patch.js';
import { ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from '../src/actions/action-contract.js';
import { installResponseLoss } from '../grader/response-loss.js';

test('credential patches preserve native envelopes and reject ambiguous matches', () => {
  const credentials = { username: 'customer', password: 'secret' };
  const fields = JSON.parse('{"isAdmin":true,"__proto__":{"polluted":true}}');
  for (const body of [credentials, { path: 'auth:signIn', args: [{ provider: 'password', params: credentials }] },
    ['customer', 'secret', 'salt'], ['customer', 'secret', 'salt', { isAdmin: false, nonce: 4 }]]) {
    const before = JSON.stringify(body);
    const changed = patchAuthRequest(body, 'customer', 'secret', { fields })!;
    assert(changed.body.includes('"isAdmin":true'));
    assert(changed.body.includes('"__proto__":'));
    assert.equal(JSON.stringify(body), before);
    assert.equal(Object.getPrototypeOf(credentials), Object.prototype);
  }
  assert.equal(patchAuthRequest({ other: 'secret' }, 'customer', 'secret', { fields }), null);
  assert.throws(() => patchAuthRequest([credentials, credentials], 'customer', 'secret', { fields }), /Multiple/);
  assert.throws(() => patchAuthRequest({ ...credentials, repeated: 'customer' }, 'customer', 'secret', { fields }), /Ambiguous/);
});

test('credential patches can replace the located password without guessing its key', () => {
  for (const body of [
    { username: 'customer', password: 'secret' },
    { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret' } }] },
    ['customer', 'secret'],
  ]) {
    const changed = patchAuthRequest(body, 'customer', 'secret', { password: "' OR '1'='1" })!;
    assert.match(changed.body, /' OR '1'='1/);
    assert.doesNotMatch(changed.body, /secret/);
  }
  assert.throws(() => patchAuthRequest({}, 'customer', 'secret', {}), /request change/);
});

test('Convex credential probes preserve the live socket and require its matching response', async () => {
  // Use the WebSocket server bundled with the pinned Playwright dependency.
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const server = createServer((_req, res) => res.end('<body>auth</body>')).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const ws = new wsServer({ server });
  const received: unknown[] = [];
  let mode: 'accept' | 'reject' | 'disconnect' = 'accept';
  ws.on('connection', (socket: { on(event: string, callback: (data: Buffer) => void): void;
    send(data: string): void; close(): void }) => socket.on('message', raw => {
    const request = JSON.parse(String(raw)); received.push(request);
    if (mode === 'disconnect') { socket.close(); return; }
    socket.send(JSON.stringify({ type: `${request.type}Response`, requestId: request.requestId + 1, success: mode !== 'accept', result: null }));
    socket.send(JSON.stringify({ type: `${request.type}Response`, requestId: request.requestId, success: mode === 'accept', result: null }));
  }));
  const browser = await chromium.launch({ headless: true });
  try {
    for (mode of ['accept', 'reject', 'disconnect'] as const) {
      const page = await browser.newPage();
      let sentFrames = 0;
      page.on('websocket', socket => socket.on('framesent', () => sentFrames++));
      await installAuthWebSocketCapture(page);
      await page.goto(url);
      await page.evaluate(async url => {
        const socket = new WebSocket(url.replace('http:', 'ws:') + '/api/1.0.0/sync');
        Object.assign(window, { testSocket: socket });
        await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
      }, url);
      const submit = () => page.evaluate(async () => {
        const socket = (window as unknown as { testSocket: WebSocket }).testSocket;
        const done = new Promise(resolve => {
          socket.addEventListener('message', resolve, { once: true });
          socket.addEventListener('close', resolve, { once: true });
        });
        socket.send(JSON.stringify({ type: 'Action', requestId: 7, udfPath: 'auth:signup',
          args: [{ username: 'customer', password: 'secret', nonce: 'unchanged' }] }));
        await done;
      });
      const result = withAuthRequestPatch(page, 'customer', 'secret', { fields: { role: 'admin' } }, submit);
      if (mode === 'disconnect') await assert.rejects(result, ActionInconclusive);
      else {
        const receipt = (await result).requestPatch;
        assert.equal(receipt.transport, 'convex-websocket');
        assert.equal(receipt.success, mode === 'accept');
        assert.equal(receipt.status, undefined);
        assert(!JSON.stringify(receipt).includes('secret'));
        await submit(); // Outside the probe, the native request remains unchanged.
        assert.equal((received.at(-1) as { args: { role?: string }[] }).args[0]!.role, undefined);
      }
      const changed = received.find(value => (value as { args: { role?: string }[] }).args[0]?.role === 'admin');
      assert.deepEqual(changed, { type: 'Action', requestId: 7, udfPath: 'auth:signup',
        args: [{ username: 'customer', password: 'secret', nonce: 'unchanged', role: 'admin' }] });
      received.length = 0;
      await page.context().close();
      assert(sentFrames > 0, 'routing must preserve passive request observation');
    }
    mode = 'accept';
    const page = await browser.newPage();
    await installAuthWebSocketCapture(page);
    const gate = await installResponseLoss(page.context());
    await page.goto(url);
    await page.evaluate(async url => {
      const socket = new WebSocket(url.replace('http:', 'ws:') + '/api/1.0.0/sync');
      Object.assign(window, { testSocket: socket, replies: 0 });
      socket.onmessage = () => { (window as unknown as { replies: number }).replies++; };
      await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
    }, url);
    gate.arm();
    await page.evaluate(() => (window as unknown as { testSocket: WebSocket }).testSocket.send(JSON.stringify({
      type: 'Mutation', requestId: 9, args: [{}], udfPath: 'shop:checkout' })));
    for (let n = 0; n < 100 && !gate.evidence().events.some(e => e.kind === 'ws-drop'); n++) {
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert(gate.evidence().events.some(e => e.kind === 'ws-drop'));
    assert.equal(await page.evaluate(() => (window as unknown as { replies: number }).replies), 0);
    await gate.finish();
    assert.deepEqual(gate.evidence().errors, []);
    await page.context().close();
  } finally {
    await browser.close(); ws.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('real browser credential patch reaches the native request and keeps uncertain delivery unmeasured', async () => {
  const received: { path: string; body: unknown; cookie?: string; authorization?: string; nonce?: string }[] = [];
  const server = createServer(async (req, res) => {
    if (req.method === 'GET') { res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>auth</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    assert.equal(Buffer.byteLength(raw), Number(req.headers['content-length']));
    received.push({ path: req.url!, body: JSON.parse(raw), cookie: req.headers.cookie,
      authorization: req.headers.authorization, nonce: req.headers['x-nonce'] as string });
    if (req.url === '/lost') { req.socket.destroy(); return; }
    if (req.url === '/redirect') { res.writeHead(307, { Location: '/signup' }).end(); return; }
    res.writeHead(200, { 'Content-Type': 'application/json', 'Set-Cookie': 'sid=created; Path=/; HttpOnly' }).end('{"accepted":true}');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    await context.addCookies([{ name: 'sid', value: 'guest', url }]);
    const page = await context.newPage(); await page.goto(url);
    const body = { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret', flow: 'signUp' } }] };
    const submit = (path = '/signup', data: unknown = body, contentType = 'application/json') => page.evaluate(async ({ path, data, contentType }) => {
      try { return { status: (await fetch(path, { method: 'POST', headers: { 'Content-Type': contentType, Authorization: 'Bearer native-token', 'X-Nonce': 'kept' },
        body: typeof data === 'string' ? data : JSON.stringify(data) })).status }; }
      catch { return { status: 0 }; }
    }, { path, data, contentType });
    const run = (fn: () => Promise<unknown>) => withAuthRequestPatch(page, 'customer', 'secret', { fields: { isAdmin: true } }, fn);
    const result = await run(() => submit());
    assert.equal(result.requestPatch.status, 200);
    assert.match(result.requestPatch.bodySha256, /^[a-f0-9]{64}$/);
    assert.deepEqual(received[0], { path: '/signup', cookie: 'sid=guest', authorization: 'Bearer native-token', nonce: 'kept',
      body: { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret', flow: 'signUp', isAdmin: true } }] } });
    assert.equal((await context.cookies()).find(c => c.name === 'sid')?.value, 'created');
    assert(!JSON.stringify(result).includes('secret'));
    for (const fn of [() => submit('/lost'), () => submit('/redirect'), () => submit('/signup', { other: 'value' }),
      () => submit('/signup', [body, body]), () => submit('/signup', 'username=customer&password=secret', 'application/x-www-form-urlencoded'),
      async () => { await submit(); return submit(); }]) {
      await assert.rejects(run(fn), ActionInconclusive);
    }
    const redirects = received.filter(r => r.path === '/redirect'); assert.equal(redirects.length, 1);
    for (const failure of [new ActionHarnessFailure('browser setup failed'),
      new Error('locator.fill: Unexpected token in selector'),
      new Error('Target page, context or browser has been closed'),
      new ActionInconclusive('input dispatch was not confirmed'), undefined, false, null]) {
      await assert.rejects(run(async () => { throw failure; }), error => {
        assert.equal(error, failure); return true;
      });
    }
    // Incomplete capture cannot turn a browser-control failure into an app defect.
    const timeout = Object.assign(new Error('locator.click: Timeout 1000ms exceeded'), { name: 'TimeoutError' });
    await assert.rejects(run(async () => { throw timeout; }), ActionInconclusive);
    const appFailure = new ActionApplicationFailure('application control was missing');
    await assert.rejects(run(async () => { throw appFailure; }), ActionInconclusive);
    await assert.rejects(run(async () => { await submit(); throw appFailure; }), error => {
      assert.equal(error, appFailure); return true;
    });
    // The route handler must be removed even after failed measurement.
    await submit(); assert.equal(JSON.stringify(received.at(-1)!.body), JSON.stringify(body));
    await context.close();
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
