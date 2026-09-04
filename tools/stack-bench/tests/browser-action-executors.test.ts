import assert from 'node:assert/strict';
import test from 'node:test';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import {
  BROWSER_ACTION_IMPLEMENTATIONS,
  parseRenderedNumber,
} from '../src/actions/browser-action-executors.js';

type UnknownRecord = Record<string, unknown>;
interface ServiceOverrides {
  readonly browser?: UnknownRecord;
  readonly clockSleep?: (milliseconds: number) => Promise<void>;
}
interface ProvidedServices {
  readonly capabilities: Record<string, unknown>;
  readonly recorded: Map<string, number>;
}

function services(actor: unknown, overrides: ServiceOverrides = {}): ProvidedServices {
  const recorded = new Map<string, number>();
  const browser = {
    defaultWithin: 5000,
    expand: (value: string | undefined) => value === '{room:test}' ? 'test-scoped' : value,
    recorded: {
      get: (key: string) => recorded.get(key),
      set: (key: string, value: number) => recorded.set(key, value),
    },
    sleep: async () => {},
    testId: (id: string) => `[data-testid="${id}"]`,
    ...overrides.browser,
  };
  return {
    capabilities: {
      actors: { get: (name: string) => name === 'a' ? actor : undefined },
      'browser-interaction': browser,
      'browser-observation': browser,
      clock: { sleep: overrides.clockSleep ?? (async () => {}) },
    },
    recorded,
  };
}

async function run(
  input: UnknownRecord & { readonly do: string },
  provided: ProvidedServices,
) {
  return executeAction(ACTION_REGISTRY, input.do, input, {
    capabilities: provided.capabilities,
  });
}

test('the extracted executor registry is exact and every migrated action has bounded metadata', () => {
  for (const id of Object.keys(BROWSER_ACTION_IMPLEMENTATIONS)) {
    const plugin = ACTION_REGISTRY.get(id);
    assert(plugin.timeoutMs > 0, id);
    assert(plugin.capabilities.includes('actors'), id);
  }
});

test('timing executes through the contract and still rejects an unknown actor', async () => {
  const slept: number[] = [];
  const provided = services({}, { clockSleep: async (ms) => { slept.push(ms); } });
  const passed = await run({ do: 'wait', actor: 'a', ms: 17 }, provided);
  assert.equal(passed.status, 'passed');
  assert.deepEqual(passed.observation, { waitedMs: 17 });
  assert.deepEqual(slept, [17]);

  const missing = await run({ do: 'wait', actor: 'missing', ms: 1 }, provided);
  assert.equal(missing.status, 'harness_failure');
  assert.equal(missing.code, 'unclassified_exception');
  assert.equal(missing.summary, 'harness did not create actor "missing"');
});

test('interaction actions receive scoped values and preserve click options', async () => {
  const calls: unknown[][] = [];
  const locator = {
    click: async (options: unknown) => { calls.push(['click', options]); },
  };
  const actor = {
    loc: (testid: string, options: unknown) => {
      calls.push(['loc', testid, options]);
      return locator;
    },
  };
  const passed = await run({ do: 'click', actor: 'a', testid: 'open',
    contains: '{room:test}', in: { testid: 'row', contains: '{room:test}' }, settleMs: 5 },
  services(actor));
  assert.equal(passed.status, 'passed');
  assert.deepEqual(calls, [
    ['loc', 'open', { contains: 'test-scoped',
      scope: { testid: 'row', contains: 'test-scoped' } }],
    ['click', { timeout: 5000 }],
  ]);
});

test('interaction scopes can match separate text fragments without assuming punctuation', async () => {
  let scope: { testid: string; contains: RegExp } | undefined;
  const actor = { loc: (_testid: string, options: {
    scope: { testid: string; contains: RegExp };
  }) => {
    scope = options.scope;
    return { click: async () => {} };
  } };
  const result = await run({ do: 'click', actor: 'a', testid: 'save',
    in: { testid: 'row', containsAll: ['Mirrorless Camera', 'East'] } }, services(actor));
  assert.equal(result.status, 'passed');
  assert(scope);
  assert.equal(scope.testid, 'row');
  assert(scope.contains.test('Mirrorless Camera @ East'));
  assert(scope.contains.test('East: Mirrorless Camera'));
  assert.equal(scope.contains.test('Mirrorless Camera @ West'), false);
});

test('fill adapts values to date input types', async () => {
  const values: Array<[string, string]> = [];
  const locator = (type: string) => ({
    waitFor: async () => {},
    evaluate: async () => 'INPUT',
    getAttribute: async (name: string) => name === 'type' ? type : null,
    fill: async (value: string) => { values.push([type, value]); },
  });
  const actor = { loc: (testid: string) => locator(testid) };

  for (const testid of ['datetime-local', 'date', 'text']) {
    const result = await run({ do: 'fill', actor: 'a', testid, text: '2020-01-01' }, services(actor));
    assert.equal(result.status, 'passed');
  }

  assert.deepEqual(values, [
    ['datetime-local', '2020-01-01T00:00'],
    ['date', '2020-01-01'],
    ['text', '2020-01-01'],
  ]);

  values.length = 0;
  await run({ do: 'fill', actor: 'a', testid: 'date', text: '2099-12-31T23:59' }, services(actor));
  assert.deepEqual(values, [['date', '2099-12-31']]);
});

test('recorded-number state is narrow, reusable, and numeric parsing is stable', async () => {
  assert.equal(parseRenderedNumber('Stock: 1,024 left'), 1024);
  assert.equal(parseRenderedNumber('$12.50'), 12.5);
  assert.equal(parseRenderedNumber('none'), null);

  let rendered = 'Total: 1,024';
  const locator = {
    waitFor: async () => {},
    evaluate: async () => 'DIV',
    innerText: async () => rendered,
  };
  const actor = { loc: () => locator };
  const provided = services(actor);
  const recorded = await run({ do: 'recordNumber', actor: 'a', testid: 'total', as: 'before' }, provided);
  assert.equal(recorded.status, 'passed');
  assert.equal(provided.recorded.get('before'), 1024);

  rendered = 'Total: 1,027';
  const compared = await run({ do: 'expectNumber', actor: 'a', testid: 'total',
    relativeTo: 'before', plus: 3 }, provided);
  assert.equal(compared.status, 'passed');
  assert.deepEqual(compared.observation, { value: 1027 });
});

test('an observation mismatch is application evidence, not a harness crash', async () => {
  const locator = {
    waitFor: async () => {},
    innerText: async () => 'contains private value',
  };
  const actor = { loc: () => locator };
  const result = await run({ do: 'expect', actor: 'a', testid: 'status',
    notContains: 'private value' }, services(actor));
  assert.equal(result.status, 'failed');
  assert.equal(result.code, 'application_failure');
  assert.match(result.summary ?? '', /shows text that must not appear/);
});

test('a visible but blank field does not satisfy a non-empty assertion', async () => {
  let rendered = '   ';
  const locator = {
    waitFor: async () => {},
    evaluate: async () => 'DIV',
    innerText: async () => rendered,
  };
  const actor = { loc: () => locator };
  const blank = await run({ do: 'expect', actor: 'a', testid: 'warehouse', nonEmpty: true },
    services(actor));
  assert.equal(blank.status, 'failed');
  assert.equal(blank.code, 'application_failure');
  assert.match(blank.summary ?? '', /control is empty/);

  rendered = 'East';
  const populated = await run({ do: 'expect', actor: 'a', testid: 'warehouse', nonEmpty: true },
    services(actor));
  assert.equal(populated.status, 'passed');
});

test('absence checks do not pass before a late element appears', async () => {
  let checks = 0;
  const actor = { loc: () => ({ isVisible: async () => ++checks > 1 }) };
  const result = await run({ do: 'expect', actor: 'a', testid: 'private-row',
    absent: true, within: 100 }, services(actor));
  assert.equal(result.status, 'failed');
  assert.match(result.summary ?? '', /was shown when it must not be/);
});

test('waitUntilAbsent waits for a visible element to leave', async () => {
  const calls: unknown[] = [];
  const actor = { loc: () => ({ waitFor: async (options: unknown) => { calls.push(options); } }) };
  const result = await run({ do: 'waitUntilAbsent', actor: 'a', testid: 'queue-item',
    contains: 'Keyboard', within: 1000 }, services(actor));
  assert.equal(result.status, 'passed');
  assert.deepEqual(calls, [{ state: 'hidden', timeout: 1000 }]);
});

test('unavailable checks do not pass before a control becomes enabled', async () => {
  let checks = 0;
  const locator = {
    filter() { return this; },
    first() { return this; },
    isVisible: async () => true,
    isDisabled: async () => ++checks === 1,
    getAttribute: async () => null,
  };
  const actor = { page: { locator: () => locator } };
  const result = await run({ do: 'expectUnavailable', actor: 'a', testid: 'admin',
    within: 100 }, services(actor));
  assert.equal(result.status, 'failed');
  assert.match(result.summary ?? '', /stayed available to/);
});

test('missing values do not satisfy agreement across actors', async () => {
  const actor = { loc: () => ({ isVisible: async () => false, innerText: async () => '' }) };
  const provided = services(actor, { browser: {
    sleep: async () => new Promise(resolve => setTimeout(resolve, 2)),
  } });
  provided.capabilities.actors = { get: (name: string) =>
    name === 'a' || name === 'b' ? actor : undefined };
  const result = await run({ do: 'expectAgreement', actors: ['a', 'b'],
    testid: 'total', within: 1 }, provided);
  assert.equal(result.status, 'failed');
  assert.match(result.summary ?? '', /missing or unreadable/);
});

test('expect can verify a persisted form value', async () => {
  const locator = {
    waitFor: async () => {},
    evaluate: async () => 'INPUT',
    inputValue: async () => 'staff',
  };
  const actor = { loc: () => locator };
  const result = await run({ do: 'expect', actor: 'a', testid: 'support-assignee',
    value: 'staff' }, services(actor));
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.observation, { visible: true, value: 'staff' });
});

test('expect can verify an element attribute', async () => {
  const locator = {
    waitFor: async () => {},
    getAttribute: async (name: string) => name === 'data-state' ? 'on' : null,
  };
  const actor = { loc: () => locator };
  const result = await run({ do: 'expect', actor: 'a', testid: 'notification-preference',
    attribute: 'data-state', value: 'on' }, services(actor));
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.observation, { visible: true, attribute: 'data-state', value: 'on' });
});

test('ordered text and unavailable controls are explicit implementation-neutral observations', async () => {
  const items = {
    filter: () => items,
    allInnerTexts: async () => ['Coffee Grinder', 'Air Purifier'],
  };
  const scope = {
    filter: () => scope,
    first: () => scope,
    locator: () => items,
  };
  const disabled = {
    filter: () => disabled,
    first: () => disabled,
    isVisible: async () => true,
    isDisabled: async () => true,
    getAttribute: async () => null,
  };
  const actor = {
    page: {
      locator: (selector: string) => selector.includes('item-list') ? scope : disabled,
    },
  };
  const provided = services(actor);
  const ordered = await run({ do: 'expectSequence', actor: 'a', testid: 'item-name',
    in: { testid: 'item-list' }, equals: ['Coffee Grinder', 'Air Purifier'] }, provided);
  assert.equal(ordered.status, 'passed');
  assert.deepEqual(
    (ordered.observation as { values: string[] }).values,
    ['Coffee Grinder', 'Air Purifier'],
  );

  const unavailable = await run({ do: 'expectUnavailable', actor: 'a', testid: 'buy-now',
    within: 1 }, provided);
  assert.equal(unavailable.status, 'passed');
  assert.deepEqual(unavailable.observation, { unavailable: true, reason: 'disabled' });
});

test('element counts use visible observations instead of hidden duplicate markup', async () => {
  const matches = {
    filter: () => ({ count: async () => 2 }),
  };
  const actor = { page: { locator: () => matches } };
  const result = await run({ do: 'expectElementCount', actor: 'a', testid: 'row',
    contains: 'item', equals: 2, within: 1 }, services(actor));
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.observation, { count: 2 });
});

test('element counts can be scoped to a matching parent', async () => {
  const child = { filter: () => ({ count: async () => 1 }) };
  const parent = { filter: () => parent, first: () => parent, locator: () => child };
  const actor = { page: { locator: () => parent } };
  const result = await run({ do: 'expectElementCount', actor: 'a', testid: 'payment-record',
    in: { testid: 'order-item', contains: 'Desk Lamp' }, equals: 1, within: 1 }, services(actor));
  assert.equal(result.status, 'passed');
  assert.deepEqual(result.observation, { count: 1 });
});

test('browser timeouts are application evidence while crashes and code bugs remain harness failures', async () => {
  const timeout = Object.assign(new Error('locator.click: element was never actionable'),
    { name: 'TimeoutError' });
  const timedOut = await run({ do: 'click', actor: 'a', testid: 'submit' },
    services({ loc: () => ({ click: async () => { throw timeout; } }) }));
  assert.equal(timedOut.status, 'failed');
  assert.equal(timedOut.code, 'application_failure');

  const crashed = await run({ do: 'click', actor: 'a', testid: 'submit' },
    services({ loc: () => ({ click: async () => {
      throw new Error('locator.click: Target page, context or browser has been closed');
    } }) }));
  assert.equal(crashed.status, 'harness_failure');
  assert.equal(crashed.code, 'unclassified_exception');

  const bug = await run({ do: 'click', actor: 'a', testid: 'submit' },
    services({ loc: () => ({ click: async () => { throw new TypeError('executor bug'); } }) }));
  assert.equal(bug.status, 'harness_failure');
  assert.equal(bug.code, 'unclassified_exception');
});
