import assert from 'node:assert/strict';
import test from 'node:test';

import { ACTION_REGISTRY } from '../action-catalog.mjs';
import { createActionRunContext, executeAction } from '../action-contract.mjs';
import {
  ACTOR_TRANSPORT_ACTION_IDS,
  ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
  createNamedActionsCapability,
} from '../actor-transport-action-executors.mjs';

function services(actors, overrides = {}) {
  const verification = [];
  let calls = null;
  const sleep = async () => {};
  const browser = {
    defaultWithin: 5000,
    expand: value => value === '{room:test}' ? 'test-scoped' : value,
    legacyScopedUser: name => `${name}-scope`,
    roomName: room => `${room}-scope`,
    scopedUser: name => `${name}scope`,
    sleep,
    testId: id => `[data-testid="${id}"]`,
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
      actors: { get: name => actors.get(name) },
      'application-files': { root: overrides.appRoot ?? null, expand: browser.expand },
      'browser-interaction': browser,
      'named-actions': named,
      subprocess: { sleep },
      'transport-observation': {
        defaultWithin: 5000,
        expand: browser.expand,
        sleep,
        verification: {
          structural: message => verification.push(['structural', message]),
          unverified: message => verification.push(['unverified', message]),
          verified: message => verification.push(['verified', message]),
        },
      },
    },
    get calls() { return calls; },
    verification,
  };
}

async function run(input, provided) {
  return executeAction(ACTION_REGISTRY, input.do, input, createActionRunContext({
    capabilities: provided.capabilities,
    implementations: ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
    attempt: { id: `test-${input.do}` },
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

test('account setup preserves scoped credentials and classifies browser failures', async () => {
  const calls = [];
  const locator = purpose => ({
    first() { return this; },
    fill: async value => calls.push([purpose, 'fill', value]),
    click: async () => calls.push([purpose, 'click']),
    waitFor: async options => calls.push([purpose, 'waitFor', options]),
  });
  const actor = { page: { locator: selector => locator(selector) } };
  const passed = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map([['a', actor]])));
  assert.equal(passed.status, 'passed');
  assert.equal(passed.observation.user, 'Alicescope');
  assert(calls.some(call => call[2] === 'pw-Alicescope'));

  const timeout = Object.assign(new Error('locator.fill: timed out'), { name: 'TimeoutError' });
  const timedOutActor = { page: { locator: () => ({ first() { return this; },
    fill: async () => { throw timeout; } }) } };
  const timedOut = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map([['a', timedOutActor]])));
  assert.equal(timedOut.status, 'failed');
  assert.equal(timedOut.code, 'application_failure');

  const buggyActor = { page: { locator: () => ({ first() { return this; },
    fill: async () => { throw new TypeError('executor bug'); } }) } };
  const bug = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map([['a', buggyActor]])));
  assert.equal(bug.status, 'harness_failure');
  assert.equal(bug.code, 'unclassified_exception');
});

test('an unreplayable WebSocket write records structural evidence, not a fabricated rejection', async () => {
  const actor = {
    name: 'a',
    lastWrite: null,
    lastWsWrite: { event: 'send_message', body: { content: 'hello' } },
  };
  const provided = services(new Map([['a', actor], ['victim', {}]]));
  const forged = await run({ do: 'forgeWrite', actor: 'a', fromActor: 'victim', settleMs: 0 }, provided);
  assert.equal(forged.status, 'passed');
  assert.deepEqual(forged.observation, { attempted: false, classification: 'structural' });

  const checked = await run({ do: 'expectForgeryRejected', actor: 'a' }, provided);
  assert.equal(checked.status, 'passed');
  assert.equal(checked.observation.classification, 'structural');
  assert.deepEqual(provided.verification.map(([kind]) => kind), ['structural']);
});

test('replay retargeting maps nested entity ids by field and relationship depth', async () => {
  const requests = [];
  const actor = (name, received, writes) => ({
    name,
    received: received.map(value => JSON.stringify(value)),
    writes,
    page: {
      request: { fetch: async (url, options) => {
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
  const provided = services(new Map([['staff', staff], ['customer', customer]]));
  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    swap: { find: 'Desk Lamp', with: 'Webcam' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: true, status: 200 });
  assert.equal(requests[0].url, 'http://app.test/api/fulfilment/order-webcam/ship');
  assert.equal(requests[0].options.headers.authorization, 'Bearer customer-token');

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'failed');
  assert.match(rejected.summary, /server ACCEPTED/);
});

test('replay decodes Socket.IO entities and uses the target actor browser cookie', async () => {
  const requests = [];
  const actor = (name, received, writes, sid) => ({
    name,
    received,
    writes,
    context: { cookies: async () => [{ name: 'sid', value: sid }] },
    page: {
      evaluate: async () => null,
      request: { fetch: async (url, options) => {
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
  const provided = services(new Map([['staff', staff], ['customer', customer]]));

  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    swap: { find: 'Desk Lamp', with: 'Webcam' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: false, status: 403 });
  assert.deepEqual(JSON.parse(requests[0].options.data), { orderId: 52 });
  assert.match(requests[0].options.headers.Cookie, /sid=customer-session/);

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'passed');
  assert.equal(rejected.observation.classification, 'verified');
});

test('replay uses an authenticated named action when the source write is an opaque WebSocket call', async () => {
  const requests = [];
  const source = { name: 'staff', writes: [], received: [],
    lastWsWrite: { event: 'binary reducer call', body: {} } };
  const customer = {
    name: 'customer', writes: [], received: [],
    loc: (testid, options) => {
      assert.equal(testid, 'order-item');
      assert.deepEqual(options, { contains: 'Webcam' });
      return {
        waitFor: async value => assert.deepEqual(value, { state: 'visible', timeout: 5000 }),
        getAttribute: async attribute => {
          assert.equal(attribute, 'data-entity-id');
          return '52';
        },
      };
    },
    context: { cookies: async () => [] },
    page: { evaluate: async () => 'eyJcustomer.token.value' },
  };
  const provided = services(new Map([['staff', source], ['customer', customer]]), {
    backend: 'spacetime',
    actions: [{ id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0] }],
    spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
    fetchImpl: async (url, options) => {
      requests.push({ url, options });
      return { status: 530, ok: false };
    },
  });

  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
    namedAction: { id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0] },
    namedTarget: { testid: 'order-item', contains: 'Webcam',
      attribute: 'data-entity-id', valueType: 'number' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation,
    { attempted: true, accepted: false, status: 530, namedAction: 'ship' });
  assert.equal(requests[0].url, 'http://127.0.0.1:3000/v1/database/shop/call/ship_order');
  assert.equal(requests[0].options.body, '[52]');
  assert.equal(requests[0].options.headers.Authorization, 'Bearer eyJcustomer.token.value');

  const rejected = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
  assert.equal(rejected.status, 'passed');
  assert.equal(rejected.observation.classification, 'verified');
});

test('a redirect, transport failure, or undeclared server error is not authorization evidence', async () => {
  for (const status of [0, 302, 503]) {
    const actor = { name: 'customer', replay: { accepted: false, status, method: 'POST', url: '/ship' } };
    const provided = services(new Map([['customer', actor]]));
    const checked = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
    assert.equal(checked.status, 'failed');
    assert.match(checked.summary, /does not prove an authorization refusal/);
    assert.equal(provided.verification.length, 0);
  }
});

test('a missing numeric literal is not fabricated into an entity-id retarget', async () => {
  const requests = [];
  const buyer = {
    name: 'buyer',
    received: [JSON.stringify({ items: [{ _id: 'item-espresso', name: 'Espresso Machine', price: 449 }] })],
    writes: [{
      url: 'http://app.test/api/items/item-espresso/buy', method: 'POST',
      headers: { authorization: 'Bearer buyer-token' }, body: null,
    }],
    page: { request: { fetch: async (...args) => { requests.push(args); } } },
  };
  const provided = services(new Map([['buyer', buyer]]));
  const replayed = await run({ do: 'replayAs', actor: 'buyer', from: 'buyer', match: 'buy',
    swap: { find: '449', with: '1' }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.deepEqual(replayed.observation, { attempted: false });
  assert.equal(requests.length, 0);

  const checked = await run({ do: 'expectReplayRejected', actor: 'buyer' }, provided);
  assert.equal(checked.status, 'passed');
  assert.equal(checked.observation.classification, 'unverified');
  assert.match(provided.verification[0][1], /request has no value to edit/);
});

test('named calls preserve actor credentials, result state, and application assertions', async () => {
  const requests = [];
  const actor = name => ({
    name,
    context: { cookies: async () => [{ name: 'sid', value: name }] },
    page: { evaluate: async () => null },
  });
  const provided = services(new Map([['a', actor('a')], ['b', actor('b')]]), {
    fetchImpl: async (url, options) => {
      requests.push({ url, options });
      return { status: 200, ok: true, text: async () => '' };
    },
  });
  const called = await run({ do: 'callConcurrently', actors: ['a', 'b'],
    action: 'checkout', settleMs: 0 }, provided);
  assert.equal(called.status, 'passed');
  assert.equal(called.observation.fired, 2);
  assert.equal(requests.length, 2);
  assert.match(requests[0].options.headers.Cookie, /sid=a/);
  assert.equal(provided.calls.action, 'checkout');

  const accepted = await run({ do: 'expectCallOutcomes', accepted: 2 }, provided);
  assert.equal(accepted.status, 'passed');
  const mismatch = await run({ do: 'expectCallOutcomes', accepted: 1 }, provided);
  assert.equal(mismatch.status, 'failed');
  assert.match(mismatch.summary, /expected exactly 1/);
});

test('missing named actions and application roots stay inconclusive', async () => {
  const actor = { context: { cookies: async () => [{ name: 'sid', value: 'a' }] },
    page: { evaluate: async () => null } };
  const missingAction = await run({ do: 'callConcurrently', actors: ['a', 'a'],
    action: 'missing', settleMs: 0 }, services(new Map([['a', actor]]), { actions: [] }));
  assert.equal(missingAction.status, 'inconclusive');
  assert.match(missingAction.summary, /track names no action/);

  const missingRoot = await run({ do: 'runScript', script: 'backoffice.mjs', args: [] },
    services(new Map()));
  assert.equal(missingRoot.status, 'inconclusive');
  assert.match(missingRoot.summary, /app directory/);
});
