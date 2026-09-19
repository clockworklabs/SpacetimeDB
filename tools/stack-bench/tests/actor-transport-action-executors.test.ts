import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { classifyResponseContract } from '../src/actions/named-action-runtime.js';
import { executeAction } from '../src/actions/action-contract.js';
import {
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
  readonly sleep?: (milliseconds: number, signal: AbortSignal) => Promise<void>;
  readonly spacetime?: { uri: string; mod: string };
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
  for (const value of actors.values()) {
    const actor = record(value);
    actor.record ??= () => {};
    actor.context ??= { cookies: async () => [] };
    for (const write of (actor.writes ?? []) as UnknownRecord[]) {
      write.url ??= `${overrides.backend === 'spacetime' ? overrides.spacetime?.uri : 'http://app.test'}/api/session`;
    }
  }
  const verification: Verification[] = [];
  let calls: Calls = null;
  const sleep = overrides.sleep
    ?? (async (_milliseconds: number, _signal: AbortSignal): Promise<void> => {});
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
  return executeAction(ACTION_REGISTRY, action, input, {
    capabilities: provided.capabilities,
  });
}

test('the actor/transport executor registry is exact and capability-scoped', () => {
  for (const id of Object.keys(ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS)) {
    const plugin = ACTION_REGISTRY.get(id);
    assert(plugin.timeoutMs > 0, id);
    assert(plugin.capabilities.length > 0, id);
    assert(plugin.capabilities.every(capability => [
      'actors', 'application-files', 'browser-interaction', 'named-actions', 'subprocess',
      'transport-observation',
    ].includes(capability)), `${id}: ${plugin.capabilities.join(', ')}`);
  }
});

test('a parameterless action needs no DOM input; parameterized actions still do', async () => {
  let calls = 0;
  const provided = services(new Map([['guest', { name: 'guest' }]]), {
    fetchImpl: async (url, options) => {
      assert.equal(url, 'http://app.test/api/checkout');
      assert.deepEqual(JSON.parse(String(options.body)), {});
      calls++;
      return namedResponse(200, true);
    },
  });
  const input = { do: 'callAction', actor: 'guest', action: 'checkout', authentication: 'none',
    namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] } };
  const result = await run(input, provided);
  assert.equal(result.status, 'passed', JSON.stringify(result));
  assert.equal((await run({ ...input, namedAction: { ...input.namedAction, args: [1] } }, provided)).status,
    'harness_failure');
  assert.equal(calls, 1);
});

test('optional actor credentials preserve illicit sessions without excusing broken credential hooks', async () => {
  const input = { do: 'callAction', actor: 'guest', action: 'checkout', authentication: 'optional', settleMs: 0,
    namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] } };
  for (const observed of [{ signedOut: true }, ['illicit-session'], ['one-token', 'another-token'], { unavailable: 'credential hook failed' }]) {
    let calls = 0;
    const actor = { name: 'guest', writes: [{ url: 'http://app.test/api/checkout', headers: { 'x-csrf-token': 'caller-csrf' } }],
      record() {}, context: { cookies: async () => [] },
      page: { evaluate: async () => observed } };
    const provided = services(new Map([['guest', actor]]), {
      fetchImpl: async (_url, options) => {
        calls++;
        assert.equal((options.headers as Record<string, string>)['x-csrf-token'], 'caller-csrf');
        assert.equal((options.headers as Record<string, string>).Authorization,
          Array.isArray(observed) ? 'Bearer illicit-session' : undefined);
        return namedResponse(401, false);
      },
    });
    const result = await run(input, provided);
    const broken = Array.isArray(observed) ? observed.length > 1 : 'unavailable' in observed;
    assert.equal(result.status, broken ? 'inconclusive' : 'passed', JSON.stringify(result));
    assert.equal(calls, broken ? 0 : 1);
  }
});

test('shipping accounting waits for the staff response before reading accounting', async () => {
  const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-shipping-accounting.json'), 'utf8'));
  const steps = scenario.features[0].criteria[0].steps as UnknownRecord[];
  const index = steps.findIndex(step => step.do === 'callAction' && step.action === 'ship');
  assert(index >= 0);
  let release!: () => void;
  const response = new Promise<void>(resolve => { release = resolve; });
  let submitted!: () => void;
  const started = new Promise<void>(resolve => { submitted = resolve; });
  const staff = { name: 'staff', writes: [{ headers: { authorization: 'Bearer staff' } }] };
  const customer = { name: 'customer', loc: () => ({ waitFor: async () => {},
    getAttribute: async () => JSON.stringify({ orderId: '42' }) }) };
  const provided = services(new Map<string, unknown>([['staff', staff], ['customer', customer]]), {
    fetchImpl: async (url, options) => {
      assert.equal(url, 'http://app.test/api/fulfilment/ship');
      assert.equal(options.headers?.authorization, 'Bearer staff');
      assert.deepEqual(JSON.parse(String(options.body)), { orderId: '42' });
      submitted();
      await response;
      return namedResponse(200, true);
    },
  });
  let finished = false;
  const pending = run(steps[index]!, provided).then(result => { finished = true; return result; });
  await started;
  assert.equal(finished, false, 'the action cannot finish while its response is pending');
  release();
  assert.equal((await pending).status, 'passed');
  assert.equal(steps[index + 1]!.do, 'expectActionOutcome');
  assert.equal((await run(steps[index + 1]!, provided)).status, 'passed');
  assert.equal(steps[index + 2]!.do, 'reload');
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

test('named action input uses declared defaults and a missing route is not mistaken for a refusal', async () => {
  const actor = (input: UnknownRecord) => ({
    name: 'customer',
    loc: () => ({ waitFor: async () => {}, getAttribute: async () => JSON.stringify(input) }),
  });
  const action: NonNullable<NamedOptions['actions']>[number] = {
    id: 'restock', path: '/api/admin/restock', reducer: 'admin_restock', args: [0, 0, 1],
    params: [{ name: 'itemId', in: 'body' }, { name: 'warehouseId', in: 'body' },
      { name: 'quantity', in: 'body' }],
  };
  const withDefault = services(new Map<string, unknown>([
    ['customer', actor({ itemId: 1, warehouseId: 2 })],
  ]), { actions: [action], fetchImpl: async (_url, options) => {
    assert.deepEqual(JSON.parse(String(options.body)), { itemId: 1, warehouseId: 2, quantity: 1 });
    return namedResponse(200, true);
  } });
  const calledWithDefault = await run({ do: 'callAction', actor: 'customer', action: 'restock',
    input: { testid: 'row', attribute: 'data-action-input' }, authentication: 'none' }, withDefault);
  assert.equal(calledWithDefault.status, 'passed');

  const unexpected = services(new Map<string, unknown>([
    ['customer', actor({ itemId: 1, warehouseId: 2, quantity: 3, extra: true })],
  ]), { actions: [action] });
  const rejectedInput = await run({ do: 'callAction', actor: 'customer', action: 'restock',
    input: { testid: 'row', attribute: 'data-action-input' }, authentication: 'none' }, unexpected);
  assert.equal(rejectedInput.status, 'failed');
  assert.match(rejectedInput.summary ?? '', /unexpected extra/);

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
  assert.match(checked.summary ?? '', /does not meet the access-error status contract/);
  assert.doesNotMatch(checked.summary ?? '', /was accepted|instead of refus/);
  // A 404 names the operation the application interface requires, so a repair
  // round can create the missing endpoint instead of chasing authorization.
  assert.match(checked.summary ?? '', /the admin_restock reducer/);
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

test('generic client errors do not prove a named action was refused for authorization', async () => {
  for (const status of [400, 409, 422]) {
    const actor = {
      name: 'customer',
      actionCall: { action: 'checkout', accepted: false, status },
    };
    const provided = services(new Map<string, unknown>([['customer', actor]]));
    const checked = await run({ do: 'expectActionOutcome', actor: 'customer',
      outcome: 'refused' }, provided);
    assert.equal(checked.status, 'failed');
    assert.match(checked.summary ?? '', /does not meet the access-error status contract/);
    assert.doesNotMatch(checked.summary ?? '', /was accepted|instead of refus/);
  }
});

test('validation refusal accepts only deliberate application rejection statuses', async () => {
  for (const [status, expected] of [[400, 'passed'], [409, 'passed'], [422, 'passed'],
    [403, 'failed'], [500, 'failed']] as const) {
    const actor = { name: 'customer',
      actionCall: { action: 'cart-set-quantity', accepted: false, status } };
    const provided = services(new Map<string, unknown>([['customer', actor]]));
    const checked = await run({ do: 'expectActionOutcome', actor: 'customer',
      outcome: 'validation-refused' }, provided);
    assert.equal(checked.status, expected, checked.summary ?? undefined);
  }
});

test('business outcomes allow deliberate refusals but never turn server errors into completed work', async () => {
  for (const outcome of ['completed', 'application-refused']) {
    for (const status of [0, 200, 400, 401, 403, 404, 409, 422, 500, 530]) {
      const actor = { name: 'customer', actionCall: { action: 'review', status,
        accepted: status === 200, applicationRejected: status === 530 } };
      const provided = services(new Map<string, unknown>([['customer', actor]]));
      const result = await run({ do: 'expectActionOutcome', actor: 'customer', outcome }, provided);
      const allowed = [400, 401, 403, 409, 422, 530].includes(status)
        || (outcome === 'completed' && status === 200);
      assert.equal(result.status, allowed ? 'passed' : 'failed', `${outcome}: ${status}`);
    }
    const customer = { name: 'customer',
      actionCall: { action: 'review', status: 404, accepted: false } };
    const owner = { name: 'owner',
      actionCall: { action: 'review', status: 200, accepted: true } };
    const provided = services(new Map<string, unknown>([['customer', customer], ['owner', owner]]));
    assert.equal((await run({ do: 'expectActionOutcome', actor: 'customer', outcome,
      routeProvenBy: 'owner' }, provided)).status, 'passed');
    owner.actionCall.action = 'buy';
    assert.equal((await run({ do: 'expectActionOutcome', actor: 'customer', outcome,
      routeProvenBy: 'owner' }, provided)).status, 'failed');
  }
});

test('purchase and restock privacy refusals require their successful control', async () => {
  for (const [file, id] of [['01-purchase-session.json', '101a'], ['01-admin-write-staff.json', '103b']]) {
    const scenario = JSON.parse(readFileSync(`tracks/ecommerce/scenarios/${file}`, 'utf8'));
    const feature = scenario.features.find((feature: { criteria: { id: string }[] }) =>
      feature.criteria.some(criterion => criterion.id === id));
    const steps = [...feature.setup, ...feature.criteria.find((criterion: { id: string }) => criterion.id === id).steps];
    const refusal = steps.find(step => step.do === 'expectActionOutcome' && step.outcome === 'refused');
    const accepted = steps.findIndex(step => step.do === 'expectActionOutcome'
      && step.outcome === 'accepted' && step.actor === refusal.routeProvenBy);
    assert(accepted >= 0 && accepted < steps.indexOf(refusal));
    const control = steps.slice(0, accepted).findLast(step => step.do === 'callAction');
    const caller = { name: refusal.actor, actionCall: { action: control.action, status: 404, accepted: false } };
    const owner = { name: refusal.routeProvenBy, actionCall: { action: control.action, status: 200, accepted: true } };
    const provided = services(new Map<string, unknown>([[caller.name, caller], [owner.name, owner]]));
    assert.equal((await run(refusal, provided)).status, 'passed');
    owner.actionCall.accepted = false;
    owner.actionCall.status = 404;
    assert.equal((await run(refusal, provided)).status, 'failed');
  }
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
  assert.match(called.summary ?? '', /input for the buy action is not valid/);
  assert.equal(requests, 0);

  const checked = await run({ do: 'expectActionOutcome', actor: 'customer', outcome: 'refused' }, provided);
  // An assertion whose action never ran measures the scenario, not the app.
  assert.equal(checked.status, 'inconclusive');
  assert.match(checked.summary ?? '', /no callAction ran/);
});

test('account setup preserves scoped credentials and classifies browser failures', async () => {
  const calls: unknown[][] = [];
  let actualUser = 'Alicescope';
  const locator = (purpose: string) => ({
    first() { return this; },
    isVisible: async () => true,
    fill: async (value: string) => { calls.push([purpose, 'fill', value]); },
    inputValue: async () => actualUser,
    click: async () => { calls.push([purpose, 'click']); },
    waitFor: async (options: unknown) => { calls.push([purpose, 'waitFor', options]); },
  });
  const actor = {
    loc: () => assert.fail('a visible signup form does not need a toggle'),
    page: { locator: (selector: string) => locator(selector) },
  };
  const passed = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', actor]])));
  assert.equal(passed.status, 'passed');
  assert.equal(record(passed.observation).user, 'Alicescope');
  assert(calls.some(call => call[2] === 'pw-Alicescope'));
  actualUser = 'Alice'; calls.length = 0;
  const truncated = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', actor]])));
  assert.equal(truncated.status, 'inconclusive');
  assert.match(JSON.stringify(truncated.finding), /input changed the requested username/);
  assert(!calls.some(call => call[1] === 'click'));

  const timeout = Object.assign(new Error('locator.fill: timed out'), { name: 'TimeoutError' });
  const timedOutActor = { page: { locator: () => ({ first() { return this; },
    isVisible: async () => true,
    fill: async () => { throw timeout; } }) } };
  const timedOut = await run({ do: 'signUp', actor: 'a', name: 'Alice' },
    services(new Map<string, unknown>([['a', timedOutActor]])));
  assert.equal(timedOut.status, 'failed');
  assert.equal(timedOut.code, 'application_failure');

  const buggyActor = { page: { locator: () => ({ first() { return this; },
    isVisible: async () => true,
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
    or() { return { first() { return this; },
      filter(options: unknown) { assert.deepEqual(options, { visible: true }); return this; },
      waitFor: async (options: unknown) => { calls.push(['form-or-toggle', 'waitFor', options]); } }; },
    isVisible: async () => formVisible,
    waitFor: async (options: unknown) => {
      calls.push(['username', 'waitFor', options]);
      assert.equal(formVisible, true);
    },
    fill: async (value: string) => { calls.push(['username', 'fill', value]); },
    inputValue: async () => 'admin',
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
    ['form-or-toggle', 'waitFor', { state: 'visible', timeout: 5000 }],
    ['toggle', 'click', { timeout: 5000 }],
    ['username', 'waitFor', { state: 'visible', timeout: 5000 }],
  ]);
  assert(calls.some(call => call[0] === 'username' && call[2] === 'admin'));
  assert(calls.some(call => call[0] === 'password' && call[2] === 'secret'));
});

test('account restoration does not accept the wrong signed-in user', async () => {
  const currentUser = {
    first() { return this; },
    isVisible: async () => true,
    innerText: async () => 'signed in as another-user',
  };
  const actor = {
    name: 'browser-a',
    page: { locator: () => currentUser },
  };
  const result = await run({ do: 'ensureSignedIn', actor: 'a', name: 'alice' },
    services(new Map<string, unknown>([['a', actor]])));
  assert.equal(result.status, 'harness_failure');
  assert.match(result.summary ?? '', /different account/);
});

test('an unreplayable WebSocket write cannot earn server-side forgery credit', async () => {
  const actor = {
    name: 'a',
    lastWrite: null,
    lastWsWrite: { event: 'send_message', body: { content: 'hello' } },
  };
  const provided = services(new Map<string, unknown>([['a', actor], ['victim', {}]]));
  const forged = await run({ do: 'forgeWrite', actor: 'a', fromActor: 'victim', settleMs: 0 }, provided);
  assert.equal(forged.status, 'passed');
  assert.deepEqual(forged.observation, { attempted: false, classification: 'unverified' });

  const checked = await run({ do: 'expectForgeryRejected', actor: 'a' }, provided);
  assert.equal(checked.status, 'inconclusive');
  assert.match(checked.summary ?? '', /could not verify the forgery refusal/);
  assert.deepEqual(provided.verification.map(([kind]) => kind), ['unverified']);
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
  assert.match(checked.summary ?? '', /could not verify the forgery refusal/);
});

test('a negative delivery check observes its full window before passing', async () => {
  const delays: number[] = [];
  const actor = { name: 'outsider', wasSent: () => false };
  const provided = services(new Map<string, unknown>([['outsider', actor]]), {
    sleep: async milliseconds => { delays.push(milliseconds); },
  });
  const checked = await run({ do: 'expectNotReceived', actor: 'outsider', contains: 'secret',
    within: 1234 }, provided);
  assert.equal(checked.status, 'passed');
  assert.deepEqual(delays, [1234]);
});

test('a delivery check is inconclusive when application traffic is not observable', async () => {
  const actor = { name: 'owner', wasSent: () => false };
  const provided = services(new Map<string, unknown>([['owner', actor]]), {
    sleep: async () => undefined,
  });
  const checked = await run({ do: 'expectReceived', actor: 'owner', contains: 'private message',
    within: 1 }, provided);
  assert.equal(checked.status, 'inconclusive');
});

test('replay retargeting maps nested entity ids by field and relationship depth', async () => {
  const requests: CapturedRequest[] = [];
  const actor = (name: string, received: string[], writes: UnknownRecord[]) => ({
    name,
    received,
    writes,
    page: {
      request: { fetch: async (url: string, options: UnknownRecord) => {
        requests.push({ url, options });
        return { status: () => 200, ok: () => true };
      } },
    },
  });
  const staff = actor('staff', [[
    'data: {"orders":[{"_id":"order-desk","userId":"staff-user",',
    'data: "items":[{"itemId":"item-desk","name":"Desk Lamp"}]}]}',
    '',
  ].join('\n')], [{
    url: 'http://app.test/api/fulfilment/order-desk/ship', method: 'POST',
    headers: { authorization: 'Bearer staff-token' }, body: null,
  }]);
  const customer = actor('customer', [JSON.stringify({ order: {
    _id: 'order-webcam', userId: 'customer-user',
    items: [{ itemId: 'item-webcam', name: 'Webcam' }],
  } })], [{
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
  assert.match(rejected.summary ?? '', /who must be refused, was accepted/);
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
  assert.match(String(record(request.options.headers).cookie), /sid=customer-session/);

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
      assert.deepEqual(options, { contains: 'test-scoped' });
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
    page: { evaluate: async (callback: () => unknown) => Function('window',
      `return (${callback.toString()})()`)({ getSessionToken: () => 'eyJcustomer.token.value' }) },
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
    namedTarget: { testid: 'order-item', contains: '{room:test}',
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

test('named replay can replace a declared literal without an undeclared UI attribute', async () => {
  const requests: CapturedRequest[] = [];
  const source = { name: 'staff', writes: [], received: [],
    lastWsWrite: { event: 'binary reducer call', body: {} } };
  const customer = {
    name: 'customer', writes: [], received: [],
    context: { cookies: async () => [] },
    page: { evaluate: async () => ['customer-token'] },
  };
  const provided = services(new Map<string, unknown>([
    ['staff', source],
    ['customer', customer],
  ]), {
    backend: 'spacetime',
    spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
    fetchImpl: async (url, options) => {
      requests.push({ url, options: options as unknown as UnknownRecord });
      return namedResponse(530, false);
    },
  });

  const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'SAVE10',
    swap: { find: 'SAVE10', with: 'HACK10' },
    namedAction: { id: 'createPromotion', path: '/api/promotions', reducer: 'create_promotion',
      args: ['SAVE10', 10] }, settleMs: 0 }, provided);
  assert.equal(replayed.status, 'passed');
  assert.equal(requests[0]?.options.body, '["HACK10",10]');
});

test('a missing or malformed declared replay target is an application failure', async () => {
  for (const [value, message] of [[null, /exposes no data-entity-id/],
    ['not-an-id', /data-entity-id for the ship action is not valid/]] as const) {
    const source = { name: 'staff', writes: [], received: [],
      loc: () => ({ waitFor: async () => undefined, getAttribute: async () => value }) };
    const customer = { name: 'customer', writes: [], received: [] };
    const provided = services(new Map<string, unknown>([
      ['staff', source],
      ['customer', customer],
    ]));
    const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
      namedAction: { id: 'ship', path: '/api/ship', reducer: 'ship', args: [0] },
      namedTarget: { testid: 'order', attribute: 'data-entity-id', valueType: 'number' },
      settleMs: 0 }, provided);
    assert.equal(replayed.status, 'failed');
    assert.match(replayed.summary ?? '', message);
  }
});

test('only an explicit authorization response proves a replay refusal', async () => {
  for (const status of [0, 302, 400, 404, 422, 503]) {
    const actor = {
      name: 'customer',
      replay: { accepted: false, status, method: 'POST', url: '/ship' },
    };
    const provided = services(new Map<string, unknown>([['customer', actor]]));
    const checked = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
    assert.equal(checked.status, 'failed');
    assert.match(checked.summary ?? '', /does not meet the access-error status contract/);
    assert.doesNotMatch(checked.summary ?? '', /was accepted|instead of refus/);
    assert.equal(provided.verification.length, 0);
  }
});

test('an idempotent replay permits success or deliberate validation refusal, never a transport failure', async () => {
  for (const status of [0, 200, 204, 302, 400, 401, 403, 404, 409, 422, 500, 530]) {
    const actor = { name: 'staff', replay: { accepted: status >= 200 && status < 300,
      status, method: 'POST', url: '/refund', applicationRejected: status === 530 } };
    const provided = services(new Map<string, unknown>([['staff', actor]]));
    const checked = await run({ do: 'expectReplayCompleted', actor: 'staff' }, provided);
    assert.equal(checked.status,
      [200, 204, 400, 409, 422, 530].includes(status) ? 'passed' : 'failed', String(status));
    assert.equal((await run({ do: 'expectReplayCompleted', actor: 'staff', requireAccepted: true }, provided)).status,
      actor.replay.accepted ? 'passed' : 'failed');
    if (actor.replay.accepted) {
      assert.equal((await run({ do: 'expectReplayRejected', actor: 'staff' }, provided)).status,
        'failed', 'idempotency acceptance must not weaken authorization checks');
    }
  }
  const absent = services(new Map<string, unknown>([['staff', { name: 'staff' }]]));
  assert.equal((await run({ do: 'expectReplayCompleted', actor: 'staff' }, absent)).status,
    'inconclusive');
});

test('a private-resource replay may explicitly treat not found as refusal', async () => {
  const actor = { name: 'customer',
    replay: { accepted: false, status: 404, method: 'POST', url: '/support/1/replies' } };
  const provided = services(new Map<string, unknown>([['customer', actor]]));
  const checked = await run({ do: 'expectReplayRejected', actor: 'customer',
    allowNotFound: true }, provided);
  assert.equal(checked.status, 'passed');
});

test('only an explicit authorization response proves a forged-write refusal', async () => {
  for (const status of [400, 404, 422, 500]) {
    const actor = {
      name: 'attacker',
      forge: { accepted: false, status, tamperedField: 'userId', reason: 'tampered request sent' },
    };
    const provided = services(new Map<string, unknown>([['attacker', actor]]));
    const checked = await run({ do: 'expectForgeryRejected', actor: 'attacker' }, provided);
    assert.equal(checked.status, 'failed');
    assert.match(checked.summary ?? '', /does not meet the access-error status contract/);
    assert.doesNotMatch(checked.summary ?? '', /was accepted|instead of refus/);
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
  assert.match(replayed.summary ?? '', /could not issue the replay as/);
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
  assert.equal((await run({ do: 'expectCallOutcomes' }, provided)).status, 'passed',
    'idempotent success does not require a specific accepted-response count');
  const mismatch = await run({ do: 'expectCallOutcomes', accepted: 1 }, provided);
  assert.equal(mismatch.status, 'failed');
  assert.match(mismatch.summary ?? '', /calls were accepted, expected 1/);
});

test('named calls scope browser cookies and captured request context to the action destination', async () => {
  const seen: CapturedRequest[] = [];
  const scoped: string[] = [];
  const actor = (name: string) => ({
    name,
    writes: [
      { url: 'http://app.test/api/cart', method: 'POST', body: {},
        headers: { cookie: 'stale=must-not-replay', 'x-csrf-token': `${name}-csrf`,
          origin: 'http://app.test', referer: 'http://app.test/cart' } },
      { url: 'http://provider.test/token', method: 'POST', body: {},
        headers: { authorization: 'Bearer provider-private', cookie: 'provider=private',
          'x-csrf-token': 'provider-private', origin: 'http://provider.test' } },
    ],
    context: { cookies: async (url: string) => {
      scoped.push(url);
      assert.equal(url, 'http://app.test/api/checkout');
      // The browser owns cookie domain/path/secure matching; an unscoped read
      // would also expose provider and /account-only cookies.
      return [{ name: 'sid', value: name }];
    } },
    page: { evaluate: async () => null },
  });
  const provided = services(new Map([['a', actor('a')], ['b', actor('b')]]), {
    fetchImpl: async (url, options) => {
      seen.push({ url, options: options as unknown as UnknownRecord });
      const headers = record(options.headers);
      const user = String(headers.Cookie).slice(4);
      const valid = headers.origin === 'http://app.test'
        && headers['x-csrf-token'] === `${user}-csrf` && !headers.authorization;
      return namedResponse(valid ? 200 : 403, valid);
    },
  });
  for (const step of [
    { do: 'callAction', actor: 'a', action: 'checkout', settleMs: 0,
      namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] } },
    { do: 'expectActionOutcome', actor: 'a', outcome: 'accepted' },
    { do: 'callConcurrently', actors: ['a', 'b'], action: 'checkout', settleMs: 0 },
    { do: 'expectCallOutcomes', accepted: 2 },
  ]) {
    const result = await run(step, provided);
    assert.equal(result.status, 'passed', JSON.stringify(result));
  }
  assert.equal(scoped.length, 3);
  assert.equal(seen.length, 3);
  assert(!JSON.stringify(seen).includes('provider-private'));
  assert(!JSON.stringify(seen).includes('stale=must-not-replay'));
});

test('cross-account replay uses caller CSRF context and cannot credit missing caller context', async () => {
  for (const callerContext of [true, false]) {
    let requests = 0;
    const owner = { name: 'owner', received: [], writes: [{
      url: 'http://app.test/api/orders/1/cancel', method: 'POST', body: {},
      headers: { cookie: 'sid=owner', 'x-csrf-token': 'owner-csrf', origin: 'http://app.test' },
    }] };
    const other = { name: 'other', received: [], writes: callerContext ? [{
      url: 'http://app.test/api/cart', method: 'POST', body: {},
      headers: { cookie: 'sid=other', 'x-csrf-token': 'other-csrf', origin: 'http://app.test' },
    }] : [], context: { cookies: async (url: string) => {
      assert.equal(url, 'http://app.test/api/orders/1/cancel');
      return [{ name: 'sid', value: 'other' }];
    } }, page: { evaluate: async () => null, request: { fetch: async (_url: string, options: UnknownRecord) => {
      requests++;
      const headers = record(options.headers);
      assert.equal(headers.cookie, 'sid=other');
      assert.equal(headers['x-csrf-token'], 'other-csrf');
      assert.equal(headers.origin, 'http://app.test');
      // A valid caller request reaches the ownership check and is refused.
      return { status: () => 403, ok: () => false };
    } } } };
    const provided = services(new Map<string, unknown>([['owner', owner], ['other', other]]));
    const result = await run({ do: 'replayAs', actor: 'other', from: 'owner', match: 'cancel', settleMs: 0 }, provided);
    assert.equal(result.status, callerContext ? 'passed' : 'inconclusive', JSON.stringify(result));
    assert.equal(requests, callerContext ? 1 : 0);
    if (callerContext) assert.equal((await run({ do: 'expectReplayRejected', actor: 'other' }, provided)).status, 'passed');
  }
});

test('concurrent calls classify every result before counting accepted requests', async () => {
  for (const backend of ['postgres', 'spacetime']) {
    for (const status of [0, 400, 401, 403, 404, 409, 422, 500, 530]) {
      const actors = new Map(['a', 'b'].map(name => [name, { name,
        writes: [{ headers: { authorization: `Bearer ${name}-session` } }] }]));
      let calls = 0;
      const provided = services(actors, { backend,
        spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
        fetchImpl: async () => {
          if (calls++ === 0) return namedResponse(200, true);
          if (status === 0) throw new Error('connection lost');
          return namedResponse(status, false);
        } });
      assert.equal((await run({ do: 'callConcurrently', actors: ['a', 'b'],
        action: 'checkout', settleMs: 0 }, provided)).status, 'passed');
      const checked = await run({ do: 'expectCallOutcomes', accepted: 1 }, provided);
      assert.equal(checked.status, status === 0 ? 'inconclusive' : [400, 409, 422].includes(status)
        || (backend === 'spacetime' && status === 530) ? 'passed' : 'failed',
      `${backend}: ${status}`);
    }
  }
  const provided = services(new Map());
  const named = provided.capabilities['named-actions'] as ReturnType<typeof createNamedActionsCapability>;
  named.lastCalls.set({ action: 'checkout', fired: 2, ms: 1,
    outcomes: [{ name: 'a', status: 200, ok: true, text: '' }] });
  assert.equal((await run({ do: 'expectCallOutcomes', accepted: 1 }, provided)).status, 'inconclusive',
    'a missing outcome must not pass');
});

test('bounded checkout cohorts issue every request and retain client timing and transport outcomes', async () => {
  const actors = new Map(['a', 'b'].map(name => [name, { name,
    writes: [{ headers: { authorization: `Bearer ${name}-session` } }] }]));
  for (const requests of [1, 4, 16, 64]) {
    let dispatched = 0;
    let release!: () => void;
    const allDispatched = new Promise<void>(resolve => { release = resolve; });
    const provided = services(actors, { fetchImpl: async () => {
      dispatched++;
      if (dispatched === requests) release();
      await allDispatched;
      return namedResponse(200, true);
    } });
    const result = await run({ do: 'callConcurrently', actors: ['a', 'b'],
      action: 'checkout', requests, settleMs: 0 }, provided);
    assert.equal(result.status, 'passed');
    const evidence = record(result.observation);
    assert.equal(dispatched, requests);
    assert.equal(evidence.responses, requests);
    assert.equal(evidence.transportErrors, 0);
    assert.equal(evidence.timeouts, 0);
    assert.match(String(evidence.timingScope), /not server execution overlap/);
    for (const [index, outcome] of provided.calls!.outcomes.entries()) {
      assert.equal(outcome.requestIndex, index + 1);
      assert.equal(outcome.name, index % 2 ? 'b' : 'a');
      assert.equal(outcome.transport, 'response');
      assert(outcome.completedAtMs! >= outcome.startedAtMs!);
      assert.equal(outcome.durationMs, outcome.completedAtMs! - outcome.startedAtMs!);
    }
  }
  let calls = 0;
  const provided = services(actors, { fetchImpl: async (_url, options) => {
    if (calls++ === 0) throw new Error('sensitive transport internals');
    return new Promise((_resolve, reject) => options.signal!.addEventListener('abort',
      () => reject(options.signal!.reason), { once: true }));
  } });
  const keepAlive = setInterval(() => {}, 20);
  try {
    const result = await run({ do: 'callConcurrently', actors: ['a', 'b'],
      action: 'checkout', requests: 2, requestTimeoutMs: 10, settleMs: 0 }, provided);
    assert.equal(record(result.observation).responses, 0);
    assert.equal(record(result.observation).transportErrors, 1);
    assert.equal(record(result.observation).timeouts, 1);
    assert.doesNotMatch(JSON.stringify(result), /sensitive transport internals/);
    assert.equal((await run({ do: 'expectCallOutcomes' }, provided)).status, 'inconclusive');
  } finally { clearInterval(keepAlive); }
});

test('named writes expose refused response bodies to the existing privacy check', async () => {
  const received: string[] = [];
  const actor = { name: 'guest', record: (text: string) => received.push(text),
    wasSent: (needle: string) => received.some(text => text.includes(needle)) };
  const provided = services(new Map([['guest', actor]]), {
    fetchImpl: async () => ({ ok: false, status: 403, text: async () => 'private-owner-address' }),
  });
  assert.equal((await run({ do: 'callAction', actor: 'guest', authentication: 'none', action: 'checkout',
    namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] } }, provided)).status, 'passed');
  assert.equal((await run({ do: 'expectNotReceived', actor: 'guest', contains: 'private-owner-address' }, provided)).status, 'failed');
});

test('cancelled cohorts drain requests and retain their history in action evidence', async () => {
  for (const duringSettle of [false, true]) {
    const controller = new AbortController();
    let drained = false;
    let calls = 0;
    const actors = new Map(['a', 'b'].map(name => [name, { name,
      writes: [{ headers: { authorization: `Bearer ${name}-session` } }] }]));
    const provided = services(actors, {
      fetchImpl: async (_url, options) => {
        if (calls++ === 0 || duringSettle) return namedResponse(200, true);
        return new Promise((_resolve, reject) => {
          options.signal!.addEventListener('abort', () => setTimeout(() => {
            drained = true;
            reject(options.signal!.reason);
          }, 5), { once: true });
          controller.abort('test cancellation');
        });
      },
      sleep: async () => { controller.abort('test cancellation'); throw controller.signal.reason; },
    });
    const result = await executeAction(ACTION_REGISTRY, 'callConcurrently', {
      do: 'callConcurrently', actors: ['a', 'b'], action: 'checkout', settleMs: 1,
    }, { capabilities: provided.capabilities, signal: controller.signal, onAbort: async () => {} });
    assert.equal(result.code, 'cancelled');
    assert.equal(result.status, 'inconclusive');
    assert.equal(calls, 2);
    assert.equal(drained, !duringSettle);
    const history = record(result.observation);
    assert.deepEqual(history.outcomes, provided.calls!.outcomes);
    assert.equal(history.responses, duringSettle ? 2 : 1);
    assert.equal(history.cancelled, duringSettle ? 0 : 1);
    assert.doesNotMatch(JSON.stringify(result), /Bearer|session/);
  }
});

test('unknown request outcomes take priority over error responses in either order', async () => {
  for (const statuses of [[0, 500], [500, 0]]) {
    const provided = services(new Map());
    const named = provided.capabilities['named-actions'] as ReturnType<typeof createNamedActionsCapability>;
    named.lastCalls.set({ action: 'checkout', fired: 2, ms: 1,
      outcomes: statuses.map(status => ({ name: 'a', status, ok: false, text: '' })) });
    assert.equal((await run({ do: 'expectCallOutcomes' }, provided)).status, 'inconclusive');
  }
});

test('a committed operation with a truncated HTTP success body remains unknown', { timeout: 10000 }, async () => {
  let committed = 0;
  const server = createServer((_request, response) => {
    committed++;
    response.writeHead(200, { 'Content-Type': 'application/json' });
    if (committed === 1) response.end('{"accepted":true}');
    else response.write('{"accepted":'); // Commit occurred, but the reply never finishes.
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  try {
    const address = server.address();
    assert(address && typeof address !== 'string');
    const actors = new Map(['a', 'b'].map(name => [name, { name,
      writes: [{ headers: { authorization: `Bearer ${name}-session` } }] }]));
    const provided = services(actors, { fetchImpl: (url, options) => fetch(
      url.replace('http://app.test', `http://127.0.0.1:${address.port}`), options) });
    const result = await run({ do: 'callConcurrently', actors: ['a', 'b'], action: 'checkout',
      requestTimeoutMs: 1000, settleMs: 0 }, provided);
    assert.equal(committed, 2);
    assert.equal(record(result.observation).responses, 1);
    assert.equal(record(result.observation).timeouts, 1);
    assert.equal((await run({ do: 'expectCallOutcomes', accepted: 1 }, provided)).status, 'inconclusive');
  } finally {
    const closed = new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    server.closeAllConnections();
    await closed;
  }
});

test('purchase bursts reuse validated dynamic action inputs across stack transports', async () => {
  for (const backend of ['postgres', 'mongodb', 'spacetime']) {
    const requests: CapturedRequest[] = [];
    let raw = '{"itemId":"9007199254740993"}';
    const actors = new Map(['a', 'b'].map(name => [name, { name,
      writes: [{ headers: { authorization: `Bearer ${name}-session` } }],
      loc: () => ({ waitFor: async () => {}, getAttribute: async () => raw }),
    }]));
    const provided = services(actors, { backend,
      spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
      fetchImpl: async (url, options) => {
        requests.push({ url, options: options as unknown as UnknownRecord });
        return namedResponse(200, true);
      } });
    const action = { do: 'callConcurrently', actors: ['a', 'b'], action: 'buy',
      namedAction: { id: 'buy', path: '/api/items/:id/buy', reducer: 'buy_now', args: [0],
        params: [{ name: 'itemId', in: 'path', placeholder: ':id', wireType: 'u64' }] },
      input: { testid: 'item-card', contains: 'Keyboard', attribute: 'data-buy-input' },
      requests: 4, settleMs: 0 };
    assert.equal((await run(action, provided)).status, 'passed', backend);
    assert.equal(requests.length, 4);
    for (const request of requests) {
      if (backend === 'spacetime') assert.equal(request.options.body, '[9007199254740993]');
      else assert.match(request.url, /\/api\/items\/9007199254740993\/buy$/);
    }
    raw = '{"itemId":1,"unexpected":2}';
    assert.equal((await run(action, provided)).status, 'failed');
    assert.equal(requests.length, 4, 'malformed interface cannot dispatch another request');
  }
});

test('mixed groups prepare every input before release and keep each operation and buyer in the history', async () => {
  let reads=0;
  const requests: CapturedRequest[]=[];
  const actors=new Map(['a','admin'].map(name=>[name,{name,
    writes:[{headers:{authorization:`Bearer ${name}`}}],
    loc:()=>({waitFor:async()=>{},getAttribute:async()=>{reads++;return name==='a'?'{"itemId":"1"}':'{"itemId":"1","warehouseId":"2","quantity":2}';}}),
  }]));
  const provided=services(actors,{fetchImpl:async(url,options)=>{
    assert.equal(reads,2,'no mutation before all inputs are prepared');
    requests.push({url,options:options as unknown as UnknownRecord});return namedResponse(200,true);
  }});
  const call={do:'callConcurrently',action:'buy',actors:['a'],requests:4,settleMs:0,
    namedAction:{id:'buy',path:'/api/items/:id/buy',reducer:'buy_now',args:[0],params:[{name:'itemId',in:'path',placeholder:':id',wireType:'u64'}]},
    input:{testid:'item',attribute:'data-buy-input'},alongside:[{action:'restock',actors:['admin'],requests:4,delayMs:5,
      namedAction:{id:'restock',path:'/api/admin/restock',reducer:'admin_restock',args:[0,0,1],params:[{name:'itemId',in:'body',wireType:'u64'},{name:'warehouseId',in:'body',wireType:'u64'},{name:'quantity',in:'body'}]},
      input:{testid:'stock',attribute:'data-restock-input'}}]};
  const result=await run(call,provided);
  assert.equal(result.status,'passed',JSON.stringify(result));assert.equal(requests.length,8);
  const outcomes=record(result.observation).outcomes as UnknownRecord[];
  assert.equal(outcomes.length,8);
  for(const row of outcomes){
    assert.equal(row.name,row.action==='buy'?'a':'admin');
    assert.equal(row.dispatched,true);
    assert.deepEqual(row.values,row.action==='buy'?{itemId:'1'}:{itemId:'1',warehouseId:'2',quantity:2});
    assert(Number(row.startedAtMs)>=Number(row.scheduledAtMs));
  }
  for (const invalid of [{body:{}}, {actors:['admin','admin']}, {input:{}}, {alongside:[]},
    {namedAction:{...call.alongside[0]!.namedAction,id:'wrong'}}]) {
    const rejected=await run({...call,alongside:[{...call.alongside[0],...invalid}]},provided);
    assert.equal(rejected.status,'harness_failure');
    assert.equal(requests.length,8,'invalid nested input cannot dispatch mutations');
  }
});

test('named calls keep actor credentials separate in storage and in-memory accessors', async () => {
  for (const inMemory of [false, true]) {
    const requests: CapturedRequest[] = [];
    const storage = (entries: ReadonlyArray<readonly [string, string]>) => ({
      length: entries.length,
      key: (index: number) => entries[index]?.[0] ?? null,
      getItem: (key: string) =>
        entries.find(([candidate]) => candidate === key)?.[1] ?? null,
    });
    const actor = (name: string) => ({
      name,
      context: { cookies: async () => [{ name: 'ui', value: name }] },
      page: { evaluate: async (browserFunction: () => unknown) => Function('localStorage', 'sessionStorage', 'window',
        `return (${browserFunction.toString()})()`)(
        storage(inMemory ? [] : [['theme', 'dark'], ['pgshop_token', `${name}-opaque-session-token-value`]]),
        storage([]), inMemory ? { getSessionToken: () => `${name}-opaque-session-token-value` } : {}) },
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
    assert.equal(record(firstRequest.options.headers).Cookie, 'ui=a');
    assert.equal(record(secondRequest.options.headers).Authorization,
      'Bearer b-opaque-session-token-value');
    assert.equal(record(secondRequest.options.headers).Cookie, 'ui=b');
  }
});

test('unobserved session tokens cannot issue a replay or earn authorization credit', async () => {
  for (const getSessionToken of [undefined, () => null, () => '', () => 42,
    () => 'invalid\r\nheader', () => { throw new Error('private-token-sentinel'); }]) {
    let requests = 0;
    const actor = { name: 'customer', writes: [], received: [],
      context: { cookies: async () => [] },
      page: { evaluate: async (callback: () => unknown) => {
        const storage = { length: getSessionToken ? 1 : 0, key: () => 'auth_token',
          getItem: () => 'stale-storage-token' };
        return Function('window', 'localStorage', 'sessionStorage',
          `return (${callback.toString()})()`)({ getSessionToken }, storage, storage);
      } } };
    const provided = services(new Map<string, unknown>([
      ['customer', actor], ['staff', { name: 'staff', writes: [], received: [] }],
    ]), { fetchImpl: async () => { requests++; return namedResponse(403, false); } });
    const replayed = await run({ do: 'replayAs', actor: 'customer', from: 'staff', match: 'ship',
      namedAction: { id: 'ship', path: '/api/ship', reducer: 'ship_order', args: [52] },
      settleMs: 0 }, provided);
    assert.equal(replayed.status, 'inconclusive');
    assert.match(replayed.summary ?? '', /could not issue the replay/);
    if (getSessionToken) assert.match(JSON.stringify(replayed), /getSessionToken/);
    assert.doesNotMatch(JSON.stringify(replayed), /private-token-sentinel|stale-storage-token/);
    assert.equal(requests, 0);
    const checked = await run({ do: 'expectReplayRejected', actor: 'customer' }, provided);
    assert.equal(checked.status, 'inconclusive');
  }
});

test('missing named actions and application roots stay inconclusive', async () => {
  const actor = { context: { cookies: async () => [{ name: 'sid', value: 'a' }] },
    page: { evaluate: async () => null } };
  const missingAction = await run({ do: 'callConcurrently', actors: ['a', 'b'],
    action: 'missing', settleMs: 0 },
  services(new Map<string, unknown>([['a', actor], ['b', actor]]), { actions: [] }));
  assert.equal(missingAction.status, 'inconclusive');
  assert.match(missingAction.summary ?? '', /track names no missing action/);

  const missingRoot = await run({ do: 'runScript', script: 'backoffice.mjs', args: [] },
    services(new Map<string, unknown>()));
  assert.equal(missingRoot.status, 'inconclusive');
  assert.match(missingRoot.summary ?? '', /application directory is unknown/);
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

test('named calls override one declared parameter from another actor without changing the caller', async () => {
  for (const backend of ['postgres', 'mongodb', 'spacetime']) {
    for (const doAction of ['callAction', 'callConcurrently']) {
      const requests: CapturedRequest[] = [];
      let targetId: string | null = '202';
      const owner = { name: 'owner', writes: [{ headers: { Authorization: 'Bearer owner' } }], loc: () => ({ waitFor: async () => {},
        getAttribute: async () => JSON.stringify({ caseId: '101', orderId: '303' }) }) };
      const other = { name: 'other', writes: [{ headers: { Authorization: 'Bearer other' } }],
        loc: () => ({ waitFor: async () => {}, getAttribute: async () => targetId }) };
      const provided = services(new Map<string, unknown>([['owner', owner], ['other', other]]), {
        backend, spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
        actions: [{ id: 'link', path: '/api/cases/{caseId}/order', reducer: 'link_support_order',
          args: [0, 0], params: [{ name: 'caseId', in: 'path', placeholder: '{caseId}', wireType: 'u64' },
            { name: 'orderId', in: 'body', wireType: 'u64' }] }],
        fetchImpl: async (url, options) => {
          requests.push({ url, options: options as unknown as UnknownRecord });
          return namedResponse(200, true);
        },
      });
      const selector = { actor: 'other', testid: 'support-ticket', attribute: 'data-entity-id' };
      const input = { do: doAction, ...(doAction === 'callAction' ? { actor: 'other' } : { actors: ['other', 'owner'], requests: 1 }), from: 'owner', action: 'link',
        input: { testid: 'support-link-order', attribute: 'data-action-input', overrides: { caseId: selector } },
        settleMs: 0 };
      const called = await run(input, provided);
      assert.equal(called.status, 'passed', JSON.stringify({ backend, doAction, called }));
      const request = requests[0]!;
      assert.equal(record(request.options.headers).authorization, 'Bearer other');
      if (backend === 'spacetime') {
        assert.deepEqual(JSON.parse(String(request.options.body)), [202, 303]);
      } else {
        assert.equal(request.url, 'http://app.test/api/cases/202/order');
        assert.deepEqual(JSON.parse(String(request.options.body)), { orderId: '303' });
      }
      const rejected = await run({ ...input, input: { ...input.input, overrides: { unknown: selector } } }, provided);
      assert.equal(rejected.status, 'failed');
      assert.match(JSON.stringify(rejected.finding), /not declared/);
      targetId = null;
      assert.equal((await run(input, provided)).status, 'failed');
      assert.equal(requests.length, 1, 'invalid override must not send a request');
    }
  }
});


test('staff-role replay changes the role without changing the HTTP route or the reducer target', async () => {
  const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-staff-roles.json'), 'utf8'));
  const steps = scenario.features[0].criteria.find((c: { id: string }) => c.id === '621b').steps;
  const replay = steps.find((step: { do: string }) => step.do === 'replayAs');
  for (const backend of ['postgres', 'mongodb', 'spacetime']) {
    const requests: CapturedRequest[] = [];
    const source = { name: 'replayAdmin', received: [],
      writes: backend === 'spacetime' ? [] : [{
        url: 'http://app.test/api/staff/42/role', method: 'PUT',
        headers: { authorization: 'Bearer admin-token' }, body: { role: 'staff' },
      }],
      loc: () => ({ waitFor: async () => undefined, getAttribute: async () => '42' }),
    };
    const staff = { name: 'staff', received: [], writes: [{
      url: 'http://app.test/api/me', method: 'GET',
      headers: { authorization: 'Bearer staff-token' }, body: null,
    }], page: { request: { fetch: async (url: string, options: UnknownRecord) => {
      requests.push({ url, options });
      return { status: () => 403, ok: () => false };
    } } } };
    const provided = services(new Map<string, unknown>([['staff', staff], ['replayAdmin', source]]), {
      backend, spacetime: { uri: 'http://app.test', mod: 'shop' },
      fetchImpl: async (url, options) => {
        requests.push({ url, options: options as unknown as UnknownRecord });
        return namedResponse(530, false);
      },
    });
    assert.equal((await run({ ...replay, settleMs: 0 }, provided)).status, 'passed');
    assert.equal(requests.length, 1);
    if (backend === 'spacetime') {
      assert.equal(requests[0]!.options.body, '[42,"inventory"]');
    } else {
      assert.equal(requests[0]!.url, 'http://app.test/api/staff/42/role');
      assert.equal(requests[0]!.options.data, '{"role":"inventory"}');
    }
    assert.equal((await run({ do: 'expectReplayRejected', actor: 'staff' }, provided)).status, 'passed');
  }
  const rejected = steps.findIndex((step: { do: string }) => step.do === 'expectReplayRejected');
  const after = steps.slice(rejected + 1);
  assert.equal(after[0].do, 'reload');
  assert.equal(after.at(-1).value, 'staff', 'a denied response must leave the persisted role unchanged');
  assert.equal(after.at(-1).in.testid, 'staff-role-account-staff');
});


test('role revocation uses declared transitions despite earlier captured role writes', async () => {
  const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-staff-roles.json'), 'utf8'));
  const steps = scenario.features[0].criteria.find((c: { id: string }) => c.id === '621d').steps;
  const calls = steps.filter((step: { do: string }) => step.do === 'replayAs');
  for (const backend of ['postgres', 'mongodb', 'spacetime']) {
    const requests: CapturedRequest[] = [];
    const actor = (name: string) => ({ name, received: [], writes: [{
      url: 'http://app.test/api/staff/42/role', method: 'PUT',
      headers: { authorization: `Bearer ${name}-token` }, body: { role: 'staff' },
    }], loc: () => ({ waitFor: async () => undefined, getAttribute: async () => '42' }) });
    const provided = services(new Map<string, unknown>([
      ['roleAdmin', actor('roleAdmin')], ['promotedStaff', actor('promotedStaff')],
    ]), { backend, spacetime: { uri: 'http://app.test', mod: 'shop' },
      fetchImpl: async (url, options) => {
        requests.push({ url, options: options as unknown as UnknownRecord });
        return namedResponse(200, true);
      },
    });
    for (const step of calls) assert.equal((await run(step, provided)).status, 'passed');
    assert.equal(requests.length, calls.length);
    assert.deepEqual(requests.map(request => {
      const body = JSON.parse(String(request.options.body));
      return backend === 'spacetime' ? body[1] : body.role;
    }), ['admin', 'admin', 'staff', 'admin']);
    assert.deepEqual(requests.map(request => record(request.options.headers).authorization
      ?? record(request.options.headers).Authorization), calls.map((step: { actor: string }) => `Bearer ${step.actor}-token`));
  }
});


test('native envelopes govern single, concurrent and replay outcomes, including body loss', async () => {
  for (const reply of ['accepted', 'refused', 'schema-refused', 'unhandled', 'malformed', 'body-loss', 'native400']) {
    const actor = { name: 'buyer', received: [], writes: [], context: { cookies: async () => [] },
      page: { evaluate: async () => ['real-browser-token'] } };
    const provided = services(new Map([['buyer', actor]]));
    const prior = record(provided.capabilities['named-actions']);
    const status = reply === 'native400' ? 400 : 200;
    const text = reply === 'accepted' ? '{"status":"success","value":null}'
      : reply === 'refused' ? '{"status":"error","errorMessage":"refused","errorData":null}'
      : reply === 'schema-refused' ? JSON.stringify({ status: 'error', errorMessage: '[Request ID: c3c0e4b69f8972e5] Server Error\nArgumentValidationError: extra field' })
      : reply === 'unhandled' ? '{"status":"error","errorMessage":"missing function"}' : '{}';
    const native = { ...provided, capabilities: { ...provided.capabilities, 'named-actions': {
      ...prior,
      request: () => ({ url: 'http://native.test/api/mutation', method: 'POST', body: '{"path":"api:checkout","args":{}}', responseContract: 'convex-mutation' }),
      classifyResponse: (request: Parameters<typeof classifyResponseContract>[0], response: Parameters<typeof classifyResponseContract>[1]) =>
        classifyResponseContract(request, response),
      fetch: async (_url: string, options: { headers: Record<string, string> }) => {
        assert.equal(options.headers.Authorization, 'Bearer real-browser-token');
        return { status, ok: status === 200, text: async () => { if (reply === 'body-loss') throw new Error('lost'); return text; } };
      },
    } } };
    const call = await run({ do: 'callAction', actor: 'buyer', action: 'checkout',
      namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] }, settleMs: 0 }, native);
    assert.equal(call.status, 'passed', JSON.stringify({ reply, call }));
    const outcome = await run({ do: 'expectActionOutcome', actor: 'buyer', outcome: 'completed' }, native);
    assert.equal(outcome.status, ['accepted', 'refused', 'schema-refused'].includes(reply) ? 'passed'
      : reply === 'body-loss' ? 'inconclusive' : 'failed', reply);
    const concurrentCall = await run({ do: 'callConcurrently', action: 'checkout', actors: ['buyer'], requests: 2, settleMs: 0 }, native);
    assert.equal(concurrentCall.status, 'passed', JSON.stringify({ reply, concurrentCall }));
    const concurrent = await run({ do: 'expectCallOutcomes' }, native);
    assert.equal(concurrent.status, outcome.status, JSON.stringify({ reply, concurrent }));
    await run({ do: 'replayAs', actor: 'buyer', from: 'buyer', match: 'checkout',
      namedAction: { id: 'checkout', path: '/api/checkout', reducer: 'checkout', args: [] }, settleMs: 0 }, native);
    const replay = await run({ do: 'expectReplayCompleted', actor: 'buyer' }, native);
    assert.equal(replay.status, outcome.status, reply);
    if (reply === 'schema-refused') {
      assert.equal((await run({ do: 'expectActionOutcome', actor: 'buyer', outcome: 'refused' }, native)).status, 'failed');
      assert.equal((await run({ do: 'expectReplayRejected', actor: 'buyer' }, native)).status, 'failed');
    }
  }
});


test('native POST queries cannot be forged as writes, and mutation arguments stay inside their envelope', async () => {
  for (const withMutation of [false, true]) {
    const query = { url: 'http://native.test/api/query', method: 'POST', headers: {},
      body: { path: 'api:list', args: { userId: 'buyer' } } };
    const mutation = { ...query, url: 'http://native.test/api/mutation', body: { path: 'api:write', args: [{ userId: 'buyer', message: 'old' }] } };
    const sent: unknown[] = [];
    const actor = { name: 'buyer', writes: withMutation ? [mutation, query] : [query], lastWrite: query,
      page: { request: { fetch: async (url: string, options: UnknownRecord) => {
        assert.equal(url, mutation.url); sent.push(JSON.parse(String(options.data)));
        return { status: () => 200, ok: () => true, text: async () => '{"status":"error","errorMessage":"denied","errorData":null}' };
      } } } };
    const victim = { name: 'victim', lastWrite: { ...mutation, body: { path: 'api:write', args: [{ userId: 'victim' }] } } };
    const provided = services(new Map<string, unknown>([['buyer', actor], ['victim', victim]]));
    const prior = record(provided.capabilities['named-actions']);
    const native = { ...provided, capabilities: { ...provided.capabilities, 'named-actions': { ...prior,
      classifyResponse: (request: Parameters<typeof classifyResponseContract>[0], response: Parameters<typeof classifyResponseContract>[1]) =>
        classifyResponseContract({ ...request, responseContract: request.url?.endsWith('/query') ? 'convex-query' : 'convex-mutation' }, response),
    } } };
    const result = await run({ do: 'forgeWrite', actor: 'buyer', fromActor: 'victim', text: 'new', settleMs: 0 }, native);
    assert.equal(result.status, 'passed', JSON.stringify(result));
    assert.equal(sent.length, withMutation ? 1 : 0);
    if (withMutation) {
      assert.deepEqual(sent[0], { path: 'api:write', args: [{ userId: 'victim', message: 'new' }] });
      assert.equal((await run({ do: 'expectForgeryRejected', actor: 'buyer' }, native)).status, 'passed');
    } else assert.equal((await run({ do: 'expectForgeryRejected', actor: 'buyer' }, native)).status, 'inconclusive');
  }
});
