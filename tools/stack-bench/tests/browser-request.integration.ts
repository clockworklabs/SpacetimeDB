import assert from 'node:assert/strict';
import { once } from 'node:events';
import { writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium, type APIRequestContext } from 'playwright';
import { ActionInconclusive } from '../src/actions/action-contract.js';
import type { Finding } from '../src/actions/action-findings.js';
import { registerBrowserRequest, withBrowserRequest } from '../src/actions/browser-request.js';

function unavailable(error: unknown, reason: RegExp): boolean {
  assert(error instanceof ActionInconclusive);
  const finding = error.details.finding as Finding;
  assert(finding.kind === 'replay-unavailable');
  assert.match(String(finding.fields.detail), reason);
  return true;
}

// Failure cases: a browser-only proxy breaks controller API calls; an old cookie
// snapshot uses the wrong account; copying the whole jar erases concurrent cookies;
// rotation/deletion or explicit forged headers change replay semantics; failed
// callbacks leak request contexts. Evaluator replies must stay outside page capture.
test('evaluator requests preserve actor cookies without the browser-only proxy', async t => {
  const requests: { path: string; cookie: string; userAgent: string }[] = [];
  const server = createServer((req, res) => {
    requests.push({ path: req.url ?? '', cookie: req.headers.cookie ?? '', userAgent: req.headers['user-agent'] ?? '' });
    res.setHeader('Content-Type', 'application/json');
    if (req.url === '/reset') { req.socket.destroy(); return; }
    if (req.url === '/rotate') res.setHeader('Set-Cookie', [
      'session=rotated; Path=/; HttpOnly; SameSite=Lax', 'remove=; Path=/; Max-Age=0',
    ]);
    if (req.url === '/route-cookie' || req.url === '/drop-cookie') {
      res.setHeader('Set-Cookie', 'route-session=accepted; Path=/; Max-Age=60; HttpOnly; SameSite=Lax');
    }
    if (req.url === '/body') {
      res.write('{"value":');
      setTimeout(() => res.end('"complete-evaluator-echo"}'), 40);
      return;
    }
    res.end(JSON.stringify({ cookie: req.headers.cookie ?? '', userAgent: req.headers['user-agent'] ?? '' }));
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true, executablePath: chromium.executablePath() });
  t.after(() => browser.close());
  const a = await browser.newContext({ proxy: { server: 'http://127.0.0.1:1', bypass: '<-loopback>' } });
  const b = await browser.newContext({ proxy: { server: 'http://127.0.0.1:1', bypass: '<-loopback>' } });
  const ordinary = await browser.newContext();
  t.after(async () => { await Promise.all([a.close(), b.close(), ordinary.close()]); });
  const page = await a.newPage();
  const userAgent = await page.evaluate(() => navigator.userAgent);
  let offline = false;
  registerBrowserRequest(a, userAgent, () => offline);
  registerBrowserRequest(b, userAgent);
  let pageResponses = 0;
  page.on('response', () => pageResponses++);
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_BROWSER_REQUEST_EVIDENCE) writeFileSync(process.env.STACK_BENCH_BROWSER_REQUEST_EVIDENCE,
      JSON.stringify({ browser: browser.version(), evidence, requests, pageResponses }, null, 2));
  });
  await t.test('controller request avoids unreachable browser proxy and consumes the full body', async () => {
    await assert.rejects(a.request.get(`${url}/body`, { timeout: 500 }), /ECONNREFUSED|proxy|timeout/i);
    let used: APIRequestContext | undefined;
    const result = await withBrowserRequest(a.request, async api => {
      used = api;
      const response = await api.get(`${url}/body`, { timeout: 1000 });
      assert.equal(response.status(), 200);
      return response.json();
    });
    assert.deepEqual(result, { value: 'complete-evaluator-echo' });
    assert(used);
    await assert.rejects(used.get(`${url}/body`, { timeout: 1000 }), /disposed|closed/i);
    assert.equal(pageResponses, 0, 'Evaluator echoes must not enter browser privacy evidence');
    assert.equal(requests.at(-1)?.userAgent, userAgent);
    evidence.push({ case: 'full-body', result, pageResponses });
  });
  await t.test('current cookies rotate and delete without losing unrelated concurrent cookies', async () => {
    await a.addCookies([{ name: 'session', value: 'initial', url }, { name: 'remove', value: 'old', url }]);
    const result = await withBrowserRequest(a.request, async api => {
      const response = await api.get(`${url}/rotate`, { timeout: 1000 });
      const body = await response.json();
      await a.addCookies([{ name: 'concurrent', value: 'keep', url }]);
      return body;
    });
    assert.match(result.cookie, /(?:^|; )session=initial(?:;|$)/);
    const cookies = await a.cookies(url);
    assert.equal(cookies.find(c => c.name === 'session')?.value, 'rotated');
    assert.equal(cookies.find(c => c.name === 'remove'), undefined);
    assert.equal(cookies.find(c => c.name === 'concurrent')?.value, 'keep');
    const later = await withBrowserRequest(a.request, async api => (await api.get(`${url}/echo`, { timeout: 1000 })).json());
    assert.match(later.cookie, /session=rotated/);
    evidence.push({ case: 'cookie-updates', result, cookies, later });
  });
  await t.test('another actor stays isolated and explicit forged Cookie is preserved', async () => {
    const isolated = await withBrowserRequest(b.request, async api => (await api.get(`${url}/echo`, { timeout: 1000 })).json());
    assert.equal(isolated.cookie, '');
    const forged = await withBrowserRequest(a.request, async api => (await api.get(`${url}/echo`, {
      timeout: 1000, headers: { Cookie: 'session=forged' }, maxRedirects: 0, maxRetries: 0,
    })).json());
    assert.equal(forged.cookie, 'session=forged');
    assert.equal((await a.cookies(url)).find(c => c.name === 'session')?.value, 'rotated');
    evidence.push({ case: 'isolation', isolated, forged });
  });
  await t.test('partitioned cookies are not flattened into an evaluator cookie jar', async () => {
    await b.addCookies([{ name: 'partitioned', value: 'private', domain: '127.0.0.1', path: '/',
      secure: true, sameSite: 'None', partitionKey: 'https://example.test' }]);
    assert((await b.cookies()).some(cookie => cookie.partitionKey), 'The real browser must hold a partitioned cookie');
    let entered = false;
    await assert.rejects(withBrowserRequest(b.request, async () => { entered = true; }),
      error => unavailable(error, /partition/i));
    assert.equal(entered, false, 'Unsupported cookie identity must fail before sending a replay');
  });
  await t.test('a concurrent change to the same cookie is not overwritten', async () => {
    await a.addCookies([{ name: 'session', value: 'before-race', url }]);
    await assert.rejects(withBrowserRequest(a.request, async api => {
      await (await api.get(`${url}/rotate`, { timeout: 1000 })).text();
      await a.addCookies([{ name: 'session', value: 'newer-browser-session', url }]);
    }), error => unavailable(error, /cookie.*chang|concurrent|reconcil/i));
    assert.equal((await a.cookies(url)).find(c => c.name === 'session')?.value, 'newer-browser-session');
  });
  await t.test('unregistered contexts use their original request unchanged', async () => {
    await withBrowserRequest(ordinary.request, async api => {
      assert.equal(api, ordinary.request);
      assert.equal((await api.get(`${url}/echo`, { timeout: 1000 })).status(), 200);
    });
  });
  await t.test('an active network cut cannot be bypassed by evaluator requests', async () => {
    const before = requests.length;
    let entered = false;
    offline = true;
    try {
      await assert.rejects(withBrowserRequest(a.request, async api => {
        entered = true;
        await api.get(`${url}/echo`, { timeout: 1000 });
      }), error => unavailable(error, /offline|network.*cut/i));
      assert.equal(entered, false);
      assert.equal(requests.length, before);
    } finally { offline = false; }
  });
  await t.test('callback and network failures dispose their temporary request context', async () => {
    for (const network of [false, true]) {
      let used: APIRequestContext | undefined;
      await assert.rejects(withBrowserRequest(a.request, async api => {
        used = api;
        if (network) await api.get(`${url}/reset`, { maxRetries: 0, timeout: 1000 });
        throw new Error('fixture callback failed');
      }), network ? /socket|reset|closed/i : /fixture callback failed/);
      assert(used);
      await assert.rejects(used.get(`${url}/echo`, { timeout: 1000 }), /disposed|closed/i);
    }
  });
  await t.test('a fulfilled response lets the browser apply relative-expiry cookies once', async () => {
    const context = await browser.newContext();
    try {
      registerBrowserRequest(context, userAgent);
      const routed = await context.newPage();
      let routeError: unknown;
      await context.route(`${url}/route-cookie`, async route => {
        try {
          await withBrowserRequest(context.request, async api => {
            const response = await api.fetch(route.request(), { timeout: 1000 });
            const body = await response.body();
            await new Promise(resolve => setTimeout(resolve, 1200));
            await route.fulfill({ status: response.status(), headers: response.headers(), body });
            await routed.waitForLoadState('domcontentloaded');
          }, false);
        } catch (error) { routeError = error; }
      });
      await routed.goto(`${url}/route-cookie`, { waitUntil: 'domcontentloaded', timeout: 5000 });
      await context.unrouteAll({ behavior: 'wait' });
      assert.equal(routeError, undefined);
      const cookies = await context.cookies(url);
      assert.equal(cookies.find(cookie => cookie.name === 'route-session')?.value, 'accepted');
      evidence.push({ case: 'fulfilled-cookie', cookies });
    } finally { await context.close(); }
  });
  await t.test('a dropped routed response does not apply evaluator response cookies', async () => {
    const context = await browser.newContext();
    try {
      registerBrowserRequest(context, userAgent);
      const routed = await context.newPage();
      let routeError: unknown;
      await context.route(`${url}/drop-cookie`, async route => {
        try {
          await withBrowserRequest(context.request, async api => {
            await (await api.fetch(route.request(), { timeout: 1000 })).body();
            await route.abort('failed');
          }, false);
        } catch (error) { routeError = error; }
      });
      await assert.rejects(routed.goto(`${url}/drop-cookie`, { timeout: 5000 }), /ERR_FAILED|failed/i);
      await context.unrouteAll({ behavior: 'wait' });
      assert.equal(routeError, undefined);
      assert.equal((await context.cookies(url)).find(cookie => cookie.name === 'route-session'), undefined);
    } finally { await context.close(); }
  });
});
