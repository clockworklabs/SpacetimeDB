import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createNamedActionsCapability } from '../src/actions/actor-transport-action-executors.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('shipping accounting accepts a correct sale and rejects a missing sale or duplicate shipping charges', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-shipping-accounting.json');
  const steps = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source })
    .features[0]!.criteria[0]!.steps;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const defect of ['none', 'missing-sale', 'stock-twice', 'revenue-twice']) {
      const context = await browser.newContext();
      try {
        let east = 70, west = 30, revenue = 0, status = '';
        // The fixture supplies signed-in actors; the actual measured actions run below.
        const actors = new Map();
        for (const name of ['customer', 'admin', 'staff']) {
          const page = await context.newPage();
          await page.exposeFunction('buy', () => {
            status = 'pending';
            if (defect !== 'missing-sale') { east--; revenue += 89; }
          });
          await page.route('http://shipping-accounting.test/**', route => route.fulfill({ contentType: 'text/html', body: `
            <span id="current-user">${name === 'customer' ? 'shipping-accounting' : name}</span>
            <button id="admin-link">Admin</button><button id="staff-link">Staff</button>
            <div id="admin-revenue">${revenue}</div>
            <div id="item-card">Keyboard<button id="buy-now" onclick="buy().then(()=>location.reload())">Buy</button></div>
            <button id="orders-toggle">Orders</button>
            ${status ? `<div id="order-item" data-ship-input='{"orderId":"1"}'>Keyboard<span id="order-status">${status}</span></div>` : ''}
          ` }));
          await page.goto('http://shipping-accounting.test/');
          actors.set(name, { name, page, context, record: () => {},
            writes: [{ url: 'http://shipping-accounting.test/', headers: { authorization: `Bearer ${name}` } }],
            loc: (id: string, options?: {
            contains?: string; scope?: { testid: string; contains?: string };
          }) => {
            const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid))
              .filter({ hasText: options.scope.contains }).first() : page;
            const loc = root.locator(stableElementSelector(id)).filter({ visible: true });
            return (options?.contains ? loc.filter({ hasText: options.contains }) : loc).first();
          } });
        }
        const sleep = async () => new Promise<void>(resolve => setTimeout(resolve, 20));
        const capability = { recorded: new Map<string, number>(), defaultWithin: 500,
          expand: (value: string) => value, scopedUser: (value: string) => value,
          testId: stableElementSelector, sleep };
        // Staff ships through the declared named action with its own credentials.
        const named = createNamedActionsCapability({ backend: 'postgres', url: 'http://shipping-accounting.test',
          lastCalls: { get: () => null, set: () => {} }, sleep,
          fetchImpl: async (_url, options) => {
            assert.equal(options.headers?.authorization, 'Bearer staff');
            assert.deepEqual(JSON.parse(String(options.body)), { orderId: '1' });
            status = 'shipped';
            if (defect === 'stock-twice') west--;
            if (defect === 'revenue-twice') revenue += 89;
            return { status: 200, ok: true, text: async () => '{}' };
          } });
        const transport = { defaultWithin: 500, expand: (value: string) => value, sleep,
          verification: { unverified: () => {}, verified: () => {} } };
        let failed = null;
        for (const step of steps) {
          const result = await executeAction(ACTION_REGISTRY, step.do,
            Object.hasOwn(step, 'within') ? { ...step, within: 500 } : step, { capabilities: {
            actors: { get: (name: string) => actors.get(name) }, 'browser-interaction': capability,
            'browser-observation': capability, clock: { sleep }, 'named-actions': named, 'transport-observation': transport,
            'database-read': { getStock: async (input: { item: string; warehouse?: string }) => ({
              backend: 'postgres', item: input.item, quantity: input.warehouse === 'East' ? east : input.warehouse === 'West' ? west : east + west,
            }) },
          } });
          if (result.status !== 'passed') { failed = { step, result }; break; }
        }
        if (defect === 'none') assert.equal(failed, null, failed?.result.summary ?? undefined);
        else {
          assert(failed, defect);
          assert.equal(failed.step.do, defect === 'revenue-twice' ? 'expectNumber' : 'dbExpectStock', failed.result.summary ?? undefined);
          assert.equal(failed.result.status, 'failed');
        }
      } finally { await context.close(); }
    }
  } finally { await browser.close(); }
});
