import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { chromium } from 'playwright';
import { installResponseLoss } from '../grader/response-loss.js';

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
