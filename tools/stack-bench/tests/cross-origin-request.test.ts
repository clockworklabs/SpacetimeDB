import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import test from 'node:test';
import { chromium } from 'playwright';
import { crossOriginPost } from '../src/actions/cross-origin-request.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';
import { isFinding } from '../src/actions/action-findings.js';
import { harnessBrowserFailure } from '../src/evidence/harness-errors.js';

test('real browser origin probes expose cookie writes despite opaque responses and keep missing responses unmeasured', async () => {
  let protectOrigin = true, writes = 0;
  const observed: { origin?: string; cookie?: string; authorization?: string }[] = [];
  const server = createServer((req, res) => {
    observed.push({ origin: req.headers.origin, cookie: req.headers.cookie,
      authorization: req.headers.authorization });
    if (req.url === '/lost') { req.socket.destroy(); return; }
    if (req.method !== 'POST' || req.headers.cookie !== 'sid=private-session'
      || (protectOrigin && req.headers.origin !== `http://${req.headers.host}`)) {
      res.writeHead(403, { 'Content-Type': 'application/json' }).end('{"error":"refused"}'); return;
    }
    writes++; res.writeHead(200, { 'Content-Type': 'application/json' }).end('{"ok":true}');
  }).listen(0, '0.0.0.0');
  await once(server, 'listening');
  const port = (server.address() as { port: number }).port;
  const url = `http://127.0.0.1:${port}/buy`;
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    await context.addCookies([{ name: 'sid', value: 'private-session', url,
      httpOnly: true, sameSite: 'Lax' }]);
    const signal = AbortSignal.timeout(15_000);
    const safe = await crossOriginPost(context, { url }, 'same-site', signal);
    assert.equal(safe.cookieSent, true); assert.equal(safe.responseStatus, 403); assert.equal(writes, 0);
    protectOrigin = false;
    const unsafe = await crossOriginPost(context, { url }, 'same-site', signal);
    assert.equal(unsafe.cookieSent, true); assert.equal(unsafe.responseStatus, 200);
    assert.equal(unsafe.browserResponseType, 'opaque'); assert.equal(writes, 1);
    const crossSite = await crossOriginPost(context, { url }, 'cross-site', signal);
    assert.equal(crossSite.cookieSent, false); assert.equal(writes, 1);
    await assert.rejects(crossOriginPost(context, { url: url.replace('/buy', '/lost') }, 'same-site', signal),
      (error: unknown) => error instanceof ActionInconclusive
        && isFinding(error.details.finding) && error.details.finding.kind === 'replay-unavailable'
        && /complete browser request and response/.test(String(error.details.finding.fields.detail)));
    assert.equal(context.pages().length, 0);
    assert(observed.every(r => r.authorization === undefined));
    assert(!JSON.stringify({ safe, unsafe, crossSite }).includes('private-session'));
    await context.close();
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('a timeout loading the harness-served origin page is a harness failure', async () => {
  const timeout = Object.assign(new Error('page.goto: Timeout 10000ms exceeded.'), { name: 'TimeoutError' });
  const page = { on() {}, close: async () => {}, route: async () => {}, goto: async () => { throw timeout; } };
  const context = { newPage: async () => page, grantPermissions: async () => {} } as never;
  await assert.rejects(crossOriginPost(context, { url: 'http://127.0.0.1:18081/api' }, 'same-site',
    new AbortController().signal), error => harnessBrowserFailure(error) !== null);
});
