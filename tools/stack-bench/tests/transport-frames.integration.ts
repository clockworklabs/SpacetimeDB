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
