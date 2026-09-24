import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { connect } from 'node:net';
import type { Socket } from 'node:net';
import test from 'node:test';
import { chromium } from 'playwright';
import type { Browser, Page } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { startNetworkInterruption } from '../src/actions/network-interruption.js';
import type { NetworkInterruption } from '../src/actions/network-interruption.js';

// A fixture app server: its own page, WebSocket upgrades, and a live value it can push.
async function fixture(page: string, http: (req: IncomingMessage, res: ServerResponse, value: () => string) => boolean = () => false) {
  let value = '1';
  const sockets = new Set<Socket>(), streams = new Set<() => void>(), waiters = new Set<() => void>();
  const server = createServer((req, res) => {
    const path = new URL(req.url!, 'http://fixture').pathname;
    if (path === '/@vite/client') { res.writeHead(200, { 'Content-Type': 'text/javascript' }).end('const wsToken = "dev";'); return; }
    if (path === '/events' || path === '/stream') {
      res.writeHead(200, { 'Content-Type': path === '/events' ? 'text/event-stream' : 'text/plain' });
      const send = () => res.write(path === '/events' ? `retry: 300\ndata: ${value}\n\n` : `${value}\n`);
      send(); streams.add(send); res.on('close', () => streams.delete(send)); return;
    }
    if (path === '/poll') {
      if (new URL(req.url!, 'http://fixture').searchParams.get('since') !== value) { res.end(value); return; }
      const answer = () => { waiters.delete(answer); res.end(value); };
      waiters.add(answer); res.on('close', () => waiters.delete(answer)); return;
    }
    if (http(req, res, () => value)) return;
    res.writeHead(200, { 'Content-Type': 'text/html' }).end(page);
  });
  server.on('upgrade', (req, socket: Socket) => {
    const accept = createHash('sha1').update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const frame = (text: string) => Buffer.concat([Buffer.from([0x81, text.length]), Buffer.from(text)]);
  return {
    url: `http://127.0.0.1:${(server.address() as { port: number }).port}/`,
    set(next: string) { value = next; for (const send of streams) send(); for (const answer of [...waiters]) answer(); },
    send(text: string) { for (const socket of sockets) socket.write(frame(text)); },
    close() { for (const socket of sockets) socket.destroy(); server.closeAllConnections(); server.close(); },
  };
}

// One interruptible actor, driven through the real setOffline action.
async function interruptibleActor(browser: Browser, url: string, route = true) {
  const networkInterruption = await startNetworkInterruption();
  const context = await browser.newContext(route ? { proxy: networkInterruption.proxy } : {});
  networkInterruption.attach(context);
  const page = await context.newPage();
  await page.goto(url);
  await page.waitForTimeout(500);
  const actor = { page, networkInterruption, loc: () => { throw new Error('unused'); } };
  const capabilities = { actors: { get: () => actor },
    'browser-interaction': { defaultWithin: 1000, sleep: async (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) } };
  const setOffline = (offline: boolean, settleMs = 300) => executeAction(ACTION_REGISTRY, 'setOffline',
    { do: 'setOffline', actor: 'buyer', offline, settleMs }, { capabilities });
  return { page, networkInterruption, setOffline };
}

const windowValue = (page: Page, key: string) => page.evaluate(name => Reflect.get(window, name), key);

test('going offline closes open sockets, never delivers held updates, and keeps the dev reload socket', async () => {
  const app = await fixture(`<script>
    window.received = []; window.devReceived = []; window.loadedAt = Date.now();
    new WebSocket('ws://' + location.host + '/?token=dev').onmessage = event => window.devReceived.push(event.data);
    const connect = () => {
      const ws = new WebSocket('ws://' + location.host + '/ws');
      ws.onmessage = event => window.received.push(event.data);
      ws.onclose = () => setTimeout(connect, 100);
    };
    connect();
  </script>`);
  const browser = await chromium.launch({ headless: true });
  let interruption: NetworkInterruption | undefined;
  try {
    const { page, networkInterruption, setOffline } = await interruptibleActor(browser, app.url);
    interruption = networkInterruption;
    // A reloaded document's sockets end without a close event; they must not count as open.
    await page.reload();
    await page.waitForTimeout(500);
    const loadedAt = await windowValue(page, 'loadedAt');
    const offline = await setOffline(true);
    assert.equal(offline.status, 'passed', offline.summary ?? '');
    assert.deepEqual((offline.observation as { open: string[] }).open, []);
    app.send('during');
    await page.waitForTimeout(300);
    const online = await setOffline(false, 800);
    assert.equal(online.status, 'passed', online.summary ?? '');
    app.send('after');
    await page.waitForTimeout(300);
    assert.deepEqual(await windowValue(page, 'received'), ['after'], 'the held update is never delivered; the reconnected page receives new ones');
    assert.deepEqual(await windowValue(page, 'devReceived'), ['during', 'after'], 'the dev reload socket stays connected');
    assert.equal(await windowValue(page, 'loadedAt'), loadedAt, 'the page was not reloaded');
  } finally {
    await interruption?.dispose();
    await browser.close();
    app.close();
  }
});

// EventSource, a held long poll, and a streamed fetch, each with or without its own recovery.
const streamingPage = (recover: boolean) => `<script>
  window.loadedAt = Date.now(); window.state = { sse: null, poll: null, stream: null };
  const es = new EventSource('/events');
  es.onmessage = e => { state.sse = e.data; };
  es.onerror = () => { if (!${recover}) es.close(); };
  (async function poll(since) {
    try { const value = await (await fetch('/poll?since=' + since)).text(); state.poll = value; poll(value); }
    catch { if (${recover}) setTimeout(() => poll(since), 300); }
  })('0');
  (async function stream() {
    try {
      const reader = (await fetch('/stream')).body.getReader(), decoder = new TextDecoder();
      for (;;) { const { value, done } = await reader.read(); if (done) break;
        const text = decoder.decode(value).trim().split(/\\s+/).pop(); if (text) state.stream = text; }
    } catch {}
    if (${recover}) setTimeout(stream, 300);
  })();
</script>`;

for (const recover of [true, false]) {
  test(`going offline cuts event streams, long polls and streamed fetches (${recover ? 'recovering' : 'no recovery'} client)`, async () => {
    const app = await fixture(streamingPage(recover));
    const browser = await chromium.launch({ headless: true });
    let interruption: NetworkInterruption | undefined;
    try {
      const { page, networkInterruption, setOffline } = await interruptibleActor(browser, app.url);
      interruption = networkInterruption;
      await page.waitForTimeout(500);
      const loadedAt = await windowValue(page, 'loadedAt');
      const offline = await setOffline(true);
      assert.equal(offline.status, 'passed', offline.summary ?? '');
      app.set('2');
      await page.waitForTimeout(800);
      assert.deepEqual(await windowValue(page, 'state'), { sse: '1', poll: '1', stream: '1' }, 'nothing is delivered during the cut');
      await setOffline(false, 3000);
      assert.deepEqual(await windowValue(page, 'state'), recover
        ? { sse: '2', poll: '2', stream: '2' } : { sse: '1', poll: '1', stream: '1' });
      assert.equal(await windowValue(page, 'loadedAt'), loadedAt, 'the page was not reloaded');
    } finally {
      await interruption?.dispose();
      await browser.close();
      app.close();
    }
  });
}

test('a root/token app socket is cut, and an unproven cut is unmeasured', async () => {
  const app = await fixture(`<script>
    window.received = []; window.closes = 0;
    const ws = new WebSocket('ws://' + location.host + '/?token=application-session');
    ws.onmessage = event => window.received.push(event.data);
    ws.onclose = () => window.closes++;
  </script>`);
  const browser = await chromium.launch({ headless: true });
  const interruptions: NetworkInterruption[] = [];
  try {
    const cut = await interruptibleActor(browser, app.url);
    interruptions.push(cut.networkInterruption);
    const offline = await cut.setOffline(true);
    assert.equal(offline.status, 'passed', offline.summary ?? '');
    app.send('stock:52');
    await cut.setOffline(false);
    assert.deepEqual(await windowValue(cut.page, 'received'), []);
    assert.equal(await windowValue(cut.page, 'closes'), 1, 'an app without reconnect code stays disconnected');

    // A context whose traffic does not pass through the proxy keeps its socket: unmeasured.
    const bypassed = await interruptibleActor(browser, app.url, false);
    interruptions.push(bypassed.networkInterruption);
    assert.equal((await bypassed.setOffline(true, 0)).status, 'inconclusive');

    // An actor opened without an interruption cannot prove one.
    const plain = await executeAction(ACTION_REGISTRY, 'setOffline', { do: 'setOffline', actor: 'buyer', settleMs: 0 },
      { capabilities: { actors: { get: () => ({ page: cut.page, loc: () => { throw new Error('unused'); } }) },
        'browser-interaction': { defaultWithin: 1000, sleep: async () => {} } } });
    assert.equal(plain.status, 'inconclusive');
  } finally {
    for (const interruption of interruptions) await interruption.dispose();
    await browser.close();
    app.close();
  }
});

test('a disposed interruption releases its listener and cannot report a cut', async () => {
  const interruption = await startNetworkInterruption();
  const port = Number(new URL(interruption.proxy.server).port);
  await interruption.dispose();
  const probe = await new Promise<string>(resolve => {
    const socket = connect(port, '127.0.0.1');
    socket.on('connect', () => { socket.destroy(); resolve('open'); });
    socket.on('error', error => resolve((error as NodeJS.ErrnoException).code ?? 'error'));
  });
  assert.equal(probe, 'ECONNREFUSED');
  await assert.rejects(interruption.interrupt(), /stopped/);
  await assert.rejects(interruption.restore(), /stopped/);
});

test('an unleased appliance browser, such as the null control, gets a local proxy', async () => {
  const prior = { appliance: process.env.STACK_BENCH_APPLIANCE, lease: process.env.STACK_BENCH_LEASE };
  process.env.STACK_BENCH_APPLIANCE = '1';
  delete process.env.STACK_BENCH_LEASE;
  try {
    const interruption = await startNetworkInterruption();
    assert.match(interruption.proxy.server, /^http:\/\/127\.0\.0\.1:\d+$/);
    await interruption.dispose();
  } finally {
    if (prior.appliance === undefined) delete process.env.STACK_BENCH_APPLIANCE;
    else process.env.STACK_BENCH_APPLIANCE = prior.appliance;
    if (prior.lease !== undefined) process.env.STACK_BENCH_LEASE = prior.lease;
  }
});
