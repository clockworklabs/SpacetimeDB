import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import test from 'node:test';
import { chromium } from 'playwright';
import { patchAuthRequest, withAuthRequestPatch } from '../src/actions/auth-request-patch.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';

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
    // The route handler must be removed even after failed measurement.
    await submit(); assert.equal(JSON.stringify(received.at(-1)!.body), JSON.stringify(body));
    await context.close();
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
