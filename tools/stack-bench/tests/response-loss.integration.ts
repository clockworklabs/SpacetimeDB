import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { chromium } from 'playwright';
import { installResponseLoss } from '../grader/response-loss.js';

test('a lost browser HTTP reply keeps its committed effect and does not change the write', async t => {
  const carts = new Set<string>();
  const server = createServer((req, res) => {
    if (req.url === '/checkout') {
      let body = '';
      req.on('data', chunk => { body += chunk; });
      req.on('end', () => {
        assert.equal(req.headers['x-application-header'], 'retained');
        const cart = JSON.parse(body).cart as string;
        if (carts.has(cart)) { res.writeHead(409); res.end('cart already consumed'); return; }
        carts.add(cart); res.end(JSON.stringify({ order: carts.size }));
      });
    } else if (req.url === '/state') res.end(JSON.stringify([...carts]));
    else res.end('<!doctype html><p id="result">ready</p>');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address(); assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  for (const lose of [false, true]) await t.test(`loss=${lose}`, async () => {
    const context = await browser.newContext();
    const gate = await installResponseLoss(context);
    try {
      const page = await context.newPage(); await page.goto(url);
      if (lose) gate.arm();
      const cart = `cart-${lose}`;
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
      await gate.finish();
      if (lose) await page.waitForFunction(() => document.querySelector('#result')!.textContent === 'unknown');
      const before = carts.size;
      const retry = await page.evaluate(async cart => (await fetch('/checkout', {
        method: 'POST', headers: { 'x-application-header': 'retained' }, body: JSON.stringify({ cart }),
      })).status, cart);
      assert.equal(retry, 409); assert.equal(carts.size, before);
      assert.deepEqual(gate.evidence().errors, []); assert.equal(gate.evidence().truncated, false);
      assert.throws(() => gate.arm(), /only once/);
    } finally { await gate.finish(); await context.close(); }
  });
});

test('native text and binary WebSocket replies are dropped without changing requests or subprotocol', async t => {
  const received: Buffer[] = [];
  const server = createServer((_req, res) => res.end('<!doctype html><p id="result">ready</p>'));
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
  for (const binary of [false, true]) for (const lose of [false, true]) await t.test(`binary=${binary}, loss=${lose}`, async () => {
    const context = await browser.newContext();
    const gate = await installResponseLoss(context);
    try {
      const page = await context.newPage(); await page.goto(url);
      await page.evaluate(async wsUrl => {
        const socket = new WebSocket(`${wsUrl}/native`, 'gate.test'); socket.binaryType = 'arraybuffer';
        Object.assign(window, { socket });
        socket.onmessage = event => { document.querySelector('#result')!.textContent =
          typeof event.data === 'string' ? event.data : [...new Uint8Array(event.data)].join(','); };
        await new Promise<void>(resolve => { socket.onopen = () => resolve(); });
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
      await gate.finish();
      assert.deepEqual(gate.evidence().errors, []); assert.equal(gate.evidence().truncated, false);
    } finally { await gate.finish(); await context.close(); }
  });
});
