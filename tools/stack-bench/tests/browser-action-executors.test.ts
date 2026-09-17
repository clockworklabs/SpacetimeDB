import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium, errors } from 'playwright';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { compileActionInput } from '../src/composition/definition-compiler.js';
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
    assert(plugin.capabilities.includes('actors') || plugin.capabilities.includes('browser-observation'), id);
  }
});

test('script canary detects execution after DOM removal and rejects missing observers', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const provided = services({ page });
    assert.equal((await run({ do: 'expectNoScriptExecution', actor: 'a' }, provided)).status, 'inconclusive');
    assert.equal((await run({ do: 'armScriptCanary', actor: 'a' }, provided)).status, 'passed');
    const payload = '<img src="data:,bad-image" onerror="window.__stackBenchScriptCanary().then(() => console.log(\'canary-delivered\'))">';
    await page.setContent('<main></main>');
    await page.locator('main').evaluate((element, text) => { element.textContent = text; }, payload);
    assert.equal((await run({ do: 'expectNoScriptExecution', actor: 'a' }, provided)).status, 'passed');
    const delivered = page.waitForEvent('console', { predicate: message => message.text() === 'canary-delivered', timeout: 5000 });
    await page.setContent(payload);
    await delivered;
    await page.setContent('<main>replacement document</main>');
    assert.equal((await run({ do: 'expectNoScriptExecution', actor: 'a' }, provided)).status, 'failed');
    await page.evaluate(() => {
      delete (globalThis as unknown as { __stackBenchScriptCanary?: unknown }).__stackBenchScriptCanary;
    });
    assert.equal((await run({ do: 'expectNoScriptExecution', actor: 'a' }, provided)).status, 'inconclusive');
  } finally { await browser.close(); }
});

test('UI failures retain bounded observations but exclude passwords and unproven missing choices', async () => {
  for (const sensitive of [false, true]) {
    const result = await run({ do: 'expectNumber', actor: 'a', testid: 'stock', equals: 99, within: 1,
      in: { testid: sensitive ? 'secret' : 'item-card', contains: 'Bluetooth Speaker' } },
    services({ page: { locator: () => ({ filter: () => ({ count: async () => 1 }) }) },
      loc: () => ({ waitFor: async () => {}, evaluate: async () => 'DIV',
      innerText: async () => '100' }) }));
    assert.equal(result.finding?.kind, 'number-mismatch');
    if (sensitive) assert.doesNotMatch(result.summary ?? '', /Bluetooth Speaker/);
    else assert.match(result.summary ?? '', /entry matching "Bluetooth Speaker"/);
  }
  for (const operation of ['expect', 'expectNumber']) {
    const result = await run({ do: operation, actor: 'a', testid: 'stock',
      ...(operation === 'expect' ? { contains: 'Keyboard' } : {}),
      in: { testid: 'warehouse', contains: 'East' }, ...(operation === 'expectNumber' ? { equals: 1 } : {}) },
    services({ loc: () => ({ waitFor: async () => { throw new errors.TimeoutError('not visible'); } }) }));
    assert.equal(result.finding?.kind, 'control-missing');
    assert.match(result.summary ?? '', operation === 'expect' ? /East.*Keyboard/ : /East/);
  }
  for (const password of [false, true]) {
    const result = await run({ do: 'expect', actor: 'a', testid: 'field', value: 'Approved', within: 1 },
      services({ loc: () => ({ waitFor: async () => {},
        evaluate: async () => 'INPUT', inputValue: async () => password ? 'RAW_PASSWORD' : 'Pending',
        getAttribute: async () => password ? 'password' : 'text' }) }));
    assert.equal(result.finding?.kind, 'value-mismatch');
    if (password) assert.doesNotMatch(JSON.stringify(result.finding), /RAW_PASSWORD|Approved/);
    else assert.match(result.summary ?? '', /Pending.*Approved/);
  }
  for (const present of [false, true]) {
    const select = { tagName: 'SELECT', options: [{ value: 'weekly', label: present ? 'Weekly' : 'Daily' }] };
    const result = await run({ do: 'fill', actor: 'a', testid: 'frequency', text: 'Weekly' }, services({ loc: () => ({
      waitFor: async () => {}, evaluate: async (read: (element: typeof select) => unknown) => read(select),
      selectOption: async () => { throw new errors.TimeoutError('locator.selectOption: Timeout exceeded'); },
    }) }));
    assert.equal(result.finding?.kind, present ? 'page-timeout' : 'choice-missing');
    if (!present) assert.match(result.summary ?? '', /required choice "Weekly"/);
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

test('observation and selection errors cannot become missing-control findings', async () => {
  for (const operation of ['expect', 'expectNumber', 'waitUntilAbsent', 'fill']) {
    const result = await run({ do: operation, actor: 'a', testid: 'field',
        ...(operation === 'expectNumber' ? { equals: 1 } : {}),
        ...(operation === 'fill' ? { text: 'Weekly' } : {}) }, services({ loc: () => ({
        waitFor: async () => { throw new Error('locator.waitFor: invalid selector'); },
      }) }));
    assert.equal(result.status, 'harness_failure', `${operation}: ${result.summary}`);
    assert.equal(result.finding, null);
  }
  for (const stage of ['select', 'options']) {
    let reads = 0;
    const result = await run({ do: 'fill', actor: 'a', testid: 'field', text: 'Weekly' },
      services({ loc: () => ({ waitFor: async () => {},
        evaluate: async () => {
          if (reads++ === 0) return 'SELECT';
          throw new Error('locator.evaluate: observation script failed');
        },
        selectOption: async () => {
          if (stage === 'select') throw new Error('locator.selectOption: Protocol error');
          throw new errors.TimeoutError('locator.selectOption: Timeout exceeded');
        },
      }) }));
    assert.equal(result.status, 'harness_failure', result.summary ?? undefined);
    assert.equal(result.finding, null);
  }
});

test('message order requires observed messages and compares the merged sender sequence', async () => {
  for (const [left, right, expected] of [
    [[], [], 'failed'],
    [['AA-1', 'BB-1'], ['AA-1', 'BB-1'], 'passed'],
    [['AA-1', 'BB-1'], ['BB-1', 'AA-1'], 'failed'],
  ] as const) {
    const provided = services({});
    provided.capabilities.actors = { get: (name: string) => ({ page: {
      locator: () => ({ allInnerTexts: async () => name === 'a' ? left : right }),
    } }) };
    const result = await run({ do: 'expectOrderMatches', actors: ['a', 'b'], prefix: '(?:AA|BB)' }, provided);
    assert.equal(result.status, expected, result.summary ?? undefined);
  }
});

test('optional clicks operate enabled controls and skip unavailable controls', async () => {
  for (const [visible, enabled] of [[false, false], [true, false], [true, true]]) {
    let clicks = 0;
    const provided = services({ loc: () => ({ isVisible: async () => visible,
      isDisabled: async () => !enabled, click: async () => { clicks += 1; } }) });
    const result = await run({ do: 'click', actor: 'a', testid: 'buy-now', ifAvailable: true, within: 1 }, provided);
    assert.equal(result.status, 'passed');
    assert.equal(clicks, visible && enabled ? 1 : 0);
  }
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

test('stock observations accept an explicit zero-stock state without relaxing other numeric controls', async () => {
  let rendered = ' Out of stock ';
  const locator = {
    waitFor: async () => {},
    isVisible: async () => true,
    evaluate: async () => 'SPAN',
    innerText: async () => rendered,
  };
  const provided = services({ loc: () => locator });
  provided.capabilities.actors = { get: () => ({ loc: () => locator }) };
  const recorded = await run({ do: 'recordNumber', actor: 'a', testid: 'item-stock', as: 'stock' }, provided);
  assert.equal(recorded.status, 'passed');
  assert.equal(provided.recorded.get('stock'), 0);
  assert.equal((await run({ do: 'expectNumber', actor: 'a', testid: 'item-stock', equals: 0 }, provided)).status, 'passed');
  assert.equal((await run({ do: 'expectAgreement', actors: ['a', 'b'], testid: 'item-stock', numeric: true }, provided)).status, 'passed');
  assert.equal((await run({ do: 'recordNumber', actor: 'a', testid: 'order-total', as: 'total' }, provided)).status, 'failed');
  rendered = 'Stock unavailable';
  assert.equal((await run({ do: 'recordNumber', actor: 'a', testid: 'item-stock', as: 'stock' }, provided)).status, 'failed');
  rendered = 'Stock: -1';
  await run({ do: 'recordNumber', actor: 'a', testid: 'item-stock', as: 'stock' }, provided);
  assert.equal(provided.recorded.get('stock'), -1);
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
  assert.match(result.summary ?? '', /shows "private value" that must not appear/);
  assert.doesNotMatch(JSON.stringify(result.finding), /contains private value/);
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

test('relative element counts preserve existing rows and accept an empty baseline', async () => {
  for (const initial of [0, 3]) {
    let count = initial;
    const loc = { filter: () => loc, count: async () => count };
    const provided = services({ page: { locator: () => loc } });
    assert.equal((await run({ do: 'recordNumber', actor: 'a', testid: 'row',
      as: 'before', count: true }, provided)).status, 'passed');
    assert.equal(provided.recorded.get('before'), initial);
    count++;
    assert.equal((await run({ do: 'expectElementCount', actor: 'a', testid: 'row',
      relativeTo: 'before', plus: 1 }, provided)).status, 'passed');
    count++;
    assert.equal((await run({ do: 'expectElementCount', actor: 'a', testid: 'row',
      relativeTo: 'before', plus: 1, within: 1 }, provided)).status, 'failed');
  }
});

test('relative count assertions reject missing records and invalid count targets', async () => {
  const loc = { filter: () => loc, count: async () => 0 };
  const provided = services({ page: { locator: () => loc } });
  const input = { do: 'expectElementCount', actor: 'a', testid: 'row', relativeTo: 'before' };
  const missing = await run(input, provided);
  assert.equal(missing.status, 'inconclusive');
  assert.equal(missing.code, 'inconclusive');
  assert.equal(missing.finding?.kind, 'assertion-without-action');
  for (const base of [-1, 0.5, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
    provided.recorded.set('before', base);
    assert.equal((await run(input, provided)).status, 'harness_failure');
  }
  for (const fields of [{}, { equals: 0, relativeTo: 'before' }, { equals: 0, plus: 1 },
    { relativeTo: 'before', plus: 0.5 }, { equals: -1 }]) {
    assert.throws(() => compileActionInput({ do: 'expectElementCount', actor: 'a',
      testid: 'row', ...fields }));
  }
});

test('unactionable controls are application evidence while crashes and code bugs remain harness failures', async () => {
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

test('a click that removes its target before timing out is inconclusive and is not repeated', async () => {
  const { chromium } = await import('playwright');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent(`<button id="close" onclick="this.remove();document.body.dataset.clicks='1';
      const until=Date.now()+2000;while(Date.now()<until){}">Close</button>`);
    let clicks = 0;
    const result = await run({ do: 'click', actor: 'a', testid: 'close', within: 1000 },
      services({ loc: () => ({ click: async (options: Parameters<ReturnType<typeof page.locator>['click']>[0]) => {
        clicks += 1;
        await page.locator('#close').click(options);
      } }) }));
    assert.equal(result.status, 'inconclusive', result.summary ?? undefined);
    assert.equal(clicks, 1);
    assert.equal(await page.locator('#close').count(), 0);
    assert.equal(await page.locator('body').getAttribute('data-clicks'), '1');
    assert.match(JSON.stringify(result.observation), /performing click action/);
  } finally { await browser.close(); }
});


test('relative number bounds use recorded values and report the resolved bound', async () => {
  let value = 70;
  const provided = services({ loc: () => ({ waitFor: async () => {},
    evaluate: async () => 'SPAN', innerText: async () => String(value) }) });
  provided.recorded.set('initial', 72);
  const step = { do: 'expectNumber', actor: 'a', testid: 'timer', relativeTo: 'initial', plus: -1, within: 1 };
  assert.equal((await run({ ...step, comparison: 'atMost' }, provided)).status, 'passed');
  value = 74;
  assert.equal((await run({ ...step, plus: 1, comparison: 'atLeast' }, provided)).status, 'passed');
  value = 72;
  const failed = await run({ ...step, comparison: 'atMost' }, provided);
  assert.equal(failed.status, 'failed');
  assert.match(JSON.stringify(failed), /"atMost":71/);
  assert.doesNotMatch(JSON.stringify(failed), /"equals":71/);
  const missing = await run({ ...step, relativeTo: 'missing', comparison: 'atMost' }, provided);
  assert.equal(missing.status, 'inconclusive');
});


test('optional navigation waits for delayed controls or inline content', async () => {
  for (const inline of [false, true]) {
    let ready = false;
    let clicks = 0;
    const provided = services({ loc: (id: string) => ({
      isVisible: async () => ready && (inline ? id === 'low-stock-item' : id === 'low-stock-link'),
      isDisabled: async () => false,
      evaluateAll: async () => true,
      click: async () => { clicks += 1; },
    }) }, { browser: { sleep: async () => { ready = true; } } });
    const result = await run({ do: 'click', actor: 'a', testid: 'low-stock-link',
      ifAvailable: true, unlessVisible: 'low-stock-item', within: 1000 }, provided);
    assert.equal(result.status, 'passed', result.summary ?? undefined);
    assert.equal(clicks, inline ? 0 : 1);
  }
});

test('navigation retries a replaced destination without repeating clicks or hiding errors', async () => {
  for (const failure of ['once', 'always', 'unrelated']) {
    let reads = 0;
    let clicks = 0;
    const provided = services({ loc: () => ({
      isVisible: async () => true,
      evaluateAll: async () => {
        reads += 1;
        if (failure === 'unrelated') throw new Error('browser disconnected');
        if (failure === 'always' || reads === 1) throw new Error('Element is not attached to the DOM');
        return true;
      },
      click: async () => { clicks += 1; },
    }) }, { browser: { sleep: async (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) } });
    const result = await run({ do: 'click', actor: 'a', testid: 'low-stock-link',
      unlessVisible: 'low-stock-item', within: failure === 'always' ? 30 : 1000 }, provided);
    assert.equal(result.status, failure === 'once' ? 'passed' : 'harness_failure');
    assert.equal(clicks, 0);
    assert.equal(reads === 1, failure === 'unrelated');
  }
});

test('unreadable destinations never trigger a blind toggle click', async () => {
  for (const optional of [false, true]) {
    let clicks = 0;
    const provided = services({ loc: (id: string) => ({
      isVisible: async () => id === 'order-item',
      isDisabled: async () => false,
      evaluateAll: async () => { throw Object.assign(new Error('observation timed out'), { name: 'TimeoutError' }); },
      click: async () => { clicks += 1; throw Object.assign(new Error('required control missing'), { name: 'TimeoutError' }); },
    }) });
    const result = await run({ do: 'click', actor: 'a', testid: 'orders-toggle',
      unlessVisible: 'order-item', ifAvailable: optional, within: 10 }, provided);
    assert.equal(result.status, 'failed');
    assert.equal(clicks, 0);
  }
});

test('covered navigation accepts a destination that finishes loading without another click', async () => {
  const { chromium } = await import('playwright');
  const browser = await chromium.launch({ headless: true });
  try {
    for (const mode of ['delayed', 'absent', 'button']) {
      const page = await browser.newPage();
      try {
        await page.setContent(`<button id="sales-link" onclick="this.dataset.clicks=Number(this.dataset.clicks||0)+1">Sales</button>
          <dialog id="modal">Loading</dialog><script>
          ${mode === 'button' ? '' : 'modal.showModal();'}
          ${mode === 'delayed' ? "setTimeout(()=>modal.innerHTML='<div id=category-row>Audio</div>',150);" : ''}
          </script>`);
        const result = await run({ do: 'click', actor: 'a', testid: 'sales-link',
          unlessVisible: 'category-row', ifAvailable: true, within: 600 },
        services({ loc: (id: string) => page.locator(`#${id}`) }, {
          browser: { sleep: async (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) },
        }));
        assert.equal(result.status, mode === 'absent' ? 'failed' : 'passed', result.summary ?? undefined);
        assert.equal(await page.locator('#sales-link').getAttribute('data-clicks'), mode === 'button' ? '1' : null);
        if (mode === 'absent') assert.equal(result.finding?.kind, 'control-blocked');
      } finally { await page.close(); }
    }
  } finally { await browser.close(); }
});

test('covered navigation preserves cancellation and browser failures', async () => {
  for (const cancelled of [false, true]) {
    const controller = new AbortController();
    let reads = 0;
    let clicks = 0;
    const provided = services({ loc: () => ({
      isVisible: async () => { reads += 1; return reads > 1; },
      isDisabled: async () => false,
      evaluateAll: async () => true,
      click: async () => {
        clicks += 1;
        if (cancelled) controller.abort('cancelled by test');
        throw Object.assign(new Error(cancelled ? 'intercepts pointer events' : 'browser disconnected'),
          { name: cancelled ? 'TimeoutError' : 'Error' });
      },
    }) });
    const result = await executeAction(ACTION_REGISTRY, 'click', {
      do: 'click', actor: 'a', testid: 'sales-link', unlessVisible: 'category-row', within: 100,
    }, { capabilities: provided.capabilities, signal: controller.signal });
    assert.equal(result.status, cancelled ? 'inconclusive' : 'harness_failure');
    if (cancelled) assert.equal(result.code, 'cancelled');
    assert.equal(reads, 1);
    assert.equal(clicks, 1);
  }
});

test('reload timeouts fail the app while proven browser crashes stay harness failures', async () => {
  for (const [message, expected] of [
    ['page.reload: Timeout 20000ms exceeded', 'failed'],
    ['page.reload: Target crashed', 'harness_failure'],
  ]) {
    const provided = services({ loc: () => ({}), page: { reload: async () => {
      throw Object.assign(new Error(message), { name: 'TimeoutError' });
    } } });
    const result = await run({ do: 'reload', actor: 'a', settleMs: 0 }, provided);
    assert.equal(result.status, expected);
  }
});

test('containsText polls the selected item without requiring an exact status value', async () => {
  const step = { do: 'expect', actor: 'a', testid: 'order-item', contains: 'Keyboard',
    containsText: 'returned', ignoreCase: true, within: 1 };
  assert.doesNotThrow(() => compileActionInput(step));
  for (const conflict of [{ value: 'returned' }, { attribute: 'data-state' }, { absent: true }, { containsText: '' }]) {
    assert.throws(() => compileActionInput({ ...step, ...conflict }));
  }
  for (const returned of [false, true]) {
    let ready = false;
    const provided = services({ loc: (_id: string, options: { contains: string }) => {
      assert.equal(options.contains, 'Keyboard');
      return { waitFor: async () => {}, evaluate: async () => 'ARTICLE', getAttribute: async () => null,
        innerText: async () => `Keyboard · shipped${ready && returned ? ' · Returned' : ''}` };
    } }, { browser: { sleep: async () => { ready = true; } } });
    const result = await run(step, provided);
    assert.equal(result.status, returned ? 'passed' : 'failed');
    if (!returned) assert.match(result.summary!, /Keyboard.*does not contain "returned"/);
  }
});

test('failed disappearance identifies the matched entry and scope', async () => {
  const provided = services({ loc: () => ({ waitFor: async () => { throw new errors.TimeoutError('Timeout'); } }) });
  const result = await run({ do: 'waitUntilAbsent', actor: 'a', testid: 'item-card',
    contains: 'Coffee Grinder', in: { testid: 'search-results' }, within: 1 }, provided);
  assert.equal(result.status, 'failed');
  assert.match(result.summary!, /Coffee Grinder/);
  assert.match(result.summary!, /search-results/);
});

// The catalog navigation can remove the old destination between these two reads.
test('navigation clicks once when its old destination disappears during observation', async () => {
  const { chromium } = await import('playwright');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(150);
    await page.setContent('<button id="profile-link" onclick="this.dataset.clicked=true">Profile</button><div id="profile-address-summary">Address</div>');
    const result = await run({ do: 'click', actor: 'a', testid: 'profile-link',
      unlessVisible: 'profile-address-summary', within: 150 }, services({ loc: (id: string) => {
      const locator = page.locator('#' + id);
      if (id !== 'profile-address-summary') return locator;
      return new Proxy(locator, { get(target, key) {
        if (key === 'isVisible') return async () => {
          const visible = await target.isVisible();
          await page.locator('#profile-address-summary').evaluate(element => element.remove());
          return visible;
        };
        const value = Reflect.get(target, key);
        return typeof value === 'function' ? value.bind(target) : value;
      } });
    } }));
    assert.equal(result.status, 'passed', result.summary ?? undefined);
    assert.equal(await page.locator('#profile-link').getAttribute('data-clicked'), 'true');
  } finally { await browser.close(); }
});
