import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { captureResponses, ReceivedTransport } from '../grader/transport-frames.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';

test('HTML and live SSE leaks are captured; an unsupported stream cannot pass', async t => {
  const server = createServer((req, res) => {
    if (req.url === '/events') {
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.write('data: private-canary\n\n');
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end('<!doctype html><title>transport control</title><div>private-html-canary</div>');
    }
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address();
  assert(address && typeof address !== 'string');
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  const received = new ReceivedTransport();
  await captureResponses(page, received);
  await page.goto(`http://127.0.0.1:${address.port}`);
  for (let n = 0; n < 100 && !received.contains('private-html-canary', false); n++) await page.waitForTimeout(20);
  assert.equal(received.contains('private-html-canary'), true);
  await page.evaluate(() => new Promise<void>(resolve => {
    const events = new EventSource('/events');
    events.onmessage = () => resolve();
  }));
  // CDP and page events use separate channels. Wait for the actual retained evidence.
  for (let n = 0; n < 100 && !received.contains('private-canary', false); n++) await page.waitForTimeout(20);
  assert.equal(received.contains('private-canary'), true);
  assert.equal(received.contains('not-sent'), false);
  await page.evaluate(async () => { await fetch('/events'); });
  for (let n = 0; n < 100 && !received.incomplete; n++) await page.waitForTimeout(20);
  assert.throws(() => received.contains('not-sent'), ActionInconclusive);
});

test('HTTP privacy capture survives immediate app reload; disabled durability loses evidence', async t => {
  const server = createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://fixture.local');
    res.setHeader('Cache-Control', 'no-store');
    if (url.pathname === '/json') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ private: `private-marker-${url.searchParams.get('sample')}` }));
    } else if (url.pathname === '/empty') {
      res.writeHead(204, { 'Content-Type': 'application/json' });
      res.end();
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end('<!doctype html><title>reload capture control</title>');
    }
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address();
  assert(address && typeof address !== 'string');
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  for (const durable of [true, false]) {
    for (const kind of ['json', 'empty']) {
      await t.test(`${kind}, durable=${durable}`, async () => {
        const context = await browser.newContext();
        try {
          if (!durable) {
            // Negative control changes only the native capture setting, never the app's timing.
            const createSession = context.newCDPSession.bind(context);
            context.newCDPSession = async target => {
              const session = await createSession(target);
              const send = session.send.bind(session);
              session.send = ((method, params) => method === 'Network.configureDurableMessages'
                ? Promise.resolve({}) : send(method, params)) as typeof session.send;
              return session;
            };
          }
          const page = await context.newPage();
          const received = new ReceivedTransport();
          await captureResponses(page, received);
          await page.goto(`http://127.0.0.1:${address.port}`);
          for (let sample = 0; sample < 10; sample++) {
            const navigated = page.waitForEvent('framenavigated');
            await page.evaluate(({ kind, sample }) => {
              // Launch the app action without tying the evaluation reply to the destroyed realm.
              void (async () => {
                const response = await fetch(`/${kind}?sample=${sample}`, { method: 'POST' });
                if (kind === 'json') await response.json();
                else await response.text();
                location.reload();
              })();
            }, { kind, sample });
            await navigated;
            await page.waitForLoadState('load');
            for (let n = 0; n < 100 && received.pending; n++) await page.waitForTimeout(10);
            if (durable && kind === 'json') assert.equal(received.contains(`private-marker-${sample}`), true);
          }
          if (durable) assert.equal(received.contains('never-delivered'), false);
          else assert.throws(() => received.contains('never-delivered'), error => {
            assert(error instanceof ActionInconclusive);
            assert.match(error.message, /body-read failures: [1-9]/);
            return true;
          });
        } finally { await context.close(); }
      });
    }
  }
});
