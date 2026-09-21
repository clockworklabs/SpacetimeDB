import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { runApplicationNavigation } from '../src/actions/browser-navigation.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';

test('an explicit reload accepts a leave-page warning but still dismisses ordinary confirmations', async () => {
  const server = createServer((_request, response) => response.end(`<button id="arm">Arm</button>
    <button id="confirm">Confirm</button><span id="answer"></span><span id="loads"></span><script>
    const loads = Number(sessionStorage.getItem('loads') || 0) + 1;
    sessionStorage.setItem('loads', String(loads)); document.querySelector('#loads').textContent = loads;
    document.querySelector('#confirm').onclick = () => document.querySelector('#answer').textContent =
      confirm('Continue?') ? 'accepted' : 'dismissed';
    document.querySelector('#arm').onclick = () => window.addEventListener('beforeunload', event => {
      event.preventDefault(); event.returnValue = 'Pending write';
    });</script>`));
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const definition = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
      name: 'leave page', features: [{ id: 1, name: 'leave page', actors: ['owner'], setup: [],
        criteria: [{ id: 'reload', desc: 'explicit navigation proceeds', points: 1, steps: [
          { do: 'click', actor: 'owner', testid: 'confirm' },
          { do: 'expect', actor: 'owner', testid: 'answer', contains: 'dismissed' },
          { do: 'click', actor: 'owner', testid: 'arm' },
          { do: 'reload', actor: 'owner', settleMs: 0 },
          { do: 'expect', actor: 'owner', testid: 'loads', contains: '2' },
        ] }] }] });
    const result = await gradeFeature(browser, definition.features[0]!, {
      url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false,
    }, { runId: 'leave-page', roomName: name => name, url, actions: [], spacetime: null, nullControl: false });
    const evidence = result.criteria[0]!.evidence;
    assert.equal(evidence.status, 'passed', evidence.summary ?? undefined);
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('a stalled stylesheet leaves navigation unmeasured and the unchanged app can load later', async () => {
  let stall = true;
  const server = createServer((request, response) => {
    if (request.url === '/style.css') {
      if (!stall) response.writeHead(200, { 'Content-Type': 'text/css' }).end('');
      return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' }).end(
      '<link rel="stylesheet" href="/style.css"><script type="module">document.body.textContent="ready"</script>');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const operation of [() => page.goto(url, { waitUntil: 'domcontentloaded', timeout: 250 }),
      () => page.reload({ waitUntil: 'domcontentloaded', timeout: 250 })]) {
      await assert.rejects(runApplicationNavigation(operation, page), error => {
        assert(error instanceof ActionInconclusive);
        assert.deepEqual(error.details.observation, { pendingResources: [`stylesheet ${url}`] });
        return true;
      });
    }
    stall = false;
    await runApplicationNavigation(() => page.goto(url, { waitUntil: 'domcontentloaded' }));
    assert.equal(await page.locator('body').textContent(), 'ready');
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('fresh clients can retain identity without retaining authorized transport history', async () => {
  const browser = await chromium.launch({ headless: true });
  const secret = 'private-response-before-logout';
  const html = `<span id="identity"></span><span id="loaded"></span>
    <button id="login">Login</button><button id="logout">Logout</button><script>
    function identity() { document.querySelector('#identity').textContent =
      (document.cookie.includes('session=owner') ? 'cookie' : 'no-cookie') + '/' +
      (localStorage.getItem('session') === 'owner' ? 'local' : 'no-local') + '/' +
      (sessionStorage.getItem('session') === 'owner' ? 'session' : 'no-session'); }
    async function read() { await (await fetch('/private')).text();
      document.querySelector('#loaded').textContent = 'done'; }
    document.querySelector('#login').onclick = async () => {
      document.cookie = 'session=owner; path=/'; localStorage.setItem('session', 'owner');
      sessionStorage.setItem('session', 'owner');
      identity(); await read(); };
    document.querySelector('#logout').onclick = async () => {
      document.querySelector('#loaded').textContent = '';
      document.cookie = 'session=; Max-Age=0; path=/'; localStorage.removeItem('session');
      sessionStorage.removeItem('session');
      identity(); await read(); };
    identity();</script>`;
  let leaky = false;
  const server = createServer((request, response) => {
    const privateRead = request.url === '/private';
    const authorized = (request.headers.cookie ?? '').includes('session=owner');
    response.setHeader('Content-Type', privateRead ? 'text/plain' : 'text/html');
    response.end(privateRead ? (authorized || leaky ? secret : 'signed out') : html);
  });
  try {
    await new Promise<void>((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', resolve);
    });
    const address = server.address();
    assert(address && typeof address !== 'string');
    const url = `http://127.0.0.1:${address.port}`;
    for (const mode of ['default', 'preserved', 'leaky'] as const) {
      leaky = mode === 'leaky';
      const definition = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
        name: 'fresh storage', features: [{ id: 1, name: 'fresh storage', actors: ['owner'],
          setup: [
            { do: 'click', actor: 'owner', testid: 'login' },
            { do: 'expectReceived', actor: 'owner', contains: secret, within: 1000 },
          ], criteria: [{ id: 'fresh', desc: 'new transport history with the selected identity', points: 1,
            steps: [
              { do: 'freshClient', actor: 'owner', ...(mode === 'default' ? {} : { preserveStorage: true }) },
              { do: 'expect', actor: 'owner-fresh', testid: 'identity',
                contains: mode === 'default' ? 'no-cookie/no-local/no-session' : 'cookie/local/session' },
              { do: 'expectNotReceived', actor: 'owner-fresh', contains: secret, within: 25 },
              { do: 'click', actor: 'owner-fresh', testid: 'logout' },
              { do: 'expect', actor: 'owner-fresh', testid: 'loaded', contains: 'done' },
              { do: 'reload', actor: 'owner-fresh', settleMs: 0 },
              { do: 'expect', actor: 'owner-fresh', testid: 'identity', contains: 'no-cookie/no-local/no-session' },
              { do: 'expectNotReceived', actor: 'owner-fresh', contains: secret, within: 100 },
            ] }] }] });
      const result = await gradeFeature(browser, definition.features[0]!, {
        url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false,
      }, { runId: 'fresh-client', roomName: name => name, url, actions: [], spacetime: null, nullControl: false });
      assert.equal(result.setupEvidence.status, 'passed', result.setupEvidence.summary ?? undefined);
      const evidence = result.criteria[0]!.evidence;
      assert.equal(evidence.status, mode === 'leaky' ? 'failed' : 'passed', evidence.summary ?? undefined);
      if (mode === 'leaky') {
        assert.equal(evidence.actions.length, 8);
        assert.equal(evidence.finding?.kind, 'message-delivered');
      }
    }
    assert.throws(() => compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
      name: 'invalid storage', features: [{ id: 1, name: 'invalid storage', actors: ['owner'],
        criteria: [{ id: 'invalid', desc: 'invalid storage type', points: 1,
          steps: [{ do: 'freshClient', actor: 'owner', preserveStorage: 'yes' }] }] }] }));
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
