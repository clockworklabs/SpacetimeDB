import assert from 'node:assert/strict';
import test from 'node:test';

import { ACTION_REGISTRY } from '../action-catalog.mjs';
import { createActionRunContext, executeAction } from '../action-contract.mjs';
import {
  BROWSER_ACTION_IDS,
  BROWSER_ACTION_IMPLEMENTATIONS,
  parseRenderedNumber,
} from '../browser-action-executors.mjs';

function services(actor, overrides = {}) {
  const recorded = new Map();
  const browser = {
    defaultWithin: 5000,
    expand: value => value === '{room:test}' ? 'test-scoped' : value,
    recorded: { get: key => recorded.get(key), set: (key, value) => recorded.set(key, value) },
    sleep: async () => {},
    testId: id => `[data-testid="${id}"]`,
    ...overrides.browser,
  };
  return {
    capabilities: {
      actors: { get: name => name === 'a' ? actor : undefined },
      'browser-interaction': browser,
      'browser-observation': browser,
      clock: { sleep: overrides.clockSleep ?? (async () => {}) },
    },
    recorded,
  };
}

async function run(input, provided) {
  return executeAction(ACTION_REGISTRY, input.do, input, createActionRunContext({
    capabilities: provided.capabilities,
    implementations: BROWSER_ACTION_IMPLEMENTATIONS,
    attempt: { id: `test-${input.do}` },
  }));
}

test('the extracted executor registry is exact and every migrated action has bounded metadata', () => {
  assert.deepEqual(Object.keys(BROWSER_ACTION_IMPLEMENTATIONS).sort(), BROWSER_ACTION_IDS);
  for (const id of BROWSER_ACTION_IDS) {
    const plugin = ACTION_REGISTRY.get(id);
    assert(plugin.deadline.timeoutMs > 0, id);
    assert(plugin.capabilities.includes('actors'), id);
    assert.match(plugin.evidence.type, /-evidence$/, id);
  }
});

test('timing executes through the contract and still rejects an unknown actor', async () => {
  const slept = [];
  const provided = services({}, { clockSleep: async ms => slept.push(ms) });
  const passed = await run({ do: 'wait', actor: 'a', ms: 17 }, provided);
  assert.equal(passed.status, 'passed');
  assert.deepEqual(passed.observation, { waitedMs: 17 });
  assert.deepEqual(slept, [17]);

  const missing = await run({ do: 'wait', actor: 'missing', ms: 1 }, provided);
  assert.equal(missing.status, 'failed');
  assert.equal(missing.code, 'application_failure');
  assert.equal(missing.summary, 'unknown actor "missing"');
});

test('interaction actions receive scoped values and preserve the legacy click options', async () => {
  const calls = [];
  const locator = {
    click: async options => calls.push(['click', options]),
  };
  const actor = {
    loc: (testid, options) => {
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
  assert.match(result.summary, /unexpectedly contains/);
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
  assert.match(blank.summary, /visible but empty/);

  rendered = 'East';
  const populated = await run({ do: 'expect', actor: 'a', testid: 'warehouse', nonEmpty: true },
    services(actor));
  assert.equal(populated.status, 'passed');
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
