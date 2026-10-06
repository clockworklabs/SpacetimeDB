import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { connect } from 'node:net';
import type { Socket } from 'node:net';
import test from 'node:test';
import { writeFileSync } from 'node:fs';
import { gzipSync, deflateSync, brotliCompressSync } from 'node:zlib';
import { chromium } from 'playwright';
import type { Browser, Page } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { startNetworkInterruption } from '../src/actions/network-interruption.js';
import type { NetworkInterruption } from '../src/actions/network-interruption.js';
import { startNetworkProxy } from '../container/browser-network-proxy.js';

// A fixture app server: its own page, WebSocket upgrades, and a live value it can push.
async function fixture(page: string, http: (req: IncomingMessage, res: ServerResponse, value: () => string) => boolean = () => false,
  wire?: (chunk: Buffer) => void, viteDelayMs = 0) {
  let value = '1';
  const sockets = new Set<Socket>(), streams = new Set<() => void>(), waiters = new Set<() => void>();
  const server = createServer((req, res) => {
    const path = new URL(req.url!, 'http://fixture').pathname;
    if (path === '/@vite/client') {
      setTimeout(() => res.writeHead(200, { 'Content-Type': 'text/javascript' }).end('const wsToken = "dev";'), viteDelayMs);
      return;
    }
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
  if (wire) server.on('connection', socket => {
    let application = false;
    socket.on('data', chunk => {
      if (chunk.toString('latin1').startsWith('GET /sock')) application = true;
      if (application) wire(chunk);
    });
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
  const page = await context.newPage();
  await networkInterruption.attach(context, page);
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

// HTTP observation must retain unread/split bytes without changing forwarding.
// The receipt is proxy-owned; compressed failures and bounded-storage overflow
// cannot certify absence. Harness API replay tunnels must remain excluded.
test('HTTP proxy snapshots preserve delivered bodies and exclude replay tunnels', async t => {
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_PROXY_HTTP_EVIDENCE) writeFileSync(process.env.STACK_BENCH_PROXY_HTTP_EVIDENCE,
      JSON.stringify({ node: process.version, evidence }, null, 2));
  });
  for (const encoding of ['identity', 'gzip', 'deflate', 'br', 'unknown', 'corrupt', 'overflow']) {
    await t.test(encoding, async () => {
      type Record = { id: string; url: string; method: string; status: number; contentType: string; body: string; complete: boolean };
      type Reply = { id?: number; ok?: boolean; port?: number; records?: Record[]; incomplete?: boolean; pending?: number };
      const waiting = new Map<number, { resolve: (reply: Reply) => void; reject: (error: Error) => void }>();
      let next = 0, stopped = false;
      const handle = startNetworkProxy(reply => {
        const result = reply as Reply;
        waiting.get(result.id ?? -1)?.resolve(result); waiting.delete(result.id ?? -1);
      }, code => { stopped = true; for (const value of waiting.values()) value.reject(new Error(`proxy exited ${code}`)); waiting.clear(); });
      const command = (cmd: string, fields = {}) => new Promise<Reply>((resolve, reject) => {
        const id = ++next; waiting.set(id, { resolve, reject }); handle(JSON.stringify({ id, cmd, ...fields }));
      });
      const marker = 'private-🔒-marker';
      const plain = Buffer.from(JSON.stringify({ value: marker }));
      const body = encoding === 'gzip' ? gzipSync(plain) : encoding === 'deflate' ? deflateSync(plain)
        : encoding === 'br' ? brotliCompressSync(plain) : encoding === 'overflow' ? Buffer.alloc(8 * 1024 * 1024 + 1, 120) : plain;
      let complete: (() => void) | undefined;
      const server = createServer((req, res) => {
        if (req.url === '/') { res.end('<title>observer</title>'); return; }
        if (req.url === '/favicon.ico') { res.writeHead(204).end(); return; }
        if (req.url === '/preheaders') { complete = () => res.end('late body'); return; }
        if (req.url === '/failed') { req.socket.destroy(); return; }
        if (req.url === '/refused') { res.writeHead(403).end('refused'); return; }
        if (req.url === '/duplicate') {
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ value: 'public-repeat-'.repeat(160000) })); return;
        }
        res.writeHead(200, { 'Content-Type': 'application/json', 'x-stack-bench-capture': 'upstream-forgery',
          ...(encoding !== 'identity' && encoding !== 'overflow' ? { 'Content-Encoding': encoding === 'corrupt' ? 'gzip' : encoding } : {}) });
        const responseBody = req.url === '/cancel' ? Buffer.from('{"value":"cancel-private-prefix"}') : body;
        const split = encoding === 'identity' ? plain.indexOf(Buffer.from('🔒')) + 2 : Math.floor(responseBody.length / 2);
        res.write(responseBody.subarray(0, split));
        setTimeout(() => { if (!res.destroyed) res.write(responseBody.subarray(split)); }, 10);
        complete = () => res.end();
      }).listen(0, '127.0.0.1');
      await once(server, 'listening');
      const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
      const configured = await command('config', { user: 'fixture', pass: 'synthetic', observeHttp: true });
      const browser = await chromium.launch({ headless: true, executablePath: chromium.executablePath() });
      const context = await browser.newContext({ proxy: { server: `http://127.0.0.1:${configured.port}`, username: 'fixture', password: 'synthetic', bypass: '<-loopback>' } });
      try {
        const page = await context.newPage(); await page.goto(url);
        const response = page.waitForResponse(`${url}/data`);
        await page.evaluate(consume => { void fetch('/data').then(value => { Reflect.set(window, 'heldResponse', value); if (consume) return value.text(); }).catch(() => {}); }, encoding === 'overflow');
        const received = await response;
        await page.waitForTimeout(50);
        const partial = await command('httpSnapshot');
        assert(complete); complete();
        if (encoding === 'overflow') await received.finished();
        await page.waitForTimeout(50);
        const finished = await command('httpSnapshot');
        evidence.push({ encoding, responseReceipt: received.headers()['x-stack-bench-capture'], partial, finished });
        assert.equal(finished.pending, 0, 'Completed delivery releases the pending request');
        if (['unknown', 'corrupt', 'overflow'].includes(encoding)) {
          assert.equal(finished.incomplete, true, 'Unsupported, corrupt or lost bytes cannot establish absence');
        } else {
          assert.equal(partial.pending, 1, 'A live partial body still belongs to the navigation drain');
          const first = partial.records?.find(record => record.url === `${url}/data`);
          assert(first, 'The response must have a snapshot record');
          assert.equal(first.id, received.headers()['x-stack-bench-capture']);
          assert.notEqual(first.id, 'upstream-forgery');
          assert.equal(first.body, plain.toString(), 'Unread response bytes and split UTF-8 must remain intact');
          assert.equal(first.complete, false, 'A prefix is not a complete response receipt');
          assert.equal(partial.incomplete, false);
          const final = finished.records?.find(record => record.id === first.id);
          assert.equal(final?.complete, true);
          assert.equal(final?.body, plain.toString());
          assert.equal(finished.incomplete, false);
          if (encoding === 'identity') {
            // Drain state comes from the actual client connection, including
            // requests with no headers and cancellation without a body end.
            const preheadersStarted = once(server, 'request');
            await page.evaluate(() => { void fetch('/preheaders').catch(() => {}); });
            await preheadersStarted;
            const preheaders = await command('httpSnapshot');
            assert.equal(preheaders.pending, 1);
            assert(!preheaders.records?.some(record => record.url === `${url}/preheaders`));
            await page.reload();
            await page.waitForTimeout(30);
            const canceledHeaders = await command('httpSnapshot');
            assert.equal(canceledHeaders.pending, 0);
            const cancelResponse = page.waitForResponse(`${url}/cancel`);
            await page.evaluate(() => { void fetch('/cancel').then(r => r.text()).catch(() => {}); });
            await cancelResponse;
            await page.waitForTimeout(30);
            assert.equal((await command('httpSnapshot')).pending, 1);
            await page.reload();
            await page.waitForTimeout(30);
            const canceledBody = await command('httpSnapshot');
            assert.equal(canceledBody.pending, 0);
            const prefix = canceledBody.records?.find(record => record.url === `${url}/cancel`);
            assert(prefix);
            assert.equal(prefix.complete, false, 'Cancellation must never certify a complete response');
            assert(prefix.body.includes('cancel-private-prefix'), 'Cancellation retains delivered prefix bytes');
            await page.evaluate(() => fetch('/refused').then(r => r.text()));
            assert.equal((await command('httpSnapshot')).pending, 0);
            await page.evaluate(() => fetch('/failed').catch(() => {}));
            const failed = await command('httpSnapshot');
            assert.equal(failed.pending, 0, 'Forwarding errors release the request');
            evidence.push({ preheaders, canceledHeaders, canceledBody, failed });
            // Repeated completed bundles retain one body without losing receipts.
            for (let n = 0; n < 5; n++) await page.evaluate(() => fetch('/duplicate').then(r => r.text()));
            const duplicates = await command('httpSnapshot');
            const repeated = duplicates.records?.filter(record => record.url === `${url}/duplicate`);
            assert.equal(repeated?.length, 5);
            assert.equal(repeated?.filter(record => record.body.includes('public-repeat-')).length, 1);
            assert.equal(duplicates.incomplete, false);
            const replayStarted = once(server, 'request');
            const replay = context.request.get(`${url}/replay`);
            await replayStarted; complete();
            assert.equal((await replay).status(), 200);
            const afterReplay = await command('httpSnapshot');
            assert(!afterReplay.records?.some(record => record.url === `${url}/replay`), 'APIRequestContext CONNECT is not browser HTTP delivery');
          }
        }
      } catch (error) { evidence.push({ encoding, error: error instanceof Error ? error.message : String(error) }); throw error; }
      finally {
        await context.close(); await browser.close();
        if (!stopped) await command('dispose');
        server.closeAllConnections(); server.close();
      }
    });
  }
});

test('split WebSocket request bytes pass through while the Vite token is checked', async () => {
  let received = '';
  const app = await fixture('', undefined, chunk => { received += chunk.toString('latin1'); }, 300);
  const interruption = await startNetworkInterruption();
  try {
    const authority = new URL(app.url).host;
    const credentials = Buffer.from(`${interruption.proxy.username}:${interruption.proxy.password}`).toString('base64');
    const first = 'GET /socket?token=dev HTTP/1.1\r\n';
    const rest = `Host: ${authority}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nX-Split-Probe: must-arrive\r\n\r\n`;
    for (const chunks of [[first.slice(0, 9), first.slice(9), rest], [first, rest]]) {
      received = '';
      const client = connect(Number(new URL(interruption.proxy.server).port), '127.0.0.1');
      try {
        await once(client, 'connect');
        const connected = once(client, 'data');
        client.write(`CONNECT ${authority} HTTP/1.1\r\nHost: ${authority}\r\nProxy-Authorization: Basic ${credentials}\r\n\r\n`);
        await connected;
        for (const chunk of chunks) {
          client.write(chunk);
          await new Promise(resolve => setTimeout(resolve, 75));
        }
        await new Promise(resolve => setTimeout(resolve, 300));
        assert.equal(received, first + rest);
        await interruption.interrupt();
        await new Promise(resolve => setTimeout(resolve, 50));
        assert.equal(client.destroyed, false, 'the split Vite token must keep its tooling socket open');
        await interruption.restore();
      } finally { client.destroy(); }
    }
  } finally {
    await interruption.dispose();
    app.close();
  }
});

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
    assert.equal(await bypassed.page.evaluate(() => fetch('/ping').then(response => response.ok)), true,
      'another actor keeps network access');
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

test('same-document navigation keeps a live socket tracked until the document is replaced', async () => {
  const app = await fixture(`<script>window.ws = new WebSocket('ws://' + location.host + '/socket');</script>`);
  const browser = await chromium.launch({ headless: true });
  let interruption: NetworkInterruption | undefined;
  try {
    const actor = await interruptibleActor(browser, app.url, false);
    interruption = actor.networkInterruption;
    await actor.page.waitForFunction(() => (window as unknown as { ws: WebSocket }).ws.readyState === WebSocket.OPEN);
    await actor.page.evaluate(() => { history.pushState({}, '', '/next'); location.hash = 'later'; });
    const during = await interruption.interrupt();
    assert.equal(during.open.length, 1, 'the live socket remains visible after same-document navigation');
    await interruption.restore();
    await actor.page.goto('data:text/html,replaced');
    const replaced = await interruption.interrupt();
    assert.deepEqual(replaced.open, [], 'the replaced document does not leave a stale socket');
  } finally {
    await interruption?.dispose();
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
