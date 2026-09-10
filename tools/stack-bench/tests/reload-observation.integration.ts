import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { chromium, type Page } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';

test('reservation restart readback rejects returned stock with a lost cart entry', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/03-deferred-durability.json');
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source })
    .features.find(feature => feature.id === 314)!;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const retained of [true, false]) {
      const page = await browser.newPage();
      await page.route('http://reservation.test/', route => route.fulfill({ contentType: 'text/html',
        body: `<span id="current-user">durable-reservation</span>
          <div id="item-card">Desk Lamp<span id="item-stock">100</span></div>
          <button id="cart-toggle">Cart</button><section id="cart" hidden>
          ${retained ? '<div id="cart-item">Desk Lamp<span id="cart-item-expired">Expired</span></div>' : ''}
          </section><script>document.querySelector('#cart-toggle').onclick=()=>{document.querySelector('#cart').hidden=false;};</script>` }));
      await page.goto('http://reservation.test/');
      const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => {
        const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid),
          { hasText: options.scope.contains }).first() : page;
        return root.locator(stableElementSelector(id), { hasText: options?.contains }).first();
      } };
      const capability = { defaultWithin: 300, recorded: new Map([['before', 100]]),
        expand: (value: string) => value, scopedUser: (value: string) => value,
        testId: stableElementSelector, sleep: async () => {} };
      let failed: string | undefined;
      for (const step of feature.criteria[0]!.steps) {
        const input = { ...step, ...(['expect', 'expectNumber'].includes(step.do) ? { within: 300 } : {}),
          ...('settleMs' in step ? { settleMs: 0 } : {}) };
        const result = await executeAction(ACTION_REGISTRY, step.do, input,
          { capabilities: { actors: { get: () => actor }, 'browser-interaction': capability,
            'browser-observation': capability } });
        if (result.status !== 'passed') { failed = String(step.testid ?? step.do); break; }
      }
      assert.equal(failed, retained ? undefined : 'cart-item-expired');
      await page.close();
    }
  } finally { await browser.close(); }
});

test('order status probes reject negative labels that contain the expected status', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const actor = { page, loc: () => page.locator('#order-status') };
    const capability = { defaultWithin: 300, expand: (value: string) => value,
      testId: stableElementSelector, sleep: async () => {} };
    for (const [file, status, misleading] of [
      ['02-fulfilment-ship.json', 'shipped', 'not shipped'],
      ['02-order-cancellation-history.json', 'cancelled', 'cancellation failed'],
      ['03-order-delivery.json', 'delivered', 'not delivered'],
    ]) {
      const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios', file!);
      const definition = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source });
      const step = definition.features.flatMap(feature => feature.criteria.flatMap(criterion => criterion.steps))
        .find(step => step.testid === 'order-status' && step.value === status)!;
      assert(step);
      for (const text of [status, `  ${status}  `, misleading]) {
        await page.setContent(`<span id="order-status">${text}</span>`);
        const result = await executeAction(ACTION_REGISTRY, step.do, { ...step, within: 300 },
          { capabilities: { actors: { get: () => actor }, 'browser-observation': capability } });
        assert.equal(result.status, text?.trim() === status ? 'passed' : 'failed', result.summary ?? undefined);
      }
    }
  } finally { await browser.close(); }
});

test('restock reload observations work on persistent pages and reopened panels without hiding pending work', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/03-server-time.json');
    const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
    const criterion = feature.criteria[0]!;
    const entry = criterion.steps.findIndex(step => step.testid === 'admin-link');
    assert(entry >= 0, 'the measured post-reload path must contain the declared admin entry');
    const steps = criterion.steps.slice(entry);
    const reorderSource = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-automatic-reorder.json');
    const reorder = compileScenarioDefinition(JSON.parse(readFileSync(reorderSource, 'utf8')),
      { source: reorderSource }).features[0]!;
    const reorderEntry = reorder.setup.filter(step => step.do === 'click' && step.actor === 'admin');
    const actor = { page, loc: (id: string, options?: { contains?: string }) => {
      const locator = page.locator(stableElementSelector(id));
      return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
    } };
    const capability = { defaultWithin: 500, expand: (value: string) => value,
      testId: stableElementSelector, sleep: async () => new Promise(resolve => setTimeout(resolve, 1)) };
    for (const layout of ['page', 'panel', 'nested', 'broken']) {
      await page.route('http://restock.test/', route => route.fulfill({ contentType: 'text/html',
        body: `${layout === 'page' ? '' : '<button id="admin-link">Admin</button>'}
          <section id="admin" ${layout === 'page' ? '' : 'hidden'}>
            ${layout === 'nested' || layout === 'broken' ? '<button id="restocks-link">Restocks</button>' : ''}
            <section id="restocks" ${layout === 'nested' || layout === 'broken' ? 'hidden' : ''}>
              <button id="schedule-restock-submit">Schedule</button>
              <div id="pending-restock-item">Espresso Machine</div>
            </section>
          </section><script>document.querySelector('#admin-link')?.addEventListener('click', () => {
            document.querySelector('#admin').hidden = false;
          });
          document.querySelector('#restocks-link')?.addEventListener('click', () => {
            ${layout === 'broken' ? '' : "document.querySelector('#restocks').hidden = false;"}
          });</script>` }));
      await page.goto('http://restock.test/');
      // Automatic reorder observes the same pending list and must also reach nested tools.
      if (layout === 'nested') {
        for (const step of reorderEntry) {
          const result = await executeAction(ACTION_REGISTRY, step.do, step,
            { capabilities: { actors: { get: () => actor }, 'browser-interaction': capability } });
          assert.equal(result.status, 'passed', result.summary ?? undefined);
        }
        assert.equal(await page.locator('#pending-restock-item').isVisible(), true);
      }
      await page.reload();
      const results = [];
      for (const step of steps) results.push(await executeAction(ACTION_REGISTRY, step.do, step,
        { capabilities: { actors: { get: () => actor },
          'browser-interaction': capability, 'browser-observation': capability } }));
      assert.equal(results[0]!.status, 'passed', 'optional entry also permits an absent self-link');
      assert.equal(results[1]!.status, 'passed', 'optional in-area entry supports both inline and separate tools');
      assert.equal(results[2]!.status, layout === 'broken' ? 'failed' : 'passed');
      if (layout !== 'broken') {
        assert.equal(results[3]!.status, 'failed', 'visible pending work must fail absence');
        await page.locator('#pending-restock-item').evaluate(element => element.remove());
        const absent = steps[3]!;
        assert.equal((await executeAction(ACTION_REGISTRY, absent.do, absent,
          { capabilities: { actors: { get: () => actor }, 'browser-observation': capability } })).status, 'passed');
      }
      await page.unroute('http://restock.test/');
    }
  } finally { await browser.close(); }
});

test('shipping permits delayed writes without an open-page update', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/02-fulfilment-ship.json');
  const steps = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source })
    .features[0]!.criteria[0]!.steps;
  const submit = steps.findIndex(step => step.testid === 'ship-submit');
  const reload = steps.findIndex((step, index) => index > submit && step.do === 'reload');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let finished = false;
    await page.route('http://shipping.test/**', async route => {
      if (route.request().url().endsWith('/ship')) {
        await new Promise(resolve => setTimeout(resolve, 150));
        finished = true;
        await route.fulfill({ body: '{}' });
      } else {
        await route.fulfill({ contentType: 'text/html', body:
          finished ? '<p>Shipped</p>' : '<div id="queue-item">Keyboard<button id="ship-submit" onclick="fetch(\'/ship\', {method:\'POST\'})">Ship</button></div>' });
      }
    });
    await page.goto('http://shipping.test/');
    const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)) };
    const capability = { defaultWithin: 1000, expand: (value: string) => value,
      testId: stableElementSelector, sleep: async (ms: number) => { await new Promise(resolve => setTimeout(resolve, ms)); } };
    for (const step of steps.slice(submit, reload)) {
      const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: {
        actors: { get: () => actor }, 'browser-interaction': capability,
        'browser-observation': capability,
      } });
      assert.equal(result.status, 'passed', result.summary ?? undefined);
    }
    assert(finished, 'the delayed submit must complete before the scenario reaches reload');
    assert.equal(await page.locator('#queue-item').count(), 1, 'the submitting page need not update');
    await page.reload();
    assert.equal(await page.locator('#queue-item').count(), 0, 'fresh read sees the completed write');
  } finally { await browser.close(); }
});

test('low-stock live observations stay open while another client restocks', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/02-low-stock.json');
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
  assert(feature.actors);
  const steps = feature.criteria.find(criterion => criterion.id === '5a')!.steps;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const live of [true, false]) {
      const pages: Map<string, Page> = new Map(await Promise.all(feature.actors.map(async name => [name, await browser.newPage()] as const)));
      const watcher = pages.get('admin')!;
      let stock = 3;
      for (const [name, page] of pages) {
        await page.exposeFunction('changeStock', async (quantity: number) => {
          stock += quantity;
          if (live) await watcher.locator('#low-stock-item').evaluate((row, total) => {
            (row as HTMLElement).hidden = total > 10;
          }, stock);
        });
        await page.setContent(`<form id="signin">
          <input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button>
          </form><strong id="current-user" hidden></strong>
          <button id="admin-link">Admin</button>
          <section id="admin-area" ${name === 'admin' ? '' : 'hidden'}>
            <button id="low-stock-link">Low stock</button>
            <section id="inventory" ${name === 'admin' ? 'hidden' : ''}>
              <div id="admin-location-row">Air Purifier East<input id="restock-input"><button id="restock-submit">Restock</button></div>
            </section>
            <section id="low-stock" ${name === 'admin' ? '' : 'hidden'}><h2>Low stock</h2><div id="low-stock-item">Air Purifier</div></section>
          </section>
          <div id="item-card">Air Purifier<button id="buy-now">Buy now</button></div>
          <script>
            document.querySelector('#signin').onsubmit = event => {
              event.preventDefault(); const user = document.querySelector('#current-user');
              user.textContent = document.querySelector('#signin-username').value; user.hidden = false;
            };
            // Re-entering Admin keeps its current subtab, as a normal app can do.
            document.querySelector('#admin-link').onclick = () => { document.querySelector('#admin-area').hidden = false; };
            document.querySelector('#low-stock-link').onclick = () => {
              document.querySelector('#inventory').hidden = true; document.querySelector('#low-stock').hidden = false;
            };
            document.querySelector('#restock-submit').onclick = () => window.changeStock(Number(document.querySelector('#restock-input').value));
            document.querySelector('#buy-now').onclick = () => window.changeStock(-1);
          </script>`);
      }
      const capability = { defaultWithin: 500, expand: (value: string) => value,
        scopedUser: (value: string) => value, testId: stableElementSelector,
        sleep: async () => { await new Promise(resolve => setTimeout(resolve, 1)); } };
      const actors = { get: (name: string) => {
        const page = pages.get(name)!;
        return { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => {
          const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid), { hasText: options.scope.contains }).first() : page;
          return root.locator(stableElementSelector(id), { hasText: options?.contains }).first();
        } };
      } };
      let failed: string | undefined;
      for (const step of steps) {
        if (step.actor === 'admin') assert(['expect', 'waitUntilAbsent'].includes(step.do), 'the observer must not navigate or reload');
        const input = ['click', 'expect', 'waitUntilAbsent'].includes(step.do) ? { ...step, within: 500 } : step;
        const result = await executeAction(ACTION_REGISTRY, step.do, input, { capabilities: {
          actors, 'browser-interaction': capability, 'browser-observation': capability,
        } });
        if (result.status !== 'passed') { failed = step.do; break; }
        assert.equal(await watcher.locator('#low-stock').isVisible(), true);
      }
      assert.equal(failed, live ? undefined : 'waitUntilAbsent');
      assert.equal(stock, live ? 10 : 11);
      await Promise.all([...pages.values()].map(page => page.close()));
    }
  } finally { await browser.close(); }
});
