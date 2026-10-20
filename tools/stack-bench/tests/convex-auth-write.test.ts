import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium, type Page } from 'playwright';
import { withAuthRequestPatch, withAuthWriteInventory } from '../src/actions/auth-request-patch.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { convexAdapter } from '../src/stacks/backends/convex-adapter.js';

// The SDK refreshes a token after signup. Both native actions must be probed;
// the second has no typed username or password to locate its argument object.
test('leased Convex signup and refresh actions each receive one authority probe', async () => {
  let vulnerable = false, stock = 0;
  const accounts = new Map<string, string>();
  const calls: { outerRole: unknown; paramsRole: unknown; argsRole: unknown }[] = [];
  const serve = () => createServer(async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Headers', 'content-type');
    if (req.method === 'OPTIONS') { res.writeHead(204).end(); return; }
    if (req.method !== 'POST') { res.end('<body>signup</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += String(chunk);
    const body = JSON.parse(raw);
    if (req.url === '/restock') {
      if (accounts.get(body.user) !== 'admin') { res.writeHead(403).end(); return; }
      stock++; res.end('ok'); return;
    }
    const args = body.args[0];
    calls.push({ outerRole: body.role, paramsRole: args.params?.role, argsRole: args.role });
    if (args.params) accounts.set(args.params.username, 'customer');
    else if (args.refreshToken && vulnerable && args.role === 'admin') accounts.set(args.refreshToken, 'admin');
    res.setHeader('content-type', 'application/json'); res.end('{"ok":true}');
  }).listen(0, '127.0.0.1');
  const backend = serve(), app = serve();
  await Promise.all([once(backend, 'listening'), once(app, 'listening')]);
  const origin = (server: typeof backend) => `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const lease = createBackendLease({ runId: 'convex-auth-writes', backend: 'convex', track: 'ecommerce',
    runIndex: 0, serverUri: origin(backend) });
  lease.state = 'active';
  const patch = convexAdapter.grading.authRequestPatch?.(lease);
  const browser = await chromium.launch({ headless: true });
  const fresh = async () => { const context = await browser.newContext(); const page = await context.newPage();
    await page.goto(origin(app)); return page; };
  const submit = (page: Page, user: string, url = `${origin(backend)}/api/action`) => page.evaluate(async ({ user, url }) => {
    const post = async (args: unknown) => { const response = await fetch(url, { method: 'POST',
      headers: { 'content-type': 'application/json' }, body: JSON.stringify({ path: 'auth:signIn',
        format: 'convex_encoded_json', args: [args] }) }); await response.text(); };
    await post({ provider: 'password', params: { username: user, password: 'fixture-password', flow: 'signUp' } });
    await post({ refreshToken: user });
  }, { user, url });
  try {
    const baselinePage = await fresh();
    const baseline = await withAuthWriteInventory(baselinePage, () => submit(baselinePage, 'ordinary'));
    assert.equal(baseline.writes.length, 2);
    await baselinePage.context().close();
    for (vulnerable of [false, true]) for (const index of [0, 1]) {
      const page = await fresh(), user = `probe-${vulnerable}-${index}`, before = stock;
      calls.length = 0;
      const result = await withAuthRequestPatch(page, user, 'fixture-password', { fields: { role: 'admin' } },
        () => submit(page, user), patch, 'signup', { writes: baseline.writes, index });
      assert.equal(result.requestPatch.status, 200);
      assert.equal(result.requestPatch.shape, index === 0 ? 'object' : 'convex-args-object');
      assert.deepEqual(calls, index === 0 ? [
        { outerRole: undefined, paramsRole: 'admin', argsRole: undefined },
        { outerRole: undefined, paramsRole: undefined, argsRole: undefined },
      ] : [
        { outerRole: undefined, paramsRole: undefined, argsRole: undefined },
        { outerRole: undefined, paramsRole: undefined, argsRole: 'admin' },
      ]);
      const status = await page.evaluate(async ({ url, user }) => (await fetch(url+'/restock', {
        method: 'POST', body: JSON.stringify({ user }) })).status, { url: origin(backend), user });
      assert.equal(status, vulnerable && index === 1 ? 200 : 403);
      assert.equal(stock - before, vulnerable && index === 1 ? 1 : 0);
      await page.context().close();
    }
    // An app route or look-alike URL cannot inherit the leased native envelope.
    for (const url of [`${origin(app)}/api/action`, `${origin(backend)}/api/action?proxy=1`,
      `${origin(backend)}/nested/api/action`]) {
      const page = await fresh();
      const baseline = await withAuthWriteInventory(page, () => submit(page, 'ordinary-proxy', url));
      assert.equal(baseline.writes.length, 2);
      await assert.rejects(() => withAuthRequestPatch(page, 'proxy', 'fixture-password', { fields: { role: 'admin' } },
        () => submit(page, 'proxy', url), patch, 'signup', { writes: baseline.writes, index: 1 }), ActionInconclusive);
      await page.context().close();
    }
  } finally {
    await browser.close();
    await Promise.all([backend, app].map(server => new Promise<void>(resolve => server.close(() => resolve()))));
  }
});
