import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { createActionRunContext, executeAction } from '../src/actions/action-contract.js';
import {
  ACTOR_TRANSPORT_ACTION_IDS,
  ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
  createNamedActionsCapability,
} from '../src/actions/actor-transport-action-executors.js';

type UnknownRecord = Record<string, unknown>;
type NamedOptions = Parameters<typeof createNamedActionsCapability>[0];
type Calls = ReturnType<NamedOptions['lastCalls']['get']>;
type Verification = readonly ['structural' | 'unverified' | 'verified', string];

interface ServiceOverrides {
  readonly actions?: NamedOptions['actions'];
  readonly appRoot?: string | null;
  readonly backend?: string;
  readonly fetchImpl?: NamedOptions['fetchImpl'];
  readonly spacetime?: unknown;
}

interface ProvidedServices {
  readonly capabilities: Readonly<Record<string, unknown>>;
  readonly calls: Calls;
  readonly verification: Verification[];
}

interface CapturedRequest {
  readonly options: UnknownRecord;
  readonly url: string;
}

const record = (value: unknown): UnknownRecord => {
  assert(value !== null && typeof value === 'object');
  return value as UnknownRecord;
};

const namedResponse = (status: number, ok: boolean) => ({
  status,
  ok,
  text: async (): Promise<string> => '',
});

function services(
  actors: ReadonlyMap<string, unknown>,
  overrides: ServiceOverrides = {},
): ProvidedServices {
  const verification: Verification[] = [];
  let calls: Calls = null;
  const sleep = async (_milliseconds: number, _signal: AbortSignal): Promise<void> => {};
  const browser = {
    defaultWithin: 5000,
    expand: (value: string) => value === '{room:test}' ? 'test-scoped' : value,
    hyphenatedScopedUser: (name: string) => `${name}-scope`,
    roomName: (room: string) => `${room}-scope`,
    scopedUser: (name: string) => `${name}scope`,
    sleep,
    testId: (id: string) => `[data-testid="${id}"]`,
  };
  const named = createNamedActionsCapability({
    actions: overrides.actions ?? [{ id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] }],
    backend: overrides.backend ?? 'postgres',
    url: 'http://app.test',
    spacetime: overrides.spacetime,
    lastCalls: { get: () => calls, set: value => { calls = value; } },
    sleep,
    fetchImpl: overrides.fetchImpl ?? (async () => ({ status: 200, ok: true, text: async () => '' })),
    now: (() => { let value = 10; return () => value++; })(),
  });
  return {
    capabilities: {
      actors: { get: (name: string) => actors.get(name) },
      'application-files': { root: overrides.appRoot ?? null, expand: browser.expand },
      'browser-interaction': browser,
      'named-actions': named,
      subprocess: { sleep },
      'transport-observation': {
        defaultWithin: 5000,
        expand: browser.expand,
        sleep,
        verification: {
          structural: (message: string) => { verification.push(['structural', message]); },
          unverified: (message: string) => { verification.push(['unverified', message]); },
          verified: (message: string) => { verification.push(['verified', message]); },
        },
      },
    },
    get calls() { return calls; },
    verification,
  };
}

async function run(input: UnknownRecord, provided: ProvidedServices) {
  const action = String(input.do);
  return executeAction(ACTION_REGISTRY, action, input, createActionRunContext({
    capabilities: provided.capabilities,
    implementations: ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
    attempt: { id: `test-${action}` },
  }));
}

test('the actor/transport executor registry is exact and capability-scoped', () => {
  assert.deepEqual(Object.keys(ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS).sort(),
    ACTOR_TRANSPORT_ACTION_IDS);
  for (const id of ACTOR_TRANSPORT_ACTION_IDS) {
    const plugin = ACTION_REGISTRY.get(id);
    assert(plugin.deadline.timeoutMs > 0, id);
    assert(plugin.capabilities.length > 0, id);
    assert(plugin.capabilities.every(capability => [
      'actors', 'application-files', 'browser-interaction', 'named-actions', 'subprocess',
      'transport-observation',
    ].includes(capability)), `${id}: ${plugin.capabilities.join(', ')}`);
  }
});

test('one named server action maps DOM input symmetrically and verifies its outcome', async () => {
  const requests: CapturedRequest[] = [];
  const source = {
    name: 'source',
    loc: (testid: string, options: UnknownRecord) => {
      assert.equal(testid, 'item-card');
      assert.deepEqual(options, { contains: 'Desk Lamp' });
      return {
        waitFor: async (value: unknown) =>
          assert.deepEqual(value, { state: 'attached', timeout: 5000 }),
        getAttribute: async (attribute: string) => {
          assert.equal(attribute, 'data-action-input');
          return JSON.stringify({ itemId: 'item-42' });
        },
      };
    },
  };
  const guest = { name: 'guest' };
  const provided = services(new Map<string, unknown>([['source', source], ['guest', guest]]), {
    actions: [{ id: 'buy', path: '/api/items/:item/buy', reducer: 'buy_now', args: [0],
      params: [{ name: 'itemId', in: 'path', placeholder: ':item' }] }],
    fetchImpl: async (url, options) => {
      requests.push({ url, options: options as unknown as UnknownRecord });
      return namedResponse(401, false);
    },
  });

  const called = await run({ do: 'callAction', actor: 'guest', from: 'source', action: 'buy',
    input: { testid: 'item-card', contains: 'Desk Lamp', attribute: 'data-action-input' },
    authentication: 'none', settleMs: 0 }, provided);
  assert.equal(called.status, 'passed');
  assert.deepEqual(called.observation, { action: 'buy', accepted: false, status: 401 });
  const request = requests[0];
  assert(request);
  assert.equal(request.url, 'http://app.test/api/items/item-42/buy');
  assert.deepEqual(JSON.parse(String(request.options.body)), {});
  assert.equal(Object.hasOwn(record(request.options.headers), 'Authorization'), false);

  const checked = await run({ do: 'expectActionOutcome', actor: 'guest', outcome: 'refused' }, provided);
  assert.equal(checked.status, 'passed');
  assert.equal(record(checked.observation).classification, 'verified');
  assert.deepEqual(provided.verification.map(([kind]) => kind), ['verified']);
});

test('named action input is exact and a missing route is not mistaken for a refusal', async () => {
  const actor = (input: UnknownRecord) => ({
    name: 'customer',
    loc: () => ({ waitFor: async () => {}, getAttribute: async () => JSON.stringify(input) }),
  });
  const action: NonNullable<NamedOptions['actions']>[number] = {
    id: 'restock', path: '/api/admin/restock', reducer: 'admin_restock', args: [0, 0, 1],
    params: [{ name: 'itemId', in: 'body' }, { name: 'warehouseId', in: 'body' },
      { name: 'quantity', in: 'body' }],
  };
  const malformed = services(new Map<string, unknown>([
    ['customer', actor({ itemId: 1, warehouseId: 2 })],
  ]),
    { actions: [action] });
  const rejectedInput = await run({ do: 'callAction', actor: 'customer', action: 'restock',
    input: { testid: 'row', attribute: 'data-action-input' }, authentication: 'none' }, malformed);
  assert.equal(rejectedInput.status, 'failed');
  assert.match(rejectedInput.summary ?? '', /must contain exactly/);

  const route = { name: 'route', actionCall: { action: 'restock', accepted: true, status: 200 } };
  const missing = services(new Map<string, unknown>([
    ['customer', actor({ itemId: 1, warehouseId: 2, quantity: 3 })], ['route', route],
  ]), {
    actions: [action], fetchImpl: async () => namedResponse(404, false),
  });
  await run({ do: 'callAction', actor: 'customer', action: 'restock',
    input: { testid: 'row', attribute: 'data-action-input' }, authentication: 'none' }, missing);
  const checked = await run({ do: 'expectActionOutcome', actor: 'customer', outcome: 'refused' }, missing);
  assert.equal(checked.status, 'failed');
  assert.match(checked.summary ?? '', /does not prove/);
  // A 404 names the operation the testing interface requires, so a repair
  // round can create the missing endpoint instead of chasing authorization.
  assert.match(checked.summary ?? '', /reducer `admin_restock`/);
  assert.match(checked.summary ?? '', /POST \/api\/admin\/restock/);

  const privateResource = await run({ do: 'expectActionOutcome', actor: 'customer', outcome: 'refused',
    routeProvenBy: 'route' }, missing);
  assert.equal(privateResource.status, 'passed');
  assert.equal(record(privateResource.observation).status, 404);

  route.actionCall.action = 'different-action';
  const unrelatedProof = await run({ do: 'expectActionOutcome', actor: 'customer', outcome: 'refused',
    routeProvenBy: 'route' }, missing);
  assert.equal(unrelatedProof.status, 'failed');
});

test('an invalid Spacetime u64 input fails before transport and cannot prove refusal', async () => {
  let requests = 0;
  const customer = {
    name: 'customer',
    loc: () => ({ waitFor: async () => {},
      getAttribute: async () => JSON.stringify({ itemId: '-1' }) }),
  };
  const provided = services(new Map<string, unknown>([['customer', customer]]), {
    backend: 'spacetime',
    spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
    actions: [{ id: 'buy', path: '/api/items/:id/buy', reducer: 'buy_now', args: [0],
      params: [{ name: 'itemId', in: 'path', placeholder: ':id', wireType: 'u64' }] }],
    fetchImpl: async () => { requests += 1; return namedResponse(400, false); },
  });
  const called = await run({ do: 'callAction', actor: 'customer', action: 'buy',
    input: { testid: 'item-card', attribute: 'data-buy-input' },
    authentication: 'none', settleMs: 0 }, provided);
  assert.equal(called.status, 'failed');
  assert.match(called.summary ?? '', /invalid u64 value/);
  assert.equal(requests, 0);

  const checked = await run({ do: 'expectActionOutcome', actor: 'customer', outcome: 'refused' }, provided);
  assert.equal(checked.status, 'failed');
  assert.match(checked.summary ?? '', /no callAction ran/);
});

test('account setup preserves scoped credentials and classifies browser failures', async () => {
  const calls: unknown[][] = [];
  const locator = (purpose: string) => ({
    first() { return this; },
    fill: async (value: string) => { calls.push([purpose, 'fill', value]); },
    click: async () => { calls.push([purpose, 'click']); },
    waitFor: async (options: unknown) => { calls.push([purpose, 'waitFor', options]); },
  });
  const actor = { page: { locator: (selector: string) => locator(selector) } };
  const passed = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', actor]])));
  assert.equal(passed.status, 'passed');
  assert.equal(record(passed.observation).user, 'Alicescope');
  assert(calls.some(call => call[2] === 'pw-Alicescope'));

  const timeout = Object.assign(new Error('locator.fill: timed out'), { name: 'TimeoutError' });
  const timedOutActor = { page: { locator: () => ({ first() { return this; },
    fill: async () => { throw timeout; } }) } };
  const timedOut = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', timedOutActor]])));
  assert.equal(timedOut.status, 'failed');
  assert.equal(timedOut.code, 'application_failure');

  const buggyActor = { page: { locator: () => ({ first() { return this; },
    fill: async () => { throw new TypeError('executor bug'); } }) } };
  const bug = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', buggyActor]])));
  assert.equal(bug.status, 'harness_failure');
  assert.equal(bug.code, 'unclassified_exception');
});

test('sign in waits for a rendered toggle instead of silently missing the form', async () => {
  const calls: unknown[][] = [];
  let formVisible = false;
  const username = {
    first() { return this; },
    isVisible: async () => formVisible,
    waitFor: async (options: unknown) => {
      calls.push(['username', 'waitFor', options]);
      assert.equal(formVisible, true);
    },
    fill: async (value: string) => { calls.push(['username', 'fill', value]); },
  };
  const fields: Record<string, unknown> = {
    '[data-testid="signin-username"]': username,
    '[data-testid="signin-password"]': { first() { return this; },
      fill: async (value: string) => { calls.push(['password', 'fill', value]); } },
    '[data-testid="signin-submit"]': { first() { return this; },
      click: async () => { calls.push(['submit', 'click']); } },
    '[data-testid="current-user"]': { first() { return this; },
      waitFor: async (options: unknown) => { calls.push(['current-user', 'waitFor', options]); } },
  };
  const toggle = {
    waitFor: async (options: unknown) => { calls.push(['toggle', 'waitFor', options]); },
    click: async (options: unknown) => {
      calls.push(['toggle', 'click', options]);
      formVisible = true;
    },
  };
  const actor = {
    loc: (id: string) => {
      assert.equal(id, 'signin-toggle');
      return toggle;
    },
    page: { locator: (selector: string) => fields[selector] },
  };

  const result = await run({ do: 'signIn', actor: 'a', name: 'admin', password: 'secret', exact: true },
    services(new Map<string, unknown>([['a', actor]])));

  assert.equal(result.status, 'passed');
  assert.deepEqual(calls.slice(0, 3), [
    ['toggle', 'waitFor', { state: 'visible', timeout: 5000 }],
    ['toggle', 'click', { timeout: 5000 }],
    ['username', 'waitFor', { state: 'visible', timeout: 5000 }],
  ]);
  assert(calls.some(call => call[0] === 'username' && call[2] === 'admin'));
  assert(calls.some(call => call[0] === 'password' && call[2] === 'secret'));
});

test('an unreplayable WebSocket write records structural evidence, not a fabricated rejection', async () => {
  const actor = {
    name: 'a',
    lastWrite: null,
    lastWsWrite: { event: 'send_message', body: { content: 'hello' } },
  };
  const provided = services(new Map<string, unknown>([['a', actor], ['victim', {}]]));
  const forged = await run({ do: 'forgeWrite', actor: 'a', fromActor: 'victim', settleMs: 0 }, provided);
  assert.equal(forged.status, 'passed');
  assert.deepEqual(forged.observation, { attempted: false, classification: 'structural' });

  const checked = await run({ do: 'expectForgeryRejected', actor: 'a' }, provided);
  assert.equal(checked.status, 'passed');
  assert.equal(record(checked.observation).classification, 'structural');
  assert.deepEqual(provided.verification.map(([kind]) => kind), ['structural']);
});

test('missing transport evidence cannot earn server-side forgery credit', async () => {
  const actor = { name: 'a', lastWrite: null, lastWsWrite: null };
  const provided = services(new Map<string, unknown>([['a', actor], ['victim', {}]]));
  const forged = await run({ do: 'forgeWrite', actor: 'a', fromActor: 'victim', settleMs: 0 },
    provided);
  assert.equal(forged.status, 'passed');
  assert.equal(record(forged.observation).classification, 'unverified');

  const checked = await run({ do: 'expectForgeryRejected', actor: 'a' }, provided);
  assert.equal(checked.status, 'inconclusive');
  assert.match(checked.summary ?? '', /could not verify the server-side forgery refusal/);
});

test('replay retargeting maps nested entity ids by field and relationship depth', async () => {
  const requests: CapturedRequest[] = [];
  const actor = (name: string, received: unknown[], writes: UnknownRecord[]) => ({
    name,
    received: received.map(value => JSON.stringify(value)),
    writes,
    page: {
      request: { fetch: async (url: string, options: UnknownRecord) => {
        requests.push({ url, options });
        return { status: () => 200, ok: () => true };
      } },
    },
  });
  const staff = actor('staff', [{ orders: [{
    _id: 'order-desk', userId: 'staff-user',
    items: [{ itemId: 'item-desk', name: 'Desk Lamp' }],
  }] }], [{
    url: 'http://app.test/api/fulfilment/order-desk/ship', method: 'POST',
    headers: { authorization: 'Bearer staff-token' }, body: null,
  }]);
  const customer = actor('customer', [{ order: {
    _id: 'order-webcam', userId: 'customer-user',
    items: [{ itemId: 'item-webcam', name: 'Webcam' }],
  } }], [{
    url: 'http://app.test/api/items/item-webcam/buy', method: 'POST',
    headers: { authorization: 'Bearer customer-token' }, body: null,
  }]);
  const provided = services(new Map<string, unknown>([
    ['staff', staff],
    ['customer', customer],
  ]));
  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    swap: { find: 'Desk Lamp', with: 'Webcam' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: true, status: 200 });
  const request = requests[0];
  assert(request);
  assert.equal(request.url, 'http://app.test/api/fulfilment/order-webcam/ship');
  assert.equal(record(request.options.headers).authorization, 'Bearer customer-token');

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'failed');
  assert.match(rejected.summary ?? '', /server ACCEPTED/);
});

test('replay decodes Socket.IO entities and uses the target actor browser cookie', async () => {
  const requests: CapturedRequest[] = [];
  const actor = (name: string, received: string[], writes: UnknownRecord[], sid: string) => ({
    name,
    received,
    writes,
    context: { cookies: async () => [{ name: 'sid', value: sid }] },
    page: {
      evaluate: async () => null,
      request: { fetch: async (url: string, options: UnknownRecord) => {
        requests.push({ url, options });
        return { status: () => 403, ok: () => false };
      } },
    },
  });
  const staff = actor('staff', [JSON.stringify({ queue: [{
    id: 41, items: [{ itemId: 7, name: 'Desk Lamp' }],
  }] })], [{
    url: 'http://app.test/api/fulfilment/ship', method: 'POST',
    headers: { 'content-type': 'application/json' }, body: { orderId: 41 },
  }], 'staff-session');
  const customer = actor('customer', [
    JSON.stringify({ items: [{ id: 8, name: 'Webcam' }] }),
    `42["orders:update",${JSON.stringify({ orders: [{
      id: 52, items: [{ orderItemId: 61, itemId: 8, name: 'Webcam' }],
    }] })}]`,
  ], [{
    url: 'http://app.test/api/items/8/buy', method: 'POST',
    headers: { 'content-type': 'application/json' }, body: null,
  }], 'customer-session');
  const provided = services(new Map<string, unknown>([
    ['staff', staff],
    ['customer', customer],
  ]));

  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    swap: { find: 'Desk Lamp', with: 'Webcam' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: false, status: 403 });
  const request = requests[0];
  assert(request);
  assert.deepEqual(JSON.parse(String(request.options.data)), { orderId: 52 });
  assert.match(String(record(request.options.headers).Cookie), /sid=customer-session/);

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'passed');
  assert.equal(record(rejected.observation).classification, 'verified');
});

test('replay uses an authenticated named action when the source write is an opaque WebSocket call', async () => {
  const requests: CapturedRequest[] = [];
  const source = { name: 'staff', writes: [], received: [],
    lastWsWrite: { event: 'binary reducer call', body: {} },
    loc: (testid: string, options: UnknownRecord) => {
      assert.equal(testid, 'order-item');
      assert.deepEqual(options, { contains: 'Webcam' });
      return {
        waitFor: async (value: unknown) =>
          assert.deepEqual(value, { state: 'visible', timeout: 5000 }),
        getAttribute: async (attribute: string) => {
          assert.equal(attribute, 'data-entity-id');
          return '52';
        },
      };
    } };
  const customer = {
    name: 'customer', writes: [], received: [],
    context: { cookies: async () => [] },
    page: { evaluate: async () => 'eyJcustomer.token.value' },
  };
  const provided = services(new Map<string, unknown>([
    ['staff', source],
    ['customer', customer],
  ]), {
    backend: 'spacetime',
    actions: [{ id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0] }],
    spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
    fetchImpl: async (url, options) => {
      requests.push({ url, options: options as unknown as UnknownRecord });
      return namedResponse(530, false);
    },
  });

  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    swap: { find: '52', with: '53' },
    namedAction: { id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0] },
    namedTarget: { testid: 'order-item', contains: 'Webcam',
      attribute: 'data-entity-id', valueType: 'number' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: false, status: 530, namedAction: 'ship' });
  const request = requests[0];
  assert(request);
  assert.equal(request.url, 'http://127.0.0.1:3000/v1/database/shop/call/ship_order');
  assert.equal(request.options.body, '[53]');
  assert.equal(record(request.options.headers).Authorization, 'Bearer eyJcustomer.token.value');

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'passed');
  assert.equal(record(rejected.observation).classification, 'verified');
});

test('a redirect, transport failure, or undeclared server error is not authorization evidence', async () => {
  for (const status of [0, 302, 503]) {
    const actor = {
      name: 'customer',
      replay: { accepted: false, status, method: 'POST', url: '/ship' },
    };
    const provided = services(new Map<string, unknown>([['customer', actor]]));
    const checked = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
    assert.equal(checked.status, 'failed');
    assert.match(checked.summary ?? '', /does not prove an authorization refusal/);
    assert.equal(provided.verification.length, 0);
  }
});

test('a missing numeric literal makes the server-side replay check inconclusive', async () => {
  const requests: unknown[][] = [];
  const buyer = {
    name: 'buyer',
    received: [JSON.stringify({ items: [{ _id: 'item-espresso', name: 'Espresso Machine', price: 449 }] })],
    writes: [{
      url: 'http://app.test/api/items/item-espresso/buy', method: 'POST',
      headers: { authorization: 'Bearer buyer-token' }, body: null,
    }],
    page: { request: { fetch: async (...args: unknown[]) => { requests.push(args); } } },
  };
  const provided = services(new Map<string, unknown>([['buyer', buyer]]));
  const replayed = await run({ do: 'replayAs', actor: 'buyer', from: 'buyer', match: 'buy',
    swap: { find: '449', with: '1' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'inconclusive');
  assert.match(replayed.summary ?? '', /could not issue the server-side replay/);
  assert.equal(requests.length, 0);
});

test('named calls preserve actor credentials, result state, and application assertions', async () => {
  const requests: CapturedRequest[] = [];
  const actor = (name: string) => ({
    name,
    context: { cookies: async () => [{ name: 'sid', value: name }] },
    page: { evaluate: async () => null },
  });
  const provided = services(new Map<string, unknown>([
    ['a', actor('a')],
    ['b', actor('b')],
  ]), {
    fetchImpl: async (url, options) => {
      requests.push({ url, options: options as unknown as UnknownRecord });
      return { status: 200, ok: true, text: async () => '' };
    },
  });
  const called = await run({ do: 'callConcurrently', actors: ['a', 'b'],
    action: 'checkout', settleMs: 0 }, provided);
  assert.equal(called.status, 'passed');
  assert.equal(record(called.observation).fired, 2);
  assert.equal(requests.length, 2);
  const firstRequest = requests[0];
  assert(firstRequest);
  assert.match(String(record(firstRequest.options.headers).Cookie), /sid=a/);
  assert.equal(provided.calls?.action, 'checkout');

  const accepted = await run({ do: 'expectCallOutcomes', accepted: 2 }, provided);
  assert.equal(accepted.status, 'passed');
  const mismatch = await run({ do: 'expectCallOutcomes', accepted: 1 }, provided);
  assert.equal(mismatch.status, 'failed');
  assert.match(mismatch.summary ?? '', /expected exactly 1/);
});

test('named calls accept opaque bearer tokens stored under an explicit token key', async () => {
  const requests: CapturedRequest[] = [];
  const storage = (entries: ReadonlyArray<readonly [string, string]>) => ({
    length: entries.length,
    key: (index: number) => entries[index]?.[0] ?? null,
    getItem: (key: string) =>
      entries.find(([candidate]) => candidate === key)?.[1] ?? null,
  });
  const actor = (name: string) => ({
    name,
    context: { cookies: async () => [] },
    page: { evaluate: async (browserFunction: () => unknown) => Function('localStorage', 'sessionStorage',
      `return (${browserFunction.toString()})()`)(
      storage([['theme', 'dark'], ['pgshop_token', `${name}-opaque-session-token-value`]]),
      storage([])) },
  });
  const provided = services(new Map<string, unknown>([
    ['a', actor('a')],
    ['b', actor('b')],
  ]), {
    fetchImpl: async (url, options) => {
      requests.push({ url, options: options as unknown as UnknownRecord });
      return { status: 200, ok: true, text: async () => '' };
    },
  });
  const called = await run({ do: 'callConcurrently', actors: ['a', 'b'],
    action: 'checkout', settleMs: 0 }, provided);
  assert.equal(called.status, 'passed');
  assert.equal(requests.length, 2);
  const firstRequest = requests[0];
  const secondRequest = requests[1];
  assert(firstRequest && secondRequest);
  assert.equal(record(firstRequest.options.headers).Authorization,
    'Bearer a-opaque-session-token-value');
  assert.equal(record(secondRequest.options.headers).Authorization,
    'Bearer b-opaque-session-token-value');
});

test('missing named actions and application roots stay inconclusive', async () => {
  const actor = { context: { cookies: async () => [{ name: 'sid', value: 'a' }] },
    page: { evaluate: async () => null } };
  const missingAction = await run({ do: 'callConcurrently', actors: ['a', 'a'],
    action: 'missing', settleMs: 0 },
  services(new Map<string, unknown>([['a', actor]]), { actions: [] }));
  assert.equal(missingAction.status, 'inconclusive');
  assert.match(missingAction.summary ?? '', /track names no action/);

  const missingRoot = await run({ do: 'runScript', script: 'backoffice.mjs', args: [] },
    services(new Map<string, unknown>()));
  assert.equal(missingRoot.status, 'inconclusive');
  assert.match(missingRoot.summary ?? '', /app directory/);
});

test('an application-owned script timeout is a scored application failure', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-script-timeout-'));
  try {
    writeFileSync(join(root, 'slow.mjs'), 'await new Promise(resolve => setTimeout(resolve, 10000));\n');
    const result = await run({ do: 'runScript', script: 'slow.mjs', args: [], timeoutMs: 20 },
      services(new Map<string, unknown>(), { appRoot: root }));
    assert.equal(result.status, 'failed');
    assert.match(result.summary ?? '', /failed|timed out/i);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
