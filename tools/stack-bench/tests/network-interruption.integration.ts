import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import type { Socket } from 'node:net';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { installNetworkInterruption } from '../src/actions/network-interruption.js';

// A page that reconnects its socket whenever it closes, like a live-update client.
const PAGE = `<script>
  window.received = []; window.devReceived = [];
  // A dev server's reload socket, which the interruption must leave alone.
  new WebSocket('ws://' + location.host + '/?token=dev').onmessage = event => window.devReceived.push(event.data);
  const connect = () => {
    const ws = new WebSocket('ws://' + location.host + '/ws');
    ws.onmessage = event => window.received.push(event.data);
    ws.onclose = () => setTimeout(connect, 100);
  };
  connect();
</script>`;

test('going offline closes open sockets, refuses reconnects, and never delivers held updates', async () => {
  const sockets = new Set<Socket>();
  // The dev server's client module carries its socket token, as Vite serves it.
  const server = createServer((req, res) => req.url === '/@vite/client'
    ? res.writeHead(200, { 'Content-Type': 'text/javascript' }).end('const wsToken = "dev";')
    : res.writeHead(200, { 'Content-Type': 'text/html' }).end(PAGE));
  server.on('upgrade', (req, socket: Socket) => {
    const accept = createHash('sha1').update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}/`;
  const send = (text: string) => { for (const socket of sockets) socket.write(Buffer.concat([Buffer.from([0x81, text.length]), Buffer.from(text)])); };
  const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
  const received = (page: import('playwright').Page) => page.evaluate(() => (window as unknown as { received: string[] }).received);
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    const networkInterruption = await installNetworkInterruption(context);
    const page = await context.newPage();
    await page.goto(url);
    await wait(300);
    const actor = { page, networkInterruption, loc: () => { throw new Error('unused'); } };
    const service = { defaultWithin: 1000, sleep: async (ms: number) => wait(ms) };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service };
    const offline = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', settleMs: 300 },
      { capabilities });
    assert.equal(offline.status, 'passed', offline.summary ?? '');
    assert.equal((offline.observation as { unrouted: number }).unrouted, 0);
    assert.equal((offline.observation as { closed: number }).closed, 1);
    send('during');
    await wait(300);
    const online = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', offline: false,
      settleMs: 500 }, { capabilities });
    assert.equal(online.status, 'passed', online.summary ?? '');
    assert((online.observation as { refused: number }).refused > 0, 'reconnects during the interruption are refused');
    send('after');
    await wait(300);
    assert.deepEqual(await received(page), ['after'], 'the held update is never delivered; the reconnected page receives new ones');
    assert.deepEqual(await page.evaluate(() => (window as unknown as { devReceived: string[] }).devReceived),
      ['during', 'after'], 'the dev-server socket stays connected through the interruption');

    // A client opened without the harness route cannot prove an interruption.
    const plain = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', settleMs: 0 },
      { capabilities: { ...capabilities, actors: { get: () => ({ ...actor, networkInterruption: undefined }) } } });
    assert.equal(plain.status, 'inconclusive');

    // A socket another route handles bypasses the interruption, so the cut is unproven.
    const bypassed = await browser.newContext();
    const bypassInterruption = await installNetworkInterruption(bypassed);
    await bypassed.routeWebSocket('**/ws', route => { route.connectToServer(); });
    const bypassPage = await bypassed.newPage();
    await bypassPage.goto(url);
    await wait(300);
    const unproven = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', settleMs: 0 },
      { capabilities: { ...capabilities, actors: { get: () => ({ ...actor, page: bypassPage, networkInterruption: bypassInterruption }) } } });
    assert.equal(unproven.status, 'inconclusive');
  } finally {
    await browser.close();
    server.close();
    for (const socket of sockets) socket.destroy();
  }
});

test('going offline lets in-flight requests finish and flags one that never does', async () => {
  const server = createServer((req, res) => {
    if (req.url === '/poll') { setTimeout(() => res.end('polled'), 800); return; }
    if (req.url === '/stream') { res.writeHead(200, { 'Content-Type': 'text/event-stream' }).write('data: open\n\n'); return; }
    res.writeHead(200, { 'Content-Type': 'text/html' }).end('<p>app</p>');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}/`;
  const browser = await chromium.launch({ headless: true });
  const service = { defaultWithin: 1000, sleep: async (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) };
  const goOffline = async (path: string) => {
    const context = await browser.newContext();
    const networkInterruption = await installNetworkInterruption(context);
    const page = await context.newPage();
    await page.goto(url);
    await page.evaluate(path => { void fetch(path).then(response => response.text()).then(text => { (window as unknown as { body: string }).body = text; }); }, path);
    await page.waitForTimeout(100);
    const actor = { page, networkInterruption, loc: () => { throw new Error('unused'); } };
    const result = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', settleMs: 0 },
      { capabilities: { actors: { get: () => actor }, 'browser-interaction': service } });
    return { result, body: await page.evaluate(() => (window as unknown as { body?: string }).body) };
  };
  try {
    const poll = await goOffline('/poll');
    assert.equal(poll.result.status, 'passed', poll.result.summary ?? '');
    assert.equal(poll.body, 'polled', 'the long poll completed before the cut');
    const stream = await goOffline('/stream');
    assert.equal(stream.result.status, 'inconclusive', 'an open event stream survives offline emulation');
  } finally {
    await browser.close();
    server.closeAllConnections();
    server.close();
  }
});

test('a socket shaped like the dev reload socket is still cut unless the dev server vouches for its token', async () => {
  const sockets = new Set<Socket>();
  const server = createServer((_req, res) => res.writeHead(200, { 'Content-Type': 'text/html' }).end(`<script>
    window.received = []; window.closes = 0;
    const ws = new WebSocket('ws://' + location.host + '/?token=application-session');
    ws.onmessage = event => window.received.push(event.data);
    ws.onclose = () => window.closes++;
  </script>`));
  server.on('upgrade', (req, socket: Socket) => {
    const accept = createHash('sha1').update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: ${accept}

`);
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    const networkInterruption = await installNetworkInterruption(context);
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${(server.address() as { port: number }).port}/`);
    await page.waitForTimeout(300);
    const actor = { page, networkInterruption, loc: () => { throw new Error('unused'); } };
    const capabilities = { actors: { get: () => actor },
      'browser-interaction': { defaultWithin: 1000, sleep: async (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) } };
    const offline = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'viewer', settleMs: 100 },
      { capabilities });
    assert.equal(offline.status, 'passed', offline.summary ?? '');
    assert.equal((offline.observation as { closed: number }).closed, 1);
    for (const socket of sockets) socket.write(Buffer.from([0x81, 8, ...Buffer.from('stock:52')]));
    await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'viewer', offline: false, settleMs: 300 },
      { capabilities });
    assert.deepEqual(await page.evaluate(() => (window as unknown as { received: string[]; closes: number }).received), []);
    assert.equal(await page.evaluate(() => (window as unknown as { closes: number }).closes), 1,
      'an app without reconnect code stays disconnected');
  } finally {
    await browser.close();
    server.close();
    for (const socket of sockets) socket.destroy();
  }
});
