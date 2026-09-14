import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createNamedActionsCapability } from '../src/actions/named-action-runtime.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('review access waits for purchase acceptance and handles an orders dialog before reading item IDs', async t => {
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const actors = new Map<string, unknown>();
  for (const name of ['owner', 'stranger']) {
    const page = await browser.newPage();
    await page.setContent(`<button data-role="orders-toggle" onclick="document.querySelector('dialog').showModal()">Orders</button>
      <div id="catalog"><div data-role="item-card" data-buy-input='{"itemId":7}'>
      <button data-role="item-name" onclick="document.getElementById('catalog').hidden=true; document.getElementById('detail').hidden=false">Keyboard</button>
      <button data-role="buy-now">Buy</button></div></div>
      <div id="detail" data-role="item-detail" hidden>Keyboard <span data-role="review-item">eligible progression review</span></div>
      <dialog><button data-role="overlay-close" onclick="document.querySelector('dialog').close()">Close</button>
        <div data-role="order-item">Keyboard</div></dialog>`);
    actors.set(name, { name, page, context: page.context(), writes: [{ headers: { authorization: `Bearer ${name}` } }],
      loc: (id: string, options: { contains?: string } = {}) => {
        let locator = page.locator(`[data-role="${id}"]`).filter({ visible: true });
        if (options.contains) locator = locator.filter({ hasText: options.contains });
        return locator.first();
      } });
  }
  const calls: number[] = [];
  let purchased = false;
  const sleep = async () => {};
  const interaction = { defaultWithin: 300, testId: (id: string) => `[data-role="${id}"]`,
    expand: (value: string) => value, sleep };
  const capabilities = { actors: { get: (name: string) => actors.get(name) },
    'browser-interaction': interaction, 'browser-observation': interaction,
    'transport-observation': { ...interaction, verification: { verified() {}, unverified() {} } },
    'named-actions': createNamedActionsCapability({ actions: [], backend: 'postgres', url: 'http://fixture.test',
      lastCalls: { get: () => null, set() {} }, sleep, now: Date.now,
      fetchImpl: async (url, options) => {
        if (url === 'http://fixture.test/api/items/7/buy') {
          // The write must finish before the review request. A browser click
          // which only starts this request does not establish that prerequisite.
          await new Promise(resolve => setTimeout(resolve, 50));
          purchased = true;
          return { status: 201, ok: true, text: async () => '' };
        }
        assert.equal(url, 'http://fixture.test/api/items/7/reviews');
        const status = purchased && options.headers?.authorization === 'Bearer owner' ? 201 : 403;
        calls.push(status);
        return { status, ok: status === 201, text: async () => '' };
      } }),
  };
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-review-access.json'), 'utf8'))).features[0]!;
  const steps = feature.criteria[0]!.steps;
  // Accounts already exist in this fixture. Run the scenario's remaining setup
  // and actual actions against a page that removes catalog visibility on detail.
  for (const step of [...feature.setup.filter(step => step.do !== 'signUp'),
    ...steps.slice(0, steps.findIndex(step => step.do === 'freshClient'))]) {
    const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities });
    assert.equal(result.status, 'passed', `${step.do}: ${result.summary}`);
  }
  assert.deepEqual(calls, [201, 403]);
  const oldOrder = await executeAction(ACTION_REGISTRY, steps[0]!.do, steps[0]!, { capabilities });
  assert.equal(oldOrder.status, 'failed', 'reading the owner catalog after openItem reproduces the old false failure');
  assert.deepEqual(calls, [201, 403], 'the old order fails before contacting the server');
});
