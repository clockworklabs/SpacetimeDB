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
