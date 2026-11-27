import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createNamedActionsCapability } from '../src/actions/named-action-runtime.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('review eligibility sends the claimed username with the nonbuyer credentials', async t => {
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const context = await browser.newContext();
  await context.route('http://app.test/**', route => route.fulfill({ contentType: 'text/html',
    body: `<div data-role="item-card" data-buy-input='{"itemId":"9007199254740993"}'>Keyboard</div>` }));
  const page = await context.newPage();
  await page.goto('http://app.test/');
  const actors = new Map(['owner', 'stranger'].map(name => [name, {
    name, page, context, record() {},
    writes: ['http://app.test/api/session', 'http://native.test/v1/database/shop/call/sign_in']
      .map(url => ({ url, headers: { authorization: `Bearer ${name}` } })),
    loc: () => page.locator('[data-role="item-card"]'),
  }]));
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-review-access.json'), 'utf8')));
  const steps = scenario.features[0]!.criteria[0]!.steps;
  const calls = steps.filter(step => step.do === 'callAction');
  assert.equal(calls.length, 3);
  for (const backend of ['postgres', 'mongodb', 'spacetime']) {
    const requests: Array<{ url: string; body: string; authorization: string | undefined }> = [];
    const capabilities = {
      actors: { get: (name: string) => actors.get(name) },
      'transport-observation': { defaultWithin: 1000, sleep: async () => {},
        verification: { verified() {}, unverified() {} } },
      'named-actions': createNamedActionsCapability({ backend, url: 'http://app.test',
        spacetime: { uri: 'http://native.test', mod: 'shop' },
        lastCalls: { get: () => null, set() {} }, sleep: async () => {},
        fetchImpl: async (url, options) => {
          requests.push({ url, body: options.body!, authorization: options.headers?.authorization });
          return { status: 200, ok: true, text: async () => '' };
        } }),
    };
    for (const step of calls) {
      const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities });
      assert.equal(result.status, 'passed', result.summary ?? step.do);
    }
    assert.deepEqual(requests.map(request => request.authorization),
      ['Bearer owner', 'Bearer stranger', 'Bearer stranger']);
    const attack = requests[2]!;
    if (backend === 'spacetime') {
      assert.equal(attack.url, 'http://native.test/v1/database/shop/call/submit_review');
      assert.equal(attack.body, '[9007199254740993,5,"Forged buyer review","claim-review-owner"]');
      assert.equal(requests[0]!.body, '[9007199254740993,5,"Original buyer review"]');
    } else {
      assert.equal(attack.url, 'http://app.test/api/items/9007199254740993/reviews');
      assert.deepEqual(JSON.parse(attack.body), {
        rating: 5, comment: 'Forged buyer review', username: 'claim-review-owner',
      });
      assert.deepEqual(JSON.parse(requests[0]!.body), { rating: 5, comment: 'Original buyer review' });
    }
  }
});

test('named action inputs wait for browser initialization without sending incomplete requests', async t => {
  // A row can render before its input arrives. Invalid, missing, late, or
  // cancelled input must still send no request, including concurrent groups.
  const evidence: unknown[] = [];
  t.after(() => {
    const file = process.env.STACK_BENCH_NAMED_INPUT_EVIDENCE;
    if (file) {
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({
        rerun: 'STACK_BENCH_NAMED_INPUT_EVIDENCE=<file> node --test --test-name-pattern="named action inputs wait" dist/tests/review-owner.integration.js',
        fixture: 'Browser row renders before a held HTTP input response; registered actions use real HTTP writes.',
        evidence,
      }, null, 2) + '\n');
    }
  });
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  for (const mode of ['primary', 'override', 'concurrent', 'alongside', 'missing', 'malformed', 'late', 'cancelled']) {
    await t.test(mode, async () => {
      let release!: () => void;
      const held = new Promise<void>(resolve => { release = resolve; });
      let started!: () => void;
      const hydrating = new Promise<void>(resolve => { started = resolve; });
      const requests: Array<{ path: string; authorization: string | undefined }> = [];
      const server = createServer((request, response) => {
        if (request.url === '/hydrate') {
          started();
          void held.then(() => { response.setHeader('Content-Type', 'application/json'); response.end('{}'); });
        } else if (request.method === 'POST') {
          requests.push({ path: request.url!, authorization: request.headers.authorization });
          response.setHeader('Content-Type', 'application/json'); response.end('{}');
        } else {
          response.setHeader('Content-Type', 'text/html');
          response.end(`<div data-role="order-item" ${mode === 'override' ? `data-cancel-input='{"orderId":"101"}'` : ''}>Desk Lamp</div>
            <div data-role="other-order">Keyboard</div><script>
            fetch('/hydrate').then(() => {
              const row = document.querySelector('[data-role="${mode === 'override' ? 'other-order' : 'order-item'}"]');
              ${mode === 'missing' ? '' : `row.setAttribute('${mode === 'override' ? 'data-entity-id' : 'data-cancel-input'}',
                ${JSON.stringify(mode === 'override' ? '9007199254740993' : mode === 'malformed' ? '{broken' : '{"orderId":"9007199254740993"}')});`}
            });</script>`);
        }
      });
      await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
      const address = server.address(); assert(address && typeof address !== 'string');
      const url = `http://127.0.0.1:${address.port}`;
      const context = await browser.newContext();
      try {
        const page = await context.newPage();
        await page.goto(url, { waitUntil: 'domcontentloaded' });
        await hydrating;
        const actors = new Map(['owner', 'other'].map(name => [name, {
          name, page, context, record() {}, writes: [{ url, headers: { authorization: `Bearer ${name}` } }],
          loc: (id: string) => page.locator(`[data-role="${id}"]`),
        }]));
        const sleep = (ms: number, signal: AbortSignal) => delay(ms, undefined, { signal });
        const capabilities = {
          actors: { get: (name: string) => actors.get(name) },
          'transport-observation': { defaultWithin: 300, sleep, verification: { verified() {}, unverified() {} } },
          'named-actions': createNamedActionsCapability({ backend: 'mongodb', url,
            lastCalls: { get: () => null, set() {} }, sleep }),
        };
        const namedAction = { id: 'cancel', path: '/api/orders/:id/cancel', reducer: 'cancel_order', args: [0],
          params: [{ name: 'orderId', in: 'path', placeholder: ':id', wireType: 'u64' }] };
        const input = { testid: 'order-item', contains: 'Desk Lamp', attribute: 'data-cancel-input',
          ...(mode === 'override' ? { overrides: { orderId: {
            actor: 'owner', testid: 'other-order', attribute: 'data-entity-id',
          } } } : {}) };
        const group = { action: 'cancel', namedAction, input, from: 'owner', actors: ['other'], requests: 1, requestTimeoutMs: 300 };
        const step = mode === 'alongside'
          ? { do: 'callConcurrently', action: 'cancel', actors: ['other'], requests: 1, settleMs: 0,
            namedAction: { ...namedAction, params: [], args: [], path: '/control' }, alongside: [group] }
          : mode === 'concurrent' ? { do: 'callConcurrently', ...group, settleMs: 0 }
          : { do: 'callAction', actor: 'other', from: 'owner', action: 'cancel', namedAction, input, settleMs: 0 };
        const controller = new AbortController();
        const pending = executeAction(ACTION_REGISTRY, step.do, step, { capabilities, signal: controller.signal });
        await delay(75);
        if (mode === 'cancelled') controller.abort('fixture cancellation');
        if (mode !== 'late' && mode !== 'cancelled') release();
        const result = await pending;
        if (mode === 'late') { release(); await delay(30); }
        const expected = ['primary', 'override', 'concurrent', 'alongside'].includes(mode) ? 'passed'
          : mode === 'cancelled' ? 'inconclusive' : 'failed';
        evidence.push({ mode, expected, result, requests });
        assert.equal(result.status, expected, result.summary ?? mode);
        if (expected === 'passed') {
          const byPath = (a: { path: string }, b: { path: string }) => a.path.localeCompare(b.path);
          assert.deepEqual(requests.toSorted(byPath), [
            ...(mode === 'alongside' ? [{ path: '/control', authorization: 'Bearer other' }] : []),
            { path: '/api/orders/9007199254740993/cancel', authorization: 'Bearer other' },
          ].toSorted(byPath));
        } else {
          assert.equal(requests.length, 0, 'unready input must never dispatch a write');
          if (mode !== 'cancelled') assert.equal(result.finding?.kind,
            mode === 'malformed' ? 'interface-invalid' : 'interface-missing');
        }
      } finally {
        release(); await context.close(); server.closeAllConnections();
        await new Promise<void>(resolve => server.close(() => resolve()));
      }
    });
  }
});
