import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import test from 'node:test';
import { chromium } from 'playwright';
import { installAuthWebSocketCapture, patchAuthRequest, withAuthRequestPatch } from '../src/actions/auth-request-patch.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { supabaseAuthRequestPatch } from '../src/stacks/backends/supabase-operations.js';
import { ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from '../src/actions/action-contract.js';
import { installResponseLoss } from '../grader/response-loss.js';

test('credential patches preserve native envelopes and reject ambiguous matches', () => {
  const credentials = { username: 'customer', password: 'secret' };
  const fields = JSON.parse('{"isAdmin":true,"__proto__":{"polluted":true}}');
  const claims = [{ name: 'name' }, { name: 'password' }, { name: 'salt' }, { name: 'claims', fields: ['isAdmin', 'nonce', '__proto__'] }];
  for (const body of [credentials, { path: 'auth:signIn', args: [{ provider: 'password', params: credentials }] },
    ['customer', 'secret', 'salt', { isAdmin: false, nonce: 4 }]]) {
    const before = JSON.stringify(body);
    const changed = patchAuthRequest(body, 'customer', 'secret', { fields }, Array.isArray(body) ? claims : undefined)!;
    assert(changed.body.includes('"isAdmin":true'));
    assert(changed.body.includes('"__proto__":'));
    assert.equal(JSON.stringify(body), before);
    assert.equal(Object.getPrototypeOf(credentials), Object.prototype);
  }
  assert.equal(patchAuthRequest({ other: 'secret' }, 'customer', 'secret', { fields }), null);
  // Positional arguments take a field only where the interface declares it.
  const named = (...names: string[]) => names.map(name => ({ name }));
  const positional = ['customer', 'secret', 'salt', false];
  assert.equal(patchAuthRequest(positional, 'customer', 'secret', { fields: { isAdmin: true } },
    named('name', 'password', 'salt', 'is_admin'))!.body, '["customer","secret","salt",true]');
  assert.deepEqual(patchAuthRequest(positional.slice(0, 3), 'customer', 'secret', { fields: { role: 'admin' } },
    named('name', 'password', 'salt')), { body: '["customer","secret","salt"]', shape: 'positional', absentParameters: ['role'] });
  assert.throws(() => patchAuthRequest(positional.slice(0, 3), 'customer', 'secret', { fields }), /interface parameters/);
  // A declared top-level parameter wins over a trailing object that does not declare the field.
  const mixed = ['customer', 'secret', 'shopper', { nonce: 'keep' }];
  assert.equal(patchAuthRequest(mixed, 'customer', 'secret', { fields: { role: 'admin' } },
    [...named('name', 'password', 'role'), { name: 'metadata', fields: ['nonce'] }])!.body,
  '["customer","secret","admin",{"nonce":"keep"}]');
  assert.throws(() => patchAuthRequest(mixed, 'customer', 'secret', { fields: { role: 'admin' } }), /interface parameters/,
    'a trailing object is never chosen just because it is last');
  assert.throws(() => patchAuthRequest(['customer', 'secret', { role: 'x' }, { role: 'y' }], 'customer', 'secret',
    { fields: { role: 'admin' } }, [...named('name', 'password'), { name: 'a', fields: ['role'] }, { name: 'b', fields: ['role'] }]),
  /Ambiguous/);
  // A type that could hide the field leaves its location unknown; an exact top-level target is still proven.
  assert.throws(() => patchAuthRequest(['customer', 'secret', { some: { role: 'x' } }], 'customer', 'secret',
    { fields: { role: 'admin' } }, [...named('name', 'password'), { name: 'claims', open: true }]), /location unknown/);
  assert.equal(patchAuthRequest(['customer', 'secret', 'shopper', {}], 'customer', 'secret', { fields: { role: 'admin' } },
    [...named('name', 'password', 'role'), { name: 'claims', open: true }])!.body, '["customer","secret","admin",{}]');
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

test('a SpacetimeDB signup patch places authority by the module schema and never guesses', async () => {
  // An object parameter is [name, its fields]; the schema declares it through the typespace, as a module does.
  // [name, type] gives a raw schema type, with any referenced types in `typespace`.
  let parameters: (string | [string, string[] | object])[] | null = ['name', 'password', 'salt'];
  let typespace: unknown[] = [];
  const calls: unknown[] = [];
  const server = createServer(async (req, res) => {
    if (req.url?.startsWith('/v1/database/shop/schema')) {
      if (!parameters) { res.writeHead(404).end(); return; }
      const types: unknown[] = [...typespace];
      const elements = parameters.map(parameter => typeof parameter === 'string'
        ? { name: { some: parameter }, algebraic_type: { String: [] } }
        : !Array.isArray(parameter[1]) ? { name: { some: parameter[0] }, algebraic_type: parameter[1] }
        : { name: { some: parameter[0] }, algebraic_type: { Ref: types.push({ Product: { elements: parameter[1].map(field =>
          ({ name: { some: field }, algebraic_type: { String: [] } })) } }) - 1 } });
      res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ typespace: { types }, reducers: [],
        misc_exports: [{ Procedure: { name: 'sign_up', params: { elements } } }] }));
      return;
    }
    if (req.method === 'POST') {
      let raw = ''; for await (const chunk of req) raw += chunk;
      calls.push(JSON.parse(raw));
      res.writeHead(200, { 'Content-Type': 'application/json' }).end('true'); return;
    }
    res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>shop</body>');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage(); await page.goto(url);
    const signUp = (args: unknown[]) => withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } },
      () => page.evaluate(async args => {
        try { return (await fetch('/v1/database/shop/call/sign_up', { method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: 'Bearer native' }, body: JSON.stringify(args) })).status; }
        catch { return 0; }
      }, args));
    // The reference interface has no authority parameter: the schema proves it, and the request goes as sent.
    const correct = await signUp(['claimant', 'secret', 'salt']);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt']);
    assert.deepEqual(correct.requestPatch.absentParameters, ['role']);
    // A defective interface that accepts a role receives the claim where it declared it.
    parameters = ['name', 'password', 'salt', 'role'];
    await signUp(['claimant', 'secret', 'salt', 'customer']);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt', 'admin']);
    // So does one that declares the role inside a claims object.
    parameters = ['name', 'password', 'salt', ['claims', ['role']]];
    await signUp(['claimant', 'secret', 'salt', { role: 'customer' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt', { role: 'admin' }]);
    // A declared role parameter receives the claim even when a trailing object follows it.
    parameters = ['name', 'password', 'role', ['metadata', ['nonce']]];
    await signUp(['claimant', 'secret', 'customer', { nonce: 'keep' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'admin', { nonce: 'keep' }]);
    // Without parameter names the probe is unmeasured, not sent in a guessed shape.
    parameters = null;
    await assert.rejects(signUp(['claimant', 'secret', 'salt', { role: 'customer' }]), ActionInconclusive);
    assert.equal(calls.length, 4);
    // Optional claims: the role goes inside the object the app sent.
    const field = (name: string, algebraic_type: object) => ({ name: { some: name }, algebraic_type });
    typespace = [{ Sum: { variants: [field('some', { Product: { elements: [field('role', { String: [] }),
      field('nonce', { String: [] })] } }), field('none', { Product: { elements: [] } })] } }];
    parameters = ['name', 'password', ['claims', { Ref: 0 }]];
    await signUp(['claimant', 'secret', { some: { role: 'customer', nonce: 'keep' } }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', { some: { role: 'admin', nonce: 'keep' } }]);
    // Types the reader cannot see into are not proof of absence, and an absent option is not built:
    // each leaves the probe unmeasured and sends nothing.
    for (const [claims, argument] of [[{ Ref: 0 }, { none: [] }], [{ Ref: 9 }, { role: 'customer' }],
      [{ Product: { elements: [field('settings', { Product: { elements: [field('role', { String: [] })] } })] } },
        { settings: { role: 'customer' } }]] as const) {
      parameters = ['name', 'password', ['claims', claims]];
      await assert.rejects(signUp(['claimant', 'secret', argument]), ActionInconclusive);
    }
    assert.equal(calls.length, 5);
    // A page whose own submit throws on the aborted request is still unmeasured, not a harness failure.
    parameters = ['name', 'password', ['claims', { Ref: 9 }]];
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } },
      () => page.evaluate(() => fetch('/v1/database/shop/call/sign_up', { method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(['claimant', 'secret', { some: { role: 'customer' } }]) }).then(response => response.status))),
    ActionInconclusive);
    assert.equal(calls.length, 5);
    // An optional plain value cannot hold the field, so absence stays measurable.
    parameters = ['name', 'password', ['nickname', { Sum: { variants: [field('some', { String: [] }),
      field('none', { Product: { elements: [] } })] } }]];
    const optional = await signUp(['claimant', 'secret', { some: 'pat' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', { some: 'pat' }]);
    assert.deepEqual(optional.requestPatch.absentParameters, ['role']);
  } finally {
    await browser.close();
    server.close();
  }
});


test('a platform password endpoint patch reaches the cross-origin request only through the stack hook', async () => {
  const received: { path: string; body: unknown; apikey?: string }[] = [];
  const gateway = createServer(async (req, res) => {
    const cors = { 'Access-Control-Allow-Origin': String(req.headers.origin ?? '*'),
      'Access-Control-Allow-Headers': 'apikey, authorization, content-type', 'Access-Control-Allow-Methods': 'POST' };
    if (req.method === 'OPTIONS') { res.writeHead(204, cors).end(); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    received.push({ path: req.url!, body: JSON.parse(raw), apikey: req.headers.apikey as string });
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"access_token":"token"}');
  }).listen(0, '127.0.0.1');
  const app = createServer((_req, res) => res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>shop</body>'))
    .listen(0, '127.0.0.1');
  await Promise.all([once(gateway, 'listening'), once(app, 'listening')]);
  const origin = (server: typeof app) => `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const lease = createBackendLease({ runId: 'auth-patch', backend: 'supabase', track: 'ecommerce', runIndex: 0,
    serverUri: origin(gateway), database: 'postgres' });
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage(); await page.goto(origin(app));
    // The application derives both credentials; neither typed value appears in the request.
    const email = `${Buffer.from('claimant').toString('hex')}@accounts.invalid`;
    const digest = createHash('sha256').update('secret').digest('hex');
    const signUp = () => page.evaluate(async ({ url, email, digest }) => (await fetch(`${url}/auth/v1/signup`, {
      method: 'POST', headers: { 'Content-Type': 'application/json;charset=UTF-8', apikey: 'anon' },
      body: JSON.stringify({ email, password: digest, data: { username: 'claimant' } }) })).status,
    { url: origin(gateway), email, digest });
    const result = await withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } }, signUp,
      supabaseAuthRequestPatch(lease));
    assert.equal(result.requestPatch.shape, 'supabase-auth');
    assert.equal(result.requestPatch.status, 200);
    assert.deepEqual(received.at(-1), { path: '/auth/v1/signup', apikey: 'anon',
      body: { email, password: digest, role: 'admin', data: { username: 'claimant', role: 'admin' } } });
    assert(!JSON.stringify(result).includes(digest));
    // Without the stack hook the request holds neither typed credential, so nothing is proven or changed.
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } }, signUp),
      ActionInconclusive);
    assert.deepEqual(received.at(-1)!.body, { email, password: digest, data: { username: 'claimant' } });
    // An invalid change is refused before any hook sees the request.
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', {}, signUp, supabaseAuthRequestPatch(lease)),
      ActionInconclusive);
    assert.equal(received.length, 2);
    // A query-like password reaches password sign-in in place of the application's derived value.
    const signIn = () => page.evaluate(async ({ url, email, digest }) => (await fetch(`${url}/auth/v1/token?grant_type=password`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', apikey: 'anon' },
      body: JSON.stringify({ email, password: digest }) })).status, { url: origin(gateway), email, digest });
    await withAuthRequestPatch(page, 'kim', 'not-kims-password', { password: "' OR '1'='1" }, signIn,
      supabaseAuthRequestPatch(lease));
    assert.deepEqual(received.at(-1), { path: '/auth/v1/token?grant_type=password', apikey: 'anon',
      body: { email, password: "' OR '1'='1" } });
  } finally {
    await browser.close();
    gateway.close(); app.close();
  }
});
