import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { chromium } from 'playwright';
import { installResponseLoss } from '../grader/response-loss.js';
import { installAuthWebSocketCapture, withWriteCompletion } from '../src/actions/auth-request-patch.js';
import { installSpacetimeWriteCapture } from '../src/stacks/backends/spacetime-browser-session.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';

// Both observers must operate on one connection, including after reload. A
// committed write with a suppressed reply must not satisfy browser completion.
test('write capture and response loss compose on every native transport', { timeout: 120_000 }, async t => {
  const codec = await import(new URL('../src/stacks/spacetime-wire-codec.js', import.meta.url).href);
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const encode = (type: { serialize(writer: unknown, value: unknown): void }, value: unknown) => {
    const writer = new codec.BinaryWriter(128); type.serialize(writer, value); return Buffer.from(writer.getBuffer());
  };
  const server = createServer((req, res) => {
    if (req.url?.includes('/schema')) res.end(JSON.stringify({ reducers: [] }));
    else res.end('<!doctype html><p>checkout fixture</p>');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  const commits = new Set<string>(), forwarded: string[] = [];
  sockets.on('connection', (socket: { on(event: string, fn: (raw: Buffer) => void): void;
    send(data: string | Buffer): void }, req: { url: string; headers: Record<string, string> }) => {
    const address = new URL(req.url, url), kind = address.searchParams.get('kind')!, trial = address.searchParams.get('trial')!;
    if (kind === 'spacetime') socket.send(Buffer.concat([Buffer.from([0]), encode(codec.ServerMessage, {
      tag: 'InitialConnection', value: { identity: { __identity__: 1n }, connectionId: { __connection_id__: 1n }, token: 'fixture' },
    })]));
    else if (kind === 'socketio') socket.send(`0${JSON.stringify({ sid: trial, upgrades: [], pingInterval: 25000, pingTimeout: 20000 })}`);
    else socket.send('{"type":"ready"}');
    socket.on('message', raw => {
      if (kind === 'socketio' && String(raw) === '41') return;
      const message = kind === 'spacetime' ? codec.ClientMessage.deserialize(new codec.BinaryReader(raw))
        : kind === 'convex' ? JSON.parse(String(raw)) : JSON.parse(String(raw).slice(2));
      const id = kind === 'spacetime' ? message.value.requestId : message.requestId;
      const cart = kind === 'spacetime' ? message.value.args[0] : kind === 'convex' ? message.args[0].cart : message.cart;
      const key = `${trial}:${cart}`, accepted = !commits.has(key);
      commits.add(key); forwarded.push(createHash('sha256').update(raw).digest('hex'));
      const reply = kind === 'spacetime' ? Buffer.concat([Buffer.from([0]), encode(codec.ServerMessage, {
        tag: 'ReducerResult', value: { requestId: id, timestamp: { __timestamp_micros_since_unix_epoch__: 1n },
          result: { tag: accepted ? 'OkEmpty' : 'Err', ...(accepted ? {} : { value: 'consumed' }) } },
      })]) : kind === 'convex' ? JSON.stringify({ type: 'MutationResponse', requestId: id, success: accepted, result: null })
        : accepted ? `40${JSON.stringify({ sid: trial })}` : '44{"message":"consumed"}';
      setTimeout(() => socket.send(reply), 100);
    });
  });
  const dir = mkdtempSync(join(tmpdir(), 'socket-loss-composition-')), leasePath = join(dir, 'lease.json');
  const prior = { lease: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  const lease = createBackendLease({ runId: 'socket-loss-composition', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, serverUri: url, module: 'shop', dataDir: join(dir, 'data') });
  lease.state = 'active'; writeBackendLease(leasePath, lease);
  process.env.STACK_BENCH_LEASE = leasePath; process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const browser = await chromium.launch({ headless: true });
  const evidence: object[] = [];
  try {
    for (const kind of ['spacetime', 'convex', 'socketio']) for (const gateFirst of [false, true])
      for (const reload of [false, true]) await t.test(`${kind}, gate first=${gateFirst}, reload=${reload}`, async () => {
        const trial = `${kind}-${gateFirst}-${reload}`, context = await browser.newContext(), page = await context.newPage();
        let gate: Awaited<ReturnType<typeof installResponseLoss>> | undefined;
        try {
          if (gateFirst) gate = await installResponseLoss(context);
          await installAuthWebSocketCapture(page);
          if (kind === 'spacetime') await installSpacetimeWriteCapture(page);
          if (!gateFirst) gate = await installResponseLoss(context);
          await page.goto(url);
          const connect = () => page.evaluate(async ({ url, kind, trial }) => {
            const path = kind === 'spacetime' ? '/proxy/v1/database/shop/subscribe'
              : kind === 'convex' ? '/proxy/api/1.0.0/sync' : '/socket.io/';
            const socket = new WebSocket(`${url.replace('http:', 'ws:')}${path}?kind=${kind}&trial=${trial}`
              + (kind === 'socketio' ? '&EIO=4&transport=websocket' : ''), kind === 'spacetime' ? 'v3.bsatn.spacetimedb' : []);
            socket.binaryType = 'arraybuffer'; Object.assign(window, { socket, replies: 0 });
            await new Promise<void>(resolve => socket.addEventListener('message', () => resolve(), { once: true }));
            socket.addEventListener('message', () => { (window as unknown as { replies: number }).replies++; });
          }, { url, kind, trial });
          await connect();
          if (reload) { await page.reload(); await connect(); }
          const send = async (id: number, cart: number) => {
            const packet = kind === 'spacetime' ? encode(codec.ClientMessage, { tag: 'CallReducer',
              value: { reducer: 'checkout', requestId: id, flags: 0, args: new Uint8Array([cart]) } })
              : kind === 'convex' ? JSON.stringify({ type: 'Mutation', requestId: id, udfPath: 'shop:checkout', args: [{ cart }] })
              : `40${JSON.stringify({ requestId: id, cart })}`;
            await page.evaluate(({ packet, kind, id }) => {
              const socket = (window as unknown as { socket: WebSocket }).socket;
              if (kind === 'socketio' && id === 2) socket.send('41');
              socket.send(typeof packet === 'string' ? packet : new Uint8Array(packet));
            }, { packet: typeof packet === 'string' ? packet : [...packet], kind, id });
            return createHash('sha256').update(packet).digest('hex');
          };
          const setup = await withWriteCompletion(page, () => send(1, 1));
          assert.equal(setup.writes.length, 1, 'native capture must still observe setup with the fault installed');
          assert.equal(await page.evaluate(() => (window as unknown as { replies: number }).replies), 1);
          gate!.arm();
          const loss = withWriteCompletion(page, () => send(2, 2)).then(() => 'complete', error => {
            assert(error instanceof ActionInconclusive); return 'inconclusive';
          });
          for (let n = 0; n < 100 && !gate!.evidence().events.some(e => e.kind === 'ws-drop'); n++) await delay(10);
          assert(commits.has(`${trial}:2`), 'independent server state must show the committed operation');
          assert(gate!.evidence().events.some(e => e.kind === 'ws-drop'), 'fault must observe and suppress the native reply');
          const sent = gate!.evidence().events.find(e => e.kind === 'ws-send' && e.sha256 === forwarded.at(-1));
          assert(sent, 'the fault must record the exact bytes forwarded to the server');
          assert.equal(await page.evaluate(() => (window as unknown as { replies: number }).replies), 1);
          assert.equal(await Promise.race([loss, delay(50).then(() => 'waiting')]), 'waiting',
            'a suppressed reply must not resolve capture as complete');
          await gate!.finish(); assert.equal(await loss, 'inconclusive');
          await connect();
          const before = commits.size;
          const retry = await withWriteCompletion(page, () => send(3, 2));
          assert.equal(retry.writes.length, 1); assert.equal(commits.size, before, 'retry cannot duplicate the effect');
          assert.deepEqual(gate!.evidence().errors, []); assert.equal(gate!.evidence().truncated, false);
          evidence.push({ kind, gateFirst, reload, setupWrites: setup.writes.length,
            lostReply: 'inconclusive', fault: gate!.evidence(), duplicateEffect: false });
        } finally { await gate?.finish(); await context.close(); }
      });
  } finally {
    await browser.close(); for (const socket of sockets.clients) socket.terminate(); sockets.close();
    server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (prior.lease === undefined) delete process.env.STACK_BENCH_LEASE; else process.env.STACK_BENCH_LEASE = prior.lease;
    if (prior.token === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN; else process.env.STACK_BENCH_LEASE_TOKEN = prior.token;
    rmSync(dir, { recursive: true, force: true });
    if (process.env.STACK_BENCH_SOCKET_COMPOSITION_EVIDENCE) writeFileSync(process.env.STACK_BENCH_SOCKET_COMPOSITION_EVIDENCE,
      JSON.stringify({ command: 'node --test --test-name-pattern="write capture and response loss compose" dist/tests/response-loss.integration.js',
        cases: evidence, cleanup: 'browser, server and temporary lease closed' }, null, 2) + '\n');
  }
});

test('a lost browser HTTP reply keeps its committed effect and does not change the write', async t => {
  // A lost reply must not apply its Set-Cookie header, even after cleanup or retry.
  // A normally delivered reply must apply it; the duplicate-write refusal sets no cookie.
  const evidence: { loss: boolean; reload: boolean; cookieBeforeFinish: string | null; cookieAfterFinish: string | null;
    cookieAfterRetry: string | null; retryCookie: string | null; retryStatus: number }[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_RESPONSE_LOSS_EVIDENCE)
      writeFileSync(process.env.STACK_BENCH_RESPONSE_LOSS_EVIDENCE, JSON.stringify(evidence, null, 2) + '\n');
  });
  const carts = new Set<string>();
  const retryCookies = new Map<string, string | null>();
  const server = createServer((req, res) => {
    if (req.url === '/checkout') {
      let body = '';
      req.on('data', chunk => { body += chunk; });
      req.on('end', () => {
        assert.equal(req.headers['x-application-header'], 'retained');
        assert((req.headers.cookie ?? '').split('; ').includes('original-session=actor-original'),
          'The intercepted request must retain its original HttpOnly session cookie');
        const cart = JSON.parse(body).cart as string;
        if (carts.has(cart)) {
          retryCookies.set(cart, req.headers.cookie ?? null);
          res.writeHead(409); res.end('cart already consumed'); return;
        }
        carts.add(cart);
        res.setHeader('Set-Cookie', `checkout-session=${cart}; Path=/; HttpOnly; SameSite=Lax`);
        res.end(JSON.stringify({ order: carts.size }));
      });
    } else if (req.url === '/state') res.end(JSON.stringify([...carts]));
    else res.end('<!doctype html><p id="result">ready</p>');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address(); assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  for (const { lose, reload } of [{ lose: false, reload: false }, { lose: true, reload: false },
    { lose: true, reload: true }]) await t.test(`loss=${lose}, reload=${reload}`, async () => {
    const context = await browser.newContext();
    await context.addCookies([{ name: 'original-session', value: 'actor-original', url, httpOnly: true, sameSite: 'Lax' }]);
    const gate = await installResponseLoss(context);
    try {
      const page = await context.newPage(); await page.goto(url);
      // A normal positive checkout may reload this page before the later lost-reply probe.
      if (reload) await page.reload();
      if (lose) gate.arm();
      const cart = `cart-${lose}-${reload}`;
      await page.evaluate(cart => {
        document.querySelector('#result')!.textContent = 'pending';
        void fetch('/checkout', { method: 'POST', headers: { 'x-application-header': 'retained' },
          body: JSON.stringify({ cart }) }).then(async response => {
          document.querySelector('#result')!.textContent = await response.text();
        }).catch(() => { document.querySelector('#result')!.textContent = 'unknown'; });
      }, cart);
      for (let n = 0; n < 200; n++) {
        if (lose ? gate.evidence().events.some(e => e.kind === 'http-response')
          : (await page.locator('#result').innerText()).includes('order')) break;
        await delay(10);
      }
      assert((await (await fetch(`${url}/state`)).json()).includes(cart));
      assert.equal(gate.evidence().events.filter(e => e.kind === 'http-response').length, lose ? 1 : 0);
      if (lose) {
        assert.equal(await page.locator('#result').innerText(), 'pending');
        const request = gate.evidence().events.find(e => e.kind === 'http-request')!;
        assert.equal(request.sha256, createHash('sha256').update(JSON.stringify({ cart })).digest('hex'));
      }
      const sessionCookie = async () => (await context.cookies(url)).find(cookie => cookie.name === 'checkout-session')?.value ?? null;
      const cookieBeforeFinish = await sessionCookie();
      await gate.finish();
      if (lose) await page.waitForFunction(() => document.querySelector('#result')!.textContent === 'unknown');
      const cookieAfterFinish = await sessionCookie();
      const before = carts.size;
      const retry = await page.evaluate(async cart => (await fetch('/checkout', {
        method: 'POST', headers: { 'x-application-header': 'retained' }, body: JSON.stringify({ cart }),
      })).status, cart);
      assert.equal(retry, 409); assert.equal(carts.size, before);
      const observed = { loss: lose, reload, cookieBeforeFinish, cookieAfterFinish, cookieAfterRetry: await sessionCookie(),
        retryCookie: retryCookies.get(cart) ?? null, retryStatus: retry };
      evidence.push(observed);
      assert.deepEqual([observed.cookieBeforeFinish, observed.cookieAfterFinish, observed.cookieAfterRetry],
        [lose ? null : cart, lose ? null : cart, lose ? null : cart], 'Only a delivered reply may update the browser session cookie');
      assert.deepEqual((observed.retryCookie ?? '').split('; ').sort(),
        (lose ? ['original-session=actor-original'] : ['original-session=actor-original', `checkout-session=${cart}`]).sort(),
        'Retry must not send a session cookie from the lost reply');
      assert.deepEqual(gate.evidence().errors, []); assert.equal(gate.evidence().truncated, false);
      assert.throws(() => gate.arm(), /only once/);
    } finally { await gate.finish(); await context.close(); }
  });
});

test('native text and binary WebSocket replies are dropped without changing requests or subprotocol', async t => {
  // A verified dev socket must survive the fault. A root-path application socket
  // with a different token, or a failed client-module lookup, must not be exempt.
  const received: Buffer[] = [];
  let lookupFails = false;
  const server = createServer((req, res) => {
    if (req.url === '/@vite/client') {
      res.writeHead(lookupFails ? 404 : 200, { 'Content-Type': 'text/javascript' });
      res.end('const wsToken = "verified-dev";');
    } else res.end('<!doctype html><p id="result">ready</p>');
  });
  server.on('upgrade', (req, socket) => {
    assert.equal(req.headers['sec-websocket-protocol'], 'gate.test');
    const accept = createHash('sha1').update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\nSec-WebSocket-Protocol: gate.test\r\n\r\n`);
    let pending = Buffer.alloc(0);
    // ponytail: this fixture accepts only small, unfragmented frames; use a real
    // protocol server if a future control needs extended lengths or fragmentation.
    socket.on('data', chunk => {
      pending = Buffer.concat([pending, chunk]);
      while (pending.length >= 2) {
        const opcode = pending[0]! & 15, length = pending[1]! & 127;
        assert(length < 126); assert(pending[1]! & 128); assert(pending[0]! & 128);
        if (pending.length < 6 + length) return;
        const data = Buffer.from(pending.subarray(6, 6 + length));
        for (let i = 0; i < length; i++) data[i] = data[i]! ^ pending[2 + i % 4]!;
        pending = pending.subarray(6 + length);
        if (opcode === 8) { socket.end(); return; }
        assert(opcode === 1 || opcode === 2);
        received.push(data);
        socket.write(Buffer.concat([Buffer.from([128 | opcode, data.length]), data]));
      }
    });
    socket.on('error', () => {});
    t.after(() => socket.destroy());
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address(); assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`, wsUrl = `ws://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  const cases = [
    { binary: false, lose: false, failedLookup: false, reload: false },
    { binary: false, lose: true, failedLookup: false, reload: false },
    { binary: true, lose: false, failedLookup: false, reload: false },
    { binary: true, lose: true, failedLookup: false, reload: false },
    { binary: false, lose: true, failedLookup: true, reload: false },
    { binary: false, lose: true, failedLookup: false, reload: true },
    { binary: true, lose: true, failedLookup: false, reload: true },
  ];
  for (const { binary, lose, failedLookup, reload } of cases) await t.test(`binary=${binary}, loss=${lose}, lookup failure=${failedLookup}, reload=${reload}`, async () => {
    lookupFails = failedLookup;
    const context = await browser.newContext();
    const gate = await installResponseLoss(context);
    try {
      const page = await context.newPage(); await page.goto(url);
      if (reload) {
        // Close an intercepted old application connection, then establish the new one below.
        await page.evaluate(async wsUrl => {
          const socket = new WebSocket(`${wsUrl}/?token=application-session`, 'gate.test');
          await new Promise<void>(resolve => { socket.onopen = () => resolve(); });
        }, wsUrl);
        await page.reload();
      }
      await page.evaluate(async wsUrl => {
        const socket = new WebSocket(`${wsUrl}/?token=application-session`, 'gate.test'); socket.binaryType = 'arraybuffer';
        const dev = new WebSocket(`${wsUrl}/?token=verified-dev`, 'gate.test');
        const devReceived: string[] = [];
        Object.assign(window, { socket, dev, devReceived });
        dev.onmessage = event => devReceived.push(event.data);
        socket.onmessage = event => { document.querySelector('#result')!.textContent =
          typeof event.data === 'string' ? event.data : [...new Uint8Array(event.data)].join(','); };
        await Promise.all([socket, dev].map(ws => new Promise<void>(resolve => { ws.onopen = () => resolve(); })));
      }, wsUrl);
      if (lose) gate.arm();
      const count = received.length;
      await page.evaluate(binary => {
        const socket = (window as unknown as { socket: WebSocket }).socket;
        socket.send(binary ? new Uint8Array([0, 128, 255, 42]) : 'checkout');
      }, binary);
      for (let n = 0; n < 200; n++) {
        if (lose ? gate.evidence().events.some(e => e.kind === 'ws-drop')
          : (await page.locator('#result').innerText()) !== 'ready') break;
        await delay(10);
      }
      assert.equal(received.length, count + 1);
      assert.deepEqual(received[count], binary ? Buffer.from([0, 128, 255, 42]) : Buffer.from('checkout'));
      assert.equal(gate.evidence().events.filter(e => e.kind === 'ws-drop').length, lose ? 1 : 0);
      assert.equal(await page.locator('#result').innerText(), lose ? 'ready' : binary ? '0,128,255,42' : 'checkout');
      await page.evaluate(() => (window as unknown as { dev: WebSocket }).dev.send('tooling-update'));
      for (let n = 0; n < 200; n++) {
        if (failedLookup && lose ? gate.evidence().events.filter(e => e.kind === 'ws-drop').length === 2
          : await page.evaluate(() => (window as unknown as { devReceived: string[] }).devReceived.length === 1)) break;
        await delay(10);
      }
      assert.deepEqual(await page.evaluate(() => (window as unknown as { devReceived: string[] }).devReceived),
        failedLookup && lose ? [] : ['tooling-update'], 'Only positively verified tooling may bypass response loss');
      await gate.finish();
      await page.waitForFunction(expected => (window as unknown as { dev: WebSocket }).dev.readyState === expected,
        failedLookup ? 3 : 1, { timeout: 1000 });
      assert.deepEqual(gate.evidence().errors, []); assert.equal(gate.evidence().truncated, false);
    } finally { await gate.finish(); await context.close(); }
  });
});
