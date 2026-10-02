import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { installAuthWebSocketCapture, patchAuthRequest, withAuthRequestPatch, withAuthWriteInventory, withWriteCompletion,
  withAuthWriteTarget, stopAuthWriteInventory, type AuthWrite } from '../src/actions/auth-request-patch.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { installSpacetimeWriteCapture } from '../src/stacks/backends/spacetime-browser-session.js';
import { supabaseAuthRequestPatch } from '../src/stacks/backends/supabase-operations.js';
import { convexAuthReadEndpoints } from '../src/stacks/backends/convex-operations.js';
import { ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from '../src/actions/action-contract.js';
import { installResponseLoss } from '../grader/response-loss.js';

test('UI write completion drains delayed HTTP effects, accepts no write, and rejects lost responses', async () => {
  let release: (() => void) | undefined, received!: () => void, writes = 0;
  const server = createServer(async (req, res) => {
    if (req.method !== 'POST') { res.end('<button id="buy">Buy</button>'); return; }
    for await (const _chunk of req) { /* Consume either form or JSON input. */ }
    writes++;
    if (req.url === '/lost') { res.writeHead(200, { 'content-type': 'application/json' }); res.write('{'); }
    release = () => {
      release = undefined;
      if (req.url === '/lost') res.destroy();
      else res.writeHead(req.url === '/invalid-json' ? 400 : req.url === '/accepted-pending' ? 202 : 200).end('{}');
    };
    received();
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await installAuthWebSocketCapture(page);
    await page.goto(`http://127.0.0.1:${(server.address() as { port: number }).port}`);
    assert.deepEqual((await withWriteCompletion(page, async () => {})).writes, []);
    for (const mode of ['json', 'form', 'invalid-json', 'accepted-pending', 'lost']) {
      const request = new Promise<void>(resolve => { received = resolve; });
      await page.evaluate(mode => {
        document.querySelector<HTMLButtonElement>('#buy')!.onclick = () => {
          void fetch(['lost', 'invalid-json', 'accepted-pending'].includes(mode) ? '/' + mode : '/buy', { method: 'POST',
            headers: { 'content-type': mode === 'form' ? 'application/x-www-form-urlencoded' : 'application/json' },
            body: mode === 'form' ? 'item=keyboard' : mode === 'invalid-json' ? '{' : '{"item":"keyboard"}' }).catch(() => {});
        };
      }, mode);
      const completion = withWriteCompletion(page, () => page.locator('#buy').click());
      await request;
      assert.equal(await Promise.race([completion.then(() => 'done', () => 'failed'),
        new Promise<string>(resolve => setTimeout(() => resolve('pending'), 100))]), 'pending');
      release!();
      if (['lost', 'accepted-pending'].includes(mode)) await assert.rejects(completion, ActionInconclusive);
      else assert.equal((await completion).writes.length, 1);
    }
    // A callback dispatched after the capture returns is outside its boundary.
    // This does not establish the absence of every possible future timer.
    await page.evaluate(() => { document.querySelector<HTMLButtonElement>('#buy')!.onclick = () => {
      Object.assign(window, { laterWrite: () => fetch('/buy', { method: 'POST' }).catch(() => {}) });
    }; });
    assert.equal((await withWriteCompletion(page, () => page.locator('#buy').click())).writes.length, 0);
    const request = new Promise<void>(resolve => { received = resolve; });
    const later = page.evaluate(() => (window as unknown as { laterWrite(): Promise<unknown> }).laterWrite());
    await request; release!(); await later;
    assert.equal(writes, 6);
  } finally {
    release?.(); await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

// Signup reconnects can carry authority in Socket.IO CONNECT data. Heartbeats
// are transport traffic; an application event without an acknowledgement is not.
test('signup reconnect probes include Socket.IO CONNECT claims and exclude heartbeat traffic', async t => {
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const accounts = new Map<string, string>();
  const polls = new Map<string, string[]>();
  const connects = new Map<string, number>();
  let defective = false, stock = 0;
  const connect = (user: string, frame: string) => {
    const claims = frame.length > 2 ? JSON.parse(frame.slice(2)) : {};
    if (accounts.has(user) && defective && claims.role === 'admin') accounts.set(user, 'admin');
    return `40${JSON.stringify({ sid: `namespace-${user}` })}`;
  };
  const server = createServer(async (req, res) => {
    const user = /user=([^;]+)/.exec(req.headers.cookie ?? '')?.[1] ?? '';
    const address = new URL(req.url!, 'http://fixture.test');
    if (address.searchParams.get('transport') === 'polling') {
      const sid = address.searchParams.get('sid');
      res.setHeader('content-type', 'text/plain');
      if (!sid) {
        const id = `poll-${user}`; polls.set(id, []);
        res.end(`0${JSON.stringify({ sid: id, upgrades: ['websocket'], pingInterval: 25000, pingTimeout: 20000, maxPayload: 1000000 })}`); return;
      }
      if (req.method === 'POST') {
        let body = ''; for await (const chunk of req) body += chunk;
        for (const frame of body.split('\x1e')) if (frame.startsWith('40')) polls.get(sid)!.push(connect(user, frame), '2');
        res.end('ok'); return;
      }
      res.end(polls.get(sid)!.splice(0).join('\x1e') || '6'); return;
    }
    if (req.url === '/signup') {
      for await (const _chunk of req) { /* Consume the ordinary registration. */ }
      accounts.set(user, 'customer'); res.end('ok'); return;
    }
    if (req.url === '/restock') {
      if (accounts.get(user) !== 'admin') { res.writeHead(403).end(); return; }
      stock++; res.end('ok'); return;
    }
    res.end('<body>signup</body>');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (raw: Buffer) => void): void; send(data: string): void },
    request: { url: string; headers: { cookie?: string } }) => {
    const user = /user=([^;]+)/.exec(request.headers.cookie ?? '')?.[1] ?? '';
    if (!new URL(request.url, url).searchParams.has('sid')) {
      socket.send(`0${JSON.stringify({ sid: user, upgrades: [], pingInterval: 25000, pingTimeout: 20000, maxPayload: 1000000 })}`);
    }
    socket.on('message', raw => {
      const frame = String(raw);
      if (frame.startsWith('40')) {
        connects.set(user, (connects.get(user) ?? 0) + 1);
        if (user === 'overlap' && connects.get(user) === 1) { socket.send('6'); return; }
        if (user === 'missing-ack' && connects.get(user)! > 1) { socket.send('6'); return; }
        socket.send(connect(user, frame));
        socket.send('2');
      } else if (frame === '2probe') socket.send('3probe');
      else if (frame === '5') socket.send('2');
      else if (frame === '3' || frame.startsWith('42')) socket.send('6');
    });
  });
  const browser = await chromium.launch({ headless: true });
  try {
    const fresh = async (user: string, query = '') => {
      const context = await browser.newContext(), page = await context.newPage();
      await installAuthWebSocketCapture(page); await page.goto(url);
      await page.evaluate(async ({ url, user, query }) => {
        document.cookie = `user=${user}`;
        const socket = new WebSocket(`${url.replace('http:', 'ws:')}/socket.io/?EIO=4&transport=websocket${query}`);
        Object.assign(window, { fixtureSocket: socket });
        await new Promise<void>(resolve => socket.addEventListener('message', event => {
          if (String(event.data).startsWith('0')) socket.send('40');
          if (event.data === '2') socket.send('3');
          if (event.data === '6') resolve();
        }));
      }, { url, user, query });
      return page;
    };
    const submit = (page: Awaited<ReturnType<typeof fresh>>, auth: boolean, unknownEvent = false, polling = false) => page.evaluate(async ({ auth, unknownEvent, polling }) => {
      const socket = (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket;
      await fetch('/signup', { method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: 'customer', password: 'fixture-password' }) });
      if (polling) {
        socket.send('41'); socket.close();
        const base = '/socket.io/?EIO=4&transport=polling';
        const open = JSON.parse((await (await fetch(base)).text()).slice(1));
        const path = `${base}&sid=${open.sid}`;
        await fetch(path, { method: 'POST', body: auth ? '40{"token":"fixture-session"}' : '40' });
        const reply = await (await fetch(path)).text();
        if (!reply.startsWith('40')) throw new Error('Missing CONNECT receipt');
        await fetch(path, { method: 'POST', body: '3' });
        const upgrade = new WebSocket(`${location.origin.replace('http:', 'ws:')}/socket.io/?EIO=4&transport=websocket&sid=${open.sid}`);
        await new Promise<void>(resolve => {
          upgrade.addEventListener('open', () => upgrade.send('2probe'));
          upgrade.addEventListener('message', event => {
            if (event.data === '3probe') upgrade.send('5');
            if (event.data === '2') upgrade.send('3');
            if (event.data === '6') resolve();
          });
        });
        return;
      }
      const reply = new Promise<void>(resolve => {
        const listener = (event: MessageEvent) => { if (event.data === '6') { socket.removeEventListener('message', listener); resolve(); } };
        socket.addEventListener('message', listener);
      });
      if (unknownEvent) socket.send('42["finish",{"role":"customer"}]');
      else { socket.send('41'); socket.send(auth ? '40{"token":"fixture-session"}' : '40'); }
      await reply;
    }, { auth, unknownEvent, polling });
    let lastWrites: readonly AuthWrite[] = [];
    for (const polling of [false, true]) for (const auth of [false, true]) await t.test(`${polling ? 'polling upgrade' : 'websocket'} ${auth ? 'token' : 'empty'} CONNECT`, async () => {
      const ordinary = await fresh(`ordinary-${polling}-${auth}`);
      const baseline = await withAuthWriteInventory(ordinary, () => submit(ordinary, auth, false, polling));
      lastWrites = baseline.writes;
      assert.equal(baseline.writes.length, 2, 'the HTTP write and CONNECT are targets; disconnect and heartbeat are not');
      assert(!JSON.stringify(baseline).includes('fixture-session'));
      for (defective of [false, true]) for (const index of [0, 1]) {
        const page = await fresh(`probe-${polling}-${auth}-${defective}-${index}`), before = stock;
        const result = await withAuthRequestPatch(page, 'customer', 'fixture-password', { fields: { role: 'admin' } },
          () => submit(page, auth, false, polling), undefined, 'signup', { writes: baseline.writes, index });
        assert(result.requestPatch.status === 200 || result.requestPatch.success === true);
        const status = await page.evaluate(async () => (await fetch('/restock', { method: 'POST' })).status);
        assert.equal(status, defective && index === 1 ? 200 : 403);
        assert.equal(stock - before, defective && index === 1 ? 1 : 0);
        await page.context().close();
      }
      await ordinary.context().close();
    });
    const crossTransport = await fresh('cross-transport');
    await withAuthRequestPatch(crossTransport, 'customer', 'fixture-password', { fields: { role: 'admin' } },
      () => submit(crossTransport, true), undefined, 'signup', { writes: lastWrites, index: 1 });
    const overlap = await fresh('overlap');
    await assert.rejects(withAuthWriteInventory(overlap, () => overlap.evaluate(async () => {
      await fetch('/signup', { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
      const socket = (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket;
      await new Promise<void>(resolve => {
        socket.addEventListener('message', event => { if (event.data === '6') resolve(); });
        socket.send('40');
      });
    })), ActionInconclusive, 'a delayed pre-capture receipt must not prove a later overlapping CONNECT');
    const changedRoute = await fresh('changed-route', '&mode=promote');
    await assert.rejects(withAuthRequestPatch(changedRoute, 'customer', 'fixture-password', { fields: { role: 'admin' } },
      () => submit(changedRoute, true), undefined, 'signup', { writes: lastWrites, index: 1 }), ActionInconclusive);
    const unacknowledged = await fresh('unacknowledged');
    await assert.rejects(withAuthWriteInventory(unacknowledged, () => submit(unacknowledged, false, true)), ActionInconclusive);
    const missingAck = await fresh('missing-ack');
    await assert.rejects(withAuthWriteInventory(missingAck, () => submit(missingAck, false)), ActionInconclusive);
    const duplicate = await fresh('duplicate');
    await assert.rejects(withAuthWriteInventory(duplicate, () => duplicate.evaluate(async () => {
      const socket = (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket;
      await new Promise<void>(resolve => {
        socket.addEventListener('message', event => { if (event.data === '6') resolve(); });
        socket.send('41'); socket.send('40'); socket.send('40');
      });
    })), ActionInconclusive);
    const unknownSid = await fresh('unknown-sid');
    await assert.rejects(withAuthWriteInventory(unknownSid, () => unknownSid.evaluate(async () => {
      const socket = new WebSocket(`${location.origin.replace('http:', 'ws:')}/socket.io/?EIO=4&transport=websocket&sid=unknown`);
      await new Promise<void>(resolve => {
        socket.addEventListener('open', () => socket.send('2probe'));
        socket.addEventListener('message', () => resolve());
      });
    })), ActionInconclusive);
  } finally {
    await browser.close(); sockets.close(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

// A later write can begin while an earlier response body is still draining.
// Reaching the end of the earlier body cannot prove the later write's receipt.
test('signup inventory drains CONNECT receipts added while an HTTP body is pending', async () => {
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  let finishBody: (() => void) | undefined, acknowledge: (() => void) | undefined;
  let receivedConnect!: () => void;
  const connected = new Promise<void>(resolve => { receivedConnect = resolve; });
  const server = createServer(async (req, res) => {
    if (req.url !== '/signup') { res.end('<body>signup</body>'); return; }
    for await (const _chunk of req) { /* Consume the write before sending headers. */ }
    res.writeHead(200, { 'content-type': 'application/json' });
    res.write('{"ok":');
    finishBody = () => { finishBody = undefined; res.end('true}'); };
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (raw: Buffer) => void): void; send(data: string): void }) => {
    socket.send(`0${JSON.stringify({ sid: 'drain', upgrades: [], pingInterval: 25000, pingTimeout: 20000 })}`);
    socket.on('message', raw => {
      if (String(raw) === '40') {
        acknowledge = () => { acknowledge = undefined; socket.send('40{"sid":"namespace-drain"}'); };
        receivedConnect();
      }
    });
  });
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await installAuthWebSocketCapture(page); await page.goto(url);
    await page.evaluate(async url => {
      const socket = new WebSocket(`${url.replace('http:', 'ws:')}/socket.io/?EIO=4&transport=websocket`);
      Object.assign(window, { fixtureSocket: socket });
      await new Promise<void>(resolve => socket.addEventListener('message', () => resolve(), { once: true }));
    }, url);
    let submitted!: () => void;
    const headersReceived = new Promise<void>(resolve => { submitted = resolve; });
    const inventory = withAuthWriteInventory(page, async () => {
      await page.evaluate(async () => {
        // fetch resolves at headers; the evaluator still waits for the body.
        await fetch('/signup', { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
      });
      submitted();
    });
    await headersReceived;
    await page.evaluate(() => (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket.send('40'));
    await connected;
    finishBody!();
    const outcome = await Promise.race([
      inventory.then(() => 'measured', () => 'rejected'),
      new Promise<string>(resolve => setTimeout(() => resolve('waiting'), 150)),
    ]);
    assert.equal(outcome, 'waiting', 'inventory must wait for the CONNECT receipt added during the HTTP drain');
    acknowledge!();
    assert.equal((await inventory).writes.length, 2);
  } finally {
    finishBody?.(); acknowledge?.();
    await browser.close(); sockets.close(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

// Only the exact authenticated backend endpoint has Convex's read semantics.
// A same-named app route, another origin/port, query suffix, or method must remain
// in the inventory and receive its own probe instead of inheriting an exemption.
test('signup inventory excludes only the exact leased Convex query POST', async () => {
  const received: { url: string; method: string; role: unknown }[] = [];
  const serve = () => createServer(async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Headers', 'content-type');
    res.setHeader('Access-Control-Allow-Methods', 'POST, PUT');
    if (req.method === 'OPTIONS') { res.writeHead(204).end(); return; }
    if (req.method === 'GET') { res.end('<body>signup</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    received.push({ url: req.url!, method: req.method!, role: raw ? JSON.parse(raw).role : null });
    res.end('ok');
  }).listen(0, '127.0.0.1');
  const backend = serve(), app = serve();
  await Promise.all([once(backend, 'listening'), once(app, 'listening')]);
  const origin = (server: typeof app) => `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const lease = createBackendLease({ runId: 'convex-query-capture', backend: 'convex', track: 'ecommerce',
    runIndex: 0, serverUri: origin(backend) });
  lease.state = 'active';
  const endpoints = convexAuthReadEndpoints(lease);
  assert.deepEqual(endpoints, [`${origin(backend)}/api/query`]);
  assert.throws(() => convexAuthReadEndpoints({ ...lease, state: 'released' }), /lease/i);
  assert.throws(() => convexAuthReadEndpoints({ ...lease, backend: 'postgres' }), /lease/i);
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [url, method, exempt] of [
      [`${origin(backend)}/api/query`, 'POST', true],
      [`${origin(app)}/api/query`, 'POST', false],
      [`${origin(backend).replace('127.0.0.1', 'localhost')}/api/query`, 'POST', false],
      [`${origin(backend)}/api/query?write=true`, 'POST', false],
      [`${origin(backend)}/nested/api/query`, 'POST', false],
      [`${origin(backend)}/api/query`, 'PUT', false],
    ] as const) {
      const context = await browser.newContext(), page = await context.newPage(); await page.goto(origin(app));
      const submit = () => page.evaluate(async ({ url, method }) => {
        await fetch('/signup', { method: 'POST', headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ username: 'ordinary', password: 'fixture-password' }) });
        await fetch(url, { method, headers: { 'content-type': 'application/json' }, body: '{}' });
      }, { url, method });
      const baseline: { writes: readonly AuthWrite[] } = await withAuthWriteInventory(page, submit, endpoints);
      assert.equal(baseline.writes.length, exempt ? 1 : 2, `${method} ${url}`);
      received.length = 0;
      const index = exempt ? 0 : 1;
      await withAuthRequestPatch(page, 'ordinary', 'fixture-password', { fields: { role: 'admin' } }, submit,
        undefined, 'signup', { writes: baseline.writes, index, readEndpoints: endpoints });
      assert.deepEqual(received.map(request => request.role), exempt ? ['admin', undefined] : [undefined, 'admin']);
      await context.close();
    }
  } finally {
    await browser.close();
    await Promise.all([backend, app].map(server => new Promise<void>(resolve => server.close(() => resolve()))));
  }
});

// Failure cases fixed before implementation: an unrelated profile write must not
// hide a bodyless account finalizer, and a missing/changed target is unmeasured.
test('each signup write is patched once, including a bodyless account finalizer', async () => {
  const accounts = new Map<string, string>(), calls: { path: string; role?: string }[] = [];
  let defective = false, stock = 10;
  const server = createServer(async (req, res) => {
    if (req.method !== 'POST') { res.end('<body>accounts</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    const body = raw ? JSON.parse(raw) : {};
    const user = /user=([^;]+)/.exec(req.headers.cookie ?? '')?.[1] ?? '';
    calls.push({ path: req.url!, ...(body.role ? { role: body.role } : {}) });
    if (req.url === '/nested/stdb/v1/identity/websocket-token') accounts.set(user, defective ? body.role ?? 'customer' : 'customer');
    if (req.url === '/restock') {
      if (accounts.get(user) !== 'admin') { res.writeHead(403).end(); return; }
      stock++;
    }
    res.end('ok');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const fresh = async (user: string) => {
      const context = await browser.newContext(), page = await context.newPage();
      await page.goto(url); await page.evaluate(user => { document.cookie = `user=${user}`; }, user);
      return page;
    };
    const submit = (page: Awaited<ReturnType<typeof fresh>>, mode = 'normal') => page.evaluate(async mode => {
      await fetch('/profile', { method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ opaquePassword: 'fixture-derived-secret', role: 'customer' }) });
      if (mode === 'missing') return;
      if (mode === 'extra') await fetch('/profile', { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
      await fetch(mode === 'changed' ? '/other-finalizer' : '/nested/stdb/v1/identity/websocket-token', { method: 'POST' });
      if (mode === 'duplicate') await fetch('/nested/stdb/v1/identity/websocket-token', { method: 'POST' });
      if (mode === 'after') await fetch('/claimed-follow-up', { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
    }, mode);
    const baselinePage = await fresh('ordinary');
    const baseline = await withAuthWriteInventory(baselinePage, () => submit(baselinePage));
    assert.equal(baseline.writes.length, 2);
    assert(!JSON.stringify(baseline).includes('fixture-derived-secret'));
    const restock = (page: typeof baselinePage) => page.evaluate(async () => (await fetch('/restock', { method: 'POST' })).status);
    assert.equal(await restock(baselinePage), 403);
    accounts.set('admin', 'admin'); assert.equal(await restock(await fresh('admin')), 200);
    for (defective of [false, true]) for (const index of [0, 1]) {
      const page = await fresh(`probe-${defective}-${index}`), before = stock;
      calls.length = 0;
      const result = await withAuthRequestPatch(page, 'unused', 'unused', { fields: { role: 'admin' } },
        () => submit(page), undefined, 'signup', { writes: baseline.writes, index });
      assert.equal(result.requestPatch.status, 200);
      assert.deepEqual(calls.map(call => call.role), index === 0 ? ['admin', undefined] : ['customer', 'admin']);
      assert.equal(await restock(page), defective && index === 1 ? 200 : 403);
      assert.equal(stock - before, defective && index === 1 ? 1 : 0);
      await page.context().close();
    }
    for (const mode of ['missing', 'changed', 'extra', 'duplicate']) {
      const page = await fresh(mode);
      // A sequence the fresh client cannot repeat keeps both inventories and may run once more.
      await assert.rejects(withAuthRequestPatch(page, 'unused', 'unused', { fields: { role: 'admin' } },
        () => submit(page, mode), undefined, 'signup', { writes: baseline.writes, index: 1 }), (error: ActionInconclusive) => {
        const observation = error.details.observation as { baseline: unknown; target: number; writes: unknown[] };
        assert.equal(error.details.retryable, true, mode);
        assert.deepEqual([observation.baseline, observation.target], [baseline.writes, 1]);
        assert(observation.writes.length > 0);
        return true;
      });
      await page.context().close();
    }
    // The claim may change what the app does after the patched target, so a later
    // different write is recorded, not refused. A repeat of the target (here the
    // second profile write) would be unpatched and could undo the claim.
    for (const [mode, index] of [['changed', 0], ['after', 0], ['after', 1]] as const) {
      const page = await fresh(`${mode}-after-target-${index}`);
      calls.length = 0;
      const result = await withAuthRequestPatch(page, 'unused', 'unused', { fields: { role: 'admin' } },
        () => submit(page, mode), undefined, 'signup', { writes: baseline.writes, index });
      assert.equal(result.requestPatch.status, 200, `${mode} ${index}`);
      assert.deepEqual(calls.map(call => call.role), index === 0
        ? ['admin', undefined, ...(mode === 'after' ? [undefined] : [])] : ['customer', 'admin', undefined]);
      assert.equal(calls.at(-1)!.path, mode === 'after' ? '/claimed-follow-up' : '/other-finalizer');
      await page.context().close();
    }
    const repeated = await fresh('repeated-target');
    await assert.rejects(withAuthRequestPatch(repeated, 'unused', 'unused', { fields: { role: 'admin' } },
      () => submit(repeated, 'extra'), undefined, 'signup', { writes: baseline.writes, index: 0 }), ActionInconclusive);
    await repeated.context().close();
    const completing = await fresh('completing');
    await withAuthWriteTarget(completing, { writes: baseline.writes, index: 0 }, () =>
      withAuthRequestPatch(completing, 'unused', 'unused', { fields: { role: 'admin' } },
        () => submit(completing, 'missing'), undefined, 'signup', undefined, async () => {
          await completing.evaluate(() => fetch('/nested/stdb/v1/identity/websocket-token', { method: 'POST' }).then(() => {}));
          stopAuthWriteInventory(completing);
          await completing.evaluate(() => fetch('/separate-signin', { method: 'POST' }).then(() => {}));
        }));
    await completing.context().close();
    const delayed = await fresh('delayed');
    const delayedResult = await withAuthRequestPatch(delayed, 'unused', 'unused', { fields: { role: 'admin' } }, async () => {
      await submit(delayed, 'missing');
      await delayed.evaluate(() => { setTimeout(() => { void fetch('/nested/stdb/v1/identity/websocket-token', { method: 'POST' }); }, 30); });
    }, undefined, 'signup', { writes: baseline.writes, index: 1 });
    assert.equal(delayedResult.requestPatch.status, 200, 'capture must stay active for the selected later write');
    await delayed.context().close();
  } finally { await browser.close(); await new Promise<void>(resolve => server.close(() => resolve())); }
});

test('mixed native and HTTP signup probes expose either source of authority', async t => {
  const codec = await import(new URL('../src/stacks/spacetime-wire-codec.js', import.meta.url).href);
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const encode = (type: { serialize(writer: unknown, value: unknown): void }, value: unknown) => {
    const writer = new codec.BinaryWriter(128); type.serialize(writer, value); return Buffer.from(writer.getBuffer());
  };
  const readString = codec.AlgebraicType.makeDeserializer({ tag: 'String' });
  const writeString = codec.AlgebraicType.makeSerializer({ tag: 'String' });
  const accounts = new Map<string, string>(), profiles = new Map<string, string>();
  let mode = 'correct', stock = 10;
  const heldReceipts = new Map<number, () => void>();
  let nativeReceived: ((id: number) => void) | undefined;
  const calls: { transport: string; role: string }[] = [];
  const server = createServer(async (req, res) => {
    if (req.url?.startsWith('/v1/database/auth/schema')) {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ reducers: [{ name: 'profile_save', params: { elements:
        ['name', 'digest', 'role'].map(name => ({ name: { some: name }, algebraic_type: { String: [] } })) } }] })); return;
    }
    if (req.method !== 'POST') { res.end('<body>accounts</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    const user = /user=([^;]+)/.exec(req.headers.cookie ?? '')?.[1] ?? '';
    if (req.url === '/restock') {
      if (accounts.get(user) !== 'admin') { res.writeHead(403).end(); return; }
      stock++; res.end(); return;
    }
    const role = raw ? JSON.parse(raw).role : 'customer';
    calls.push({ transport: 'http', role });
    accounts.set(user, mode === 'http-defect' ? role : mode === 'native-defect' ? profiles.get(user)! : 'customer');
    res.end();
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (raw: Buffer) => void): void; send(data: Buffer): void; close(): void }) => {
    const send = (value: unknown) => socket.send(Buffer.concat([Buffer.from([0]), encode(codec.ServerMessage, value)]));
    send({ tag: 'InitialConnection', value: { identity: { __identity__: 1n }, connectionId: { __connection_id__: 1n }, token: 'fixture-token' } });
    socket.on('message', raw => {
      const message = codec.ClientMessage.deserialize(new codec.BinaryReader(raw));
      const reader = new codec.BinaryReader(message.value.args);
      const [user, , role] = [readString(reader), readString(reader), readString(reader)];
      calls.push({ transport: 'native', role }); profiles.set(user, role);
      if (mode === 'missing-terminal') { socket.close(); return; }
      const acknowledge = () => send({ tag: 'ReducerResult', value: { requestId: message.value.requestId,
        timestamp: { __timestamp_micros_since_unix_epoch__: 1n }, result: { tag: 'OkEmpty' } } });
      if (mode === 'append-native') {
        heldReceipts.set(message.value.requestId, acknowledge); nativeReceived?.(message.value.requestId); return;
      }
      acknowledge();
    });
  });
  const dir = mkdtempSync(join(tmpdir(), 'auth-write-probe-')), leasePath = join(dir, 'lease.json');
  const previous = { path: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  const lease = createBackendLease({ runId: 'auth-write-probe', backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    serverUri: url, module: 'auth', dataDir: join(dir, 'data') });
  lease.state = 'active'; writeBackendLease(leasePath, lease);
  process.env.STACK_BENCH_LEASE = leasePath; process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const browser = await chromium.launch({ headless: true });
  try {
    const fresh = async (user: string) => {
      const context = await browser.newContext(), page = await context.newPage();
      await installAuthWebSocketCapture(page); await installSpacetimeWriteCapture(page); await page.goto(url);
      await page.evaluate(async ({ url, user }) => {
        document.cookie = `user=${user}`;
        const socket = new WebSocket(`${url.replace('http:', 'ws:')}/nested/v1/database/auth/subscribe`, 'v3.bsatn.spacetimedb');
        Object.assign(window, { fixtureSocket: socket });
        await new Promise(resolve => socket.addEventListener('message', resolve, { once: true }));
      }, { url, user });
      return page;
    };
    const submit = async (page: Awaited<ReturnType<typeof fresh>>, user: string, omitFinalizer = false) => {
      const writer = new codec.BinaryWriter(128);
      for (const value of [user, 'opaque-derived-password', 'customer']) writeString(writer, value);
      const bytes = encode(codec.ClientMessage, { tag: mode === 'procedure' ? 'CallProcedure' : 'CallReducer', value: {
        ...(mode === 'procedure' ? { procedure: 'profile_save' } : { reducer: 'profile_save' }),
        requestId: 1, flags: 0, args: writer.getBuffer() } });
      return page.evaluate(async ({ bytes, omitFinalizer }) => {
        const socket = (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket;
        const reply = new Promise(resolve => {
          socket.addEventListener('message', resolve, { once: true }); socket.addEventListener('close', resolve, { once: true });
        });
        socket.send(new Uint8Array(bytes)); await reply;
        if (!omitFinalizer) await fetch('/nested/stdb/v1/identity/websocket-token', { method: 'POST' });
      }, { bytes: [...bytes], omitFinalizer });
    };
    for (const inventoryCall of [withAuthWriteInventory, withWriteCompletion]) await t.test(`${inventoryCall.name} waits for a reducer receipt appended during drain`, async () => {
      mode = 'append-native';
      const page = await fresh(mode);
      let firstReceived!: () => void, secondReceived!: () => void, submitted!: () => void;
      const first = new Promise<void>(resolve => { firstReceived = resolve; });
      const second = new Promise<void>(resolve => { secondReceived = resolve; });
      const submission = new Promise<void>(resolve => { submitted = resolve; });
      nativeReceived = id => { if (id === 1) firstReceived(); if (id === 2) secondReceived(); };
      const sendWrite = async (id: number) => {
        const writer = new codec.BinaryWriter(128);
        for (const value of ['append-native', 'opaque-derived-password', 'customer']) writeString(writer, value);
        const bytes = encode(codec.ClientMessage, { tag: 'CallReducer', value: {
          reducer: 'profile_save', requestId: id, flags: 0, args: writer.getBuffer() } });
        await page.evaluate(bytes => (window as unknown as { fixtureSocket: WebSocket }).fixtureSocket.send(new Uint8Array(bytes)), [...bytes]);
      };
      try {
        const inventory = inventoryCall(page, async () => {
          await sendWrite(1); await first; submitted();
        });
        await submission;
        await sendWrite(2); await second;
        heldReceipts.get(1)!(); heldReceipts.delete(1);
        const outcome = await Promise.race([
          inventory.then(() => 'measured', () => 'rejected'),
          new Promise<string>(resolve => setTimeout(() => resolve('waiting'), 150)),
        ]);
        assert.equal(outcome, 'waiting', 'an ACK for the first reducer must not prove the second reducer');
        heldReceipts.get(2)!(); heldReceipts.delete(2);
        assert.equal((await inventory).writes.length, 2);
      } finally {
        for (const acknowledge of heldReceipts.values()) acknowledge();
        heldReceipts.clear(); nativeReceived = undefined;
        await page.context().close(); mode = 'correct';
      }
    });
    const baselinePage = await fresh('baseline');
    const baseline = await withAuthWriteInventory(baselinePage, () => submit(baselinePage, 'baseline'));
    assert.deepEqual(baseline.writes.map(write => write.transport), ['spacetime-websocket', 'http']);
    assert(!JSON.stringify(baseline).includes('opaque-derived-password'));
    const restock = (page: typeof baselinePage) => page.evaluate(async () => (await fetch('/restock', { method: 'POST' })).status);
    assert.equal(await restock(baselinePage), 403);
    accounts.set('admin', 'admin'); assert.equal(await restock(await fresh('admin')), 200);
    for (mode of ['correct', 'http-defect', 'native-defect']) for (const index of [0, 1]) {
      const user = `${mode}-${index}`, page = await fresh(user), before = stock; calls.length = 0;
      const result = await withAuthRequestPatch(page, user, 'unsubmitted-password', { fields: { role: 'admin' } },
        () => submit(page, user), undefined, 'signup', { writes: baseline.writes, index });
      assert.equal(result.requestPatch.transport, index === 0 ? 'spacetime-websocket' : undefined);
      assert.deepEqual(calls.map(call => call.role), index === 0 ? ['admin', 'customer'] : ['customer', 'admin']);
      const elevated = mode === 'http-defect' && index === 1 || mode === 'native-defect' && index === 0;
      assert.equal(await restock(page), elevated ? 200 : 403); assert.equal(stock - before, elevated ? 1 : 0);
      await page.context().close();
    }
    for (mode of ['unreached', 'missing-terminal', 'procedure']) {
      const page = await fresh(mode);
      await assert.rejects(withAuthRequestPatch(page, mode, 'unused', { fields: { role: 'admin' } },
        () => submit(page, mode, true), undefined, 'signup', { writes: baseline.writes, index: 1 }), ActionInconclusive);
      await page.context().close();
    }
    mode = 'procedure';
    const procedure = await fresh(mode);
    await assert.rejects(withAuthWriteInventory(procedure, () => submit(procedure, mode)), ActionInconclusive);
    mode = 'missing-terminal';
    const incomplete = await fresh(mode);
    await assert.rejects(withWriteCompletion(incomplete, () => submit(incomplete, mode, true)), ActionInconclusive);
  } finally {
    await browser.close(); sockets.close(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (previous.path === undefined) delete process.env.STACK_BENCH_LEASE; else process.env.STACK_BENCH_LEASE = previous.path;
    if (previous.token === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN; else process.env.STACK_BENCH_LEASE_TOKEN = previous.token;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('credential patches preserve native envelopes and reject ambiguous matches', () => {
  const credentials = { username: 'customer', password: 'secret' };
  const fields = JSON.parse('{"isAdmin":true,"__proto__":{"polluted":true}}');
  const claims = [{ name: 'name' }, { name: 'password' }, { name: 'salt' }, { name: 'claims', fields: ['isAdmin', 'nonce', '__proto__'] }];
  for (const body of [credentials, { path: 'auth:signIn', args: [{ provider: 'password', params: credentials }] },
    ['customer', 'secret', 'salt', { isAdmin: false, nonce: 4 }]]) {
    const before = JSON.stringify(body);
    const changed = patchAuthRequest(body, 'customer', 'secret', { fields }, Array.isArray(body) ? claims : undefined)!;
    assert(changed.body.includes('"isAdmin":true'));
    assert(changed.body.includes('"__proto__":'));
    assert.equal(JSON.stringify(body), before);
    assert.equal(Object.getPrototypeOf(credentials), Object.prototype);
  }
  assert.equal(patchAuthRequest({ other: 'secret' }, 'customer', 'secret', { fields }), null);
  // Positional arguments take a field only where the interface declares it.
  const named = (...names: string[]) => names.map(name => ({ name }));
  const positional = ['customer', 'secret', 'salt', false];
  assert.equal(patchAuthRequest(positional, 'customer', 'secret', { fields: { isAdmin: true } },
    named('name', 'password', 'salt', 'is_admin'))!.body, '["customer","secret","salt",true]');
  assert.deepEqual(patchAuthRequest(positional.slice(0, 3), 'customer', 'secret', { fields: { role: 'admin' } },
    named('name', 'password', 'salt')), { body: '["customer","secret","salt"]', shape: 'positional', absentParameters: ['role'] });
  assert.throws(() => patchAuthRequest(positional.slice(0, 3), 'customer', 'secret', { fields }), /interface parameters/);
  // A declared top-level parameter wins over a trailing object that does not declare the field.
  const mixed = ['customer', 'secret', 'shopper', { nonce: 'keep' }];
  assert.equal(patchAuthRequest(mixed, 'customer', 'secret', { fields: { role: 'admin' } },
    [...named('name', 'password', 'role'), { name: 'metadata', fields: ['nonce'] }])!.body,
  '["customer","secret","admin",{"nonce":"keep"}]');
  assert.throws(() => patchAuthRequest(mixed, 'customer', 'secret', { fields: { role: 'admin' } }), /interface parameters/,
    'a trailing object is never chosen just because it is last');
  assert.throws(() => patchAuthRequest(['customer', 'secret', { role: 'x' }, { role: 'y' }], 'customer', 'secret',
    { fields: { role: 'admin' } }, [...named('name', 'password'), { name: 'a', fields: ['role'] }, { name: 'b', fields: ['role'] }]),
  /Ambiguous/);
  // A type that could hide the field leaves its location unknown; an exact top-level target is still proven.
  assert.throws(() => patchAuthRequest(['customer', 'secret', { some: { role: 'x' } }], 'customer', 'secret',
    { fields: { role: 'admin' } }, [...named('name', 'password'), { name: 'claims', open: true }]), /location unknown/);
  assert.equal(patchAuthRequest(['customer', 'secret', 'shopper', {}], 'customer', 'secret', { fields: { role: 'admin' } },
    [...named('name', 'password', 'role'), { name: 'claims', open: true }])!.body, '["customer","secret","admin",{}]');
  assert.throws(() => patchAuthRequest([credentials, credentials], 'customer', 'secret', { fields }), /Multiple/);
  assert.throws(() => patchAuthRequest({ ...credentials, repeated: 'customer' }, 'customer', 'secret', { fields }), /Ambiguous/);
});

test('credential patches can replace the located password without guessing its key', () => {
  for (const body of [
    { username: 'customer', password: 'secret' },
    { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret' } }] },
    ['customer', 'secret'],
  ]) {
    const changed = patchAuthRequest(body, 'customer', 'secret', { password: "' OR '1'='1" })!;
    assert.match(changed.body, /' OR '1'='1/);
    assert.doesNotMatch(changed.body, /secret/);
  }
  assert.throws(() => patchAuthRequest({}, 'customer', 'secret', {}), /request change/);
});

test('Convex credential probes preserve the live socket and require its matching response', async () => {
  // Use the WebSocket server bundled with the pinned Playwright dependency.
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const server = createServer((_req, res) => res.end('<body>auth</body>')).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const ws = new wsServer({ server });
  const received: unknown[] = [];
  let effects = 0;
  let mode: 'accept' | 'reject' | 'disconnect' = 'accept';
  ws.on('connection', (socket: { on(event: string, callback: (data: Buffer) => void): void;
    send(data: string): void; close(): void }) => socket.on('message', raw => {
    const request = JSON.parse(String(raw)); received.push(request);
    if (mode === 'disconnect') { socket.close(); return; }
    socket.send(JSON.stringify({ type: `${request.type}Response`, requestId: request.requestId + 1, success: mode !== 'accept', result: null }));
    const accepted = mode === 'accept';
    setTimeout(() => {
      if (accepted) effects++;
      socket.send(JSON.stringify({ type: `${request.type}Response`, requestId: request.requestId, success: accepted, result: null }));
    }, 150);
  }));
  const browser = await chromium.launch({ headless: true });
  try {
    for (mode of ['accept', 'reject', 'disconnect'] as const) {
      const page = await browser.newPage();
      let sentFrames = 0;
      page.on('websocket', socket => socket.on('framesent', () => sentFrames++));
      await installAuthWebSocketCapture(page);
      await page.goto(url);
      await page.evaluate(async url => {
        const socket = new WebSocket(url.replace('http:', 'ws:') + '/api/1.0.0/sync');
        Object.assign(window, { testSocket: socket });
        await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
      }, url);
      const submit = () => page.evaluate(async () => {
        const socket = (window as unknown as { testSocket: WebSocket }).testSocket;
        const done = new Promise(resolve => {
          socket.addEventListener('message', event => {
            if (JSON.parse(String(event.data)).requestId === 7) resolve(undefined);
          });
          socket.addEventListener('close', resolve, { once: true });
        });
        socket.send(JSON.stringify({ type: 'Action', requestId: 7, udfPath: 'auth:signup',
          args: [{ username: 'customer', password: 'secret', nonce: 'unchanged' }] }));
        await done;
      });
      const result = withAuthRequestPatch(page, 'customer', 'secret', { fields: { role: 'admin' } }, submit);
      if (mode === 'disconnect') await assert.rejects(result, ActionInconclusive);
      else {
        const receipt = (await result).requestPatch;
        assert.equal(receipt.transport, 'convex-websocket');
        assert.equal(receipt.success, mode === 'accept');
        assert.equal(receipt.status, undefined);
        assert(!JSON.stringify(receipt).includes('secret'));
        await submit(); // Outside the probe, the native request remains unchanged.
        assert.equal((received.at(-1) as { args: { role?: string }[] }).args[0]!.role, undefined);
        const before = effects;
        const completed = withWriteCompletion(page, () => page.evaluate(() => {
          (window as unknown as { testSocket: WebSocket }).testSocket.send(JSON.stringify({
            type: 'Mutation', requestId: 8, udfPath: 'shop:buy', args: [{ itemId: 'keyboard' }] }));
        }));
        assert.equal(await Promise.race([completed.then(() => 'done'),
          new Promise<string>(resolve => setTimeout(() => resolve('waiting'), 50))]), 'waiting');
        assert.equal((await completed).writes[0]!.transport, 'convex-websocket');
        assert.equal(effects - before, mode === 'accept' ? 1 : 0);
        const previous = mode;
        mode = 'disconnect';
        await assert.rejects(withWriteCompletion(page, submit), ActionInconclusive);
        mode = previous;
      }
      const changed = received.find(value => (value as { args: { role?: string }[] }).args[0]?.role === 'admin');
      assert.deepEqual(changed, { type: 'Action', requestId: 7, udfPath: 'auth:signup',
        args: [{ username: 'customer', password: 'secret', nonce: 'unchanged', role: 'admin' }] });
      received.length = 0;
      await page.context().close();
      assert(sentFrames > 0, 'routing must preserve passive request observation');
    }
    mode = 'accept';
    const page = await browser.newPage();
    await installAuthWebSocketCapture(page);
    const gate = await installResponseLoss(page.context());
    await page.goto(url);
    await page.evaluate(async url => {
      const socket = new WebSocket(url.replace('http:', 'ws:') + '/api/1.0.0/sync');
      Object.assign(window, { testSocket: socket, replies: 0 });
      socket.onmessage = () => { (window as unknown as { replies: number }).replies++; };
      await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
    }, url);
    gate.arm();
    await page.evaluate(() => (window as unknown as { testSocket: WebSocket }).testSocket.send(JSON.stringify({
      type: 'Mutation', requestId: 9, args: [{}], udfPath: 'shop:checkout' })));
    for (let n = 0; n < 100 && !gate.evidence().events.some(e => e.kind === 'ws-drop'); n++) {
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert(gate.evidence().events.some(e => e.kind === 'ws-drop'));
    assert.equal(await page.evaluate(() => (window as unknown as { replies: number }).replies), 0);
    await gate.finish();
    assert.deepEqual(gate.evidence().errors, []);
    await page.context().close();
  } finally {
    await browser.close(); ws.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('real browser credential patch reaches the native request and keeps uncertain delivery unmeasured', async () => {
  const received: { path: string; body: unknown; cookie?: string; authorization?: string; nonce?: string }[] = [];
  const server = createServer(async (req, res) => {
    if (req.method === 'GET') { res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>auth</body>'); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    assert.equal(Buffer.byteLength(raw), Number(req.headers['content-length']));
    received.push({ path: req.url!, body: JSON.parse(raw), cookie: req.headers.cookie,
      authorization: req.headers.authorization, nonce: req.headers['x-nonce'] as string });
    if (req.url === '/lost') { req.socket.destroy(); return; }
    if (req.url === '/redirect') { res.writeHead(307, { Location: '/signup' }).end(); return; }
    res.writeHead(200, { 'Content-Type': 'application/json', 'Set-Cookie': 'sid=created; Path=/; HttpOnly' }).end('{"accepted":true}');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    await context.addCookies([{ name: 'sid', value: 'guest', url }]);
    const page = await context.newPage(); await page.goto(url);
    const body = { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret', flow: 'signUp' } }] };
    const submit = (path = '/signup', data: unknown = body, contentType = 'application/json') => page.evaluate(async ({ path, data, contentType }) => {
      try { return { status: (await fetch(path, { method: 'POST', headers: { 'Content-Type': contentType, Authorization: 'Bearer native-token', 'X-Nonce': 'kept' },
        body: typeof data === 'string' ? data : JSON.stringify(data) })).status }; }
      catch { return { status: 0 }; }
    }, { path, data, contentType });
    const run = (fn: () => Promise<unknown>) => withAuthRequestPatch(page, 'customer', 'secret', { fields: { isAdmin: true } }, fn);
    const result = await run(() => submit());
    assert.equal(result.requestPatch.status, 200);
    assert.match(result.requestPatch.bodySha256, /^[a-f0-9]{64}$/);
    assert.deepEqual(received[0], { path: '/signup', cookie: 'sid=guest', authorization: 'Bearer native-token', nonce: 'kept',
      body: { path: 'auth:signIn', args: [{ provider: 'password', params: { username: 'customer', password: 'secret', flow: 'signUp', isAdmin: true } }] } });
    assert.equal((await context.cookies()).find(c => c.name === 'sid')?.value, 'created');
    assert(!JSON.stringify(result).includes('secret'));
    for (const fn of [() => submit('/lost'), () => submit('/redirect'), () => submit('/signup', { other: 'value' }),
      () => submit('/signup', [body, body]), () => submit('/signup', 'username=customer&password=secret', 'application/x-www-form-urlencoded'),
      async () => { await submit(); return submit(); }]) {
      await assert.rejects(run(fn), ActionInconclusive);
    }
    const redirects = received.filter(r => r.path === '/redirect'); assert.equal(redirects.length, 1);
    for (const failure of [new ActionHarnessFailure('browser setup failed'),
      new Error('locator.fill: Unexpected token in selector'),
      new Error('Target page, context or browser has been closed'),
      new ActionInconclusive('input dispatch was not confirmed'), undefined, false, null]) {
      await assert.rejects(run(async () => { throw failure; }), error => {
        assert.equal(error, failure); return true;
      });
    }
    // Incomplete capture cannot turn a browser-control failure into an app defect.
    const timeout = Object.assign(new Error('locator.click: Timeout 1000ms exceeded'), { name: 'TimeoutError' });
    await assert.rejects(run(async () => { throw timeout; }), ActionInconclusive);
    const appFailure = new ActionApplicationFailure('application control was missing');
    await assert.rejects(run(async () => { throw appFailure; }), ActionInconclusive);
    await assert.rejects(run(async () => { await submit(); throw appFailure; }), error => {
      assert.equal(error, appFailure); return true;
    });
    // The route handler must be removed even after failed measurement.
    await submit(); assert.equal(JSON.stringify(received.at(-1)!.body), JSON.stringify(body));
    await context.close();
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('a SpacetimeDB signup patch places authority by the module schema and never guesses', async () => {
  // An object parameter is [name, its fields]; the schema declares it through the typespace, as a module does.
  // [name, type] gives a raw schema type, with any referenced types in `typespace`.
  let parameters: (string | [string, string[] | object])[] | null = ['name', 'password', 'salt'];
  let typespace: unknown[] = [];
  const calls: unknown[] = [];
  const server = createServer(async (req, res) => {
    if (req.url?.startsWith('/v1/database/shop/schema')) {
      if (!parameters) { res.writeHead(404).end(); return; }
      const types: unknown[] = [...typespace];
      const elements = parameters.map(parameter => typeof parameter === 'string'
        ? { name: { some: parameter }, algebraic_type: { String: [] } }
        : !Array.isArray(parameter[1]) ? { name: { some: parameter[0] }, algebraic_type: parameter[1] }
        : { name: { some: parameter[0] }, algebraic_type: { Ref: types.push({ Product: { elements: parameter[1].map(field =>
          ({ name: { some: field }, algebraic_type: { String: [] } })) } }) - 1 } });
      res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ typespace: { types }, reducers: [],
        misc_exports: [{ Procedure: { name: 'sign_up', params: { elements } } }] }));
      return;
    }
    if (req.method === 'POST') {
      let raw = ''; for await (const chunk of req) raw += chunk;
      calls.push(JSON.parse(raw));
      res.writeHead(200, { 'Content-Type': 'application/json' }).end('true'); return;
    }
    res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>shop</body>');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage(); await page.goto(url);
    const signUp = (args: unknown[]) => withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } },
      () => page.evaluate(async args => {
        try { return (await fetch('/v1/database/shop/call/sign_up', { method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: 'Bearer native' }, body: JSON.stringify(args) })).status; }
        catch { return 0; }
      }, args));
    // The reference interface has no authority parameter: the schema proves it, and the request goes as sent.
    const correct = await signUp(['claimant', 'secret', 'salt']);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt']);
    assert.deepEqual(correct.requestPatch.absentParameters, ['role']);
    // A defective interface that accepts a role receives the claim where it declared it.
    parameters = ['name', 'password', 'salt', 'role'];
    await signUp(['claimant', 'secret', 'salt', 'customer']);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt', 'admin']);
    // So does one that declares the role inside a claims object.
    parameters = ['name', 'password', 'salt', ['claims', ['role']]];
    await signUp(['claimant', 'secret', 'salt', { role: 'customer' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'salt', { role: 'admin' }]);
    // A declared role parameter receives the claim even when a trailing object follows it.
    parameters = ['name', 'password', 'role', ['metadata', ['nonce']]];
    await signUp(['claimant', 'secret', 'customer', { nonce: 'keep' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', 'admin', { nonce: 'keep' }]);
    // Without parameter names the probe is unmeasured, not sent in a guessed shape.
    parameters = null;
    await assert.rejects(signUp(['claimant', 'secret', 'salt', { role: 'customer' }]), ActionInconclusive);
    assert.equal(calls.length, 4);
    // Optional claims: the role goes inside the object the app sent.
    const field = (name: string, algebraic_type: object) => ({ name: { some: name }, algebraic_type });
    typespace = [{ Sum: { variants: [field('some', { Product: { elements: [field('role', { String: [] }),
      field('nonce', { String: [] })] } }), field('none', { Product: { elements: [] } })] } }];
    parameters = ['name', 'password', ['claims', { Ref: 0 }]];
    await signUp(['claimant', 'secret', { some: { role: 'customer', nonce: 'keep' } }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', { some: { role: 'admin', nonce: 'keep' } }]);
    // Types the reader cannot see into are not proof of absence, and an absent option is not built:
    // each leaves the probe unmeasured and sends nothing.
    for (const [claims, argument] of [[{ Ref: 0 }, { none: [] }], [{ Ref: 9 }, { role: 'customer' }],
      [{ Product: { elements: [field('settings', { Product: { elements: [field('role', { String: [] })] } })] } },
        { settings: { role: 'customer' } }]] as const) {
      parameters = ['name', 'password', ['claims', claims]];
      await assert.rejects(signUp(['claimant', 'secret', argument]), ActionInconclusive);
    }
    assert.equal(calls.length, 5);
    // A page whose own submit throws on the aborted request is still unmeasured, not a harness failure.
    parameters = ['name', 'password', ['claims', { Ref: 9 }]];
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } },
      () => page.evaluate(() => fetch('/v1/database/shop/call/sign_up', { method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(['claimant', 'secret', { some: { role: 'customer' } }]) }).then(response => response.status))),
    ActionInconclusive);
    assert.equal(calls.length, 5);
    // An optional plain value cannot hold the field, so absence stays measurable.
    parameters = ['name', 'password', ['nickname', { Sum: { variants: [field('some', { String: [] }),
      field('none', { Product: { elements: [] } })] } }]];
    const optional = await signUp(['claimant', 'secret', { some: 'pat' }]);
    assert.deepEqual(calls.at(-1), ['claimant', 'secret', { some: 'pat' }]);
    assert.deepEqual(optional.requestPatch.absentParameters, ['role']);
  } finally {
    await browser.close();
    server.close();
  }
});


test('a platform password endpoint patch reaches the cross-origin request only through the stack hook', async () => {
  const received: { path: string; body: unknown; apikey?: string }[] = [];
  const gateway = createServer(async (req, res) => {
    const cors = { 'Access-Control-Allow-Origin': String(req.headers.origin ?? '*'),
      'Access-Control-Allow-Headers': 'apikey, authorization, content-type', 'Access-Control-Allow-Methods': 'POST' };
    if (req.method === 'OPTIONS') { res.writeHead(204, cors).end(); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    received.push({ path: req.url!, body: JSON.parse(raw), apikey: req.headers.apikey as string });
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"access_token":"token"}');
  }).listen(0, '127.0.0.1');
  const app = createServer((_req, res) => res.writeHead(200, { 'Content-Type': 'text/html' }).end('<body>shop</body>'))
    .listen(0, '127.0.0.1');
  await Promise.all([once(gateway, 'listening'), once(app, 'listening')]);
  const origin = (server: typeof app) => `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const lease = createBackendLease({ runId: 'auth-patch', backend: 'supabase', track: 'ecommerce', runIndex: 0,
    serverUri: origin(gateway), database: 'postgres' });
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage(); await page.goto(origin(app));
    // The application derives both credentials; neither typed value appears in the request.
    const email = `${Buffer.from('claimant').toString('hex')}@accounts.invalid`;
    const digest = createHash('sha256').update('secret').digest('hex');
    const signUp = () => page.evaluate(async ({ url, email, digest }) => (await fetch(`${url}/auth/v1/signup`, {
      method: 'POST', headers: { 'Content-Type': 'application/json;charset=UTF-8', apikey: 'anon' },
      body: JSON.stringify({ email, password: digest, data: { username: 'claimant' } }) })).status,
    { url: origin(gateway), email, digest });
    const result = await withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } }, signUp,
      supabaseAuthRequestPatch(lease));
    assert.equal(result.requestPatch.shape, 'supabase-auth');
    assert.equal(result.requestPatch.status, 200);
    assert.deepEqual(received.at(-1), { path: '/auth/v1/signup', apikey: 'anon',
      body: { email, password: digest, role: 'admin', data: { username: 'claimant', role: 'admin' } } });
    assert(!JSON.stringify(result).includes(digest));
    // Without the stack hook the request holds neither typed credential, so nothing is proven or changed.
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', { fields: { role: 'admin' } }, signUp),
      ActionInconclusive);
    assert.deepEqual(received.at(-1)!.body, { email, password: digest, data: { username: 'claimant' } });
    // An invalid change is refused before any hook sees the request.
    await assert.rejects(withAuthRequestPatch(page, 'claimant', 'secret', {}, signUp, supabaseAuthRequestPatch(lease)),
      ActionInconclusive);
    assert.equal(received.length, 2);
    // A query-like password reaches password sign-in in place of the application's derived value.
    const signIn = () => page.evaluate(async ({ url, email, digest }) => (await fetch(`${url}/auth/v1/token?grant_type=password`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', apikey: 'anon' },
      body: JSON.stringify({ email, password: digest }) })).status, { url: origin(gateway), email, digest });
    await withAuthRequestPatch(page, 'kim', 'not-kims-password', { password: "' OR '1'='1" }, signIn,
      supabaseAuthRequestPatch(lease));
    assert.deepEqual(received.at(-1), { path: '/auth/v1/token?grant_type=password', apikey: 'anon',
      body: { email, password: "' OR '1'='1" } });
  } finally {
    await browser.close();
    gateway.close(); app.close();
  }
});
