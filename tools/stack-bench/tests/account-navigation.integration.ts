import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('cart, order, and settings probes preserve open inline panels and open closed dialogs', async () => {
  const root = join(STACK_BENCH_ROOT, 'tracks/ecommerce');
  const read = (path: string) => JSON.parse(readFileSync(join(root, path), 'utf8'));
  const graph = read('progression/ecommerce.json') as { nodes: Array<{ id: string; dependencies: Array<{ id: string }>; gradingGroups: string[] }> };
  const depth = (id: string): number => 1 + Math.max(0, ...graph.nodes.find(node => node.id === id)!.dependencies.map(edge => depth(edge.id)));
  const packs = readdirSync(join(root, 'composition/packs')).map(name => read(`composition/packs/${name}`)) as Array<{
    id: string; checks: Array<{ id: string; source: string; feature: number; criteria?: string[] }>;
  }>;
  const selected = graph.nodes.filter(node => depth(node.id) <= 3).flatMap(node => node.gradingGroups).flatMap(ref => {
    const [id, group] = ref.split('#');
    return packs.find(pack => pack.id === id)!.checks.filter(check => check.id === group);
  });
  const steps = selected.flatMap(check => {
    const scenario = compileScenarioDefinition(read(check.source));
    const feature = scenario.features.find(feature => feature.id === check.feature)!;
    return [...feature.setup, ...feature.criteria.filter(criterion => !check.criteria || check.criteria.includes(criterion.id))
      .flatMap(criterion => criterion.steps)]
      .filter(step => step.do === 'click' && ['cart-toggle', 'orders-toggle', 'notification-settings'].includes(step.testid ?? ''));
  });
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const initiallyOpen of [false, true]) {
      await page.setContent(`<button id="cart-toggle" onclick="document.querySelector('#cart').hidden = !document.querySelector('#cart').hidden">Cart</button>
        <section id="cart" ${initiallyOpen ? '' : 'hidden'}><span id="cart-total">10</span></section>
        <button id="orders-toggle" onclick="document.querySelector('#orders').hidden = !document.querySelector('#orders').hidden">Orders</button>
        <section id="orders" ${initiallyOpen ? '' : 'hidden'}><span data-role="order-item">Headphones</span><span data-role="order-item">Keyboard</span></section>
        <button id="notification-settings" onclick="document.querySelector('#settings').hidden = !document.querySelector('#settings').hidden">Settings</button>
        <section id="settings" ${initiallyOpen ? '' : 'hidden'}><input id="notification-order" type="checkbox"></section>`);
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
      for (const step of steps) {
        const sentinel = step.testid === 'cart-toggle' ? 'cart-total'
          : step.testid === 'orders-toggle' ? 'order-item' : 'notification-order';
        assert.equal(step.unlessVisible, sentinel);
        const result = await executeAction(ACTION_REGISTRY, 'click', step, { capabilities: {
          actors: { get: () => actor }, 'browser-interaction': {
            defaultWithin: 1000, expand: (value: string) => value, testId: stableElementSelector, sleep: async () => {},
          },
        } });
        assert.equal(result.status, 'passed', result.summary ?? undefined);
        assert.equal(await actor.loc(sentinel).isVisible(), true);
      }
    }
    await page.setContent('<button id="cart-toggle">Cart</button>');
    const brokenActor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
    const brokenCapabilities = { actors: { get: () => brokenActor },
      'browser-interaction': { defaultWithin: 100, expand: (value: string) => value, testId: stableElementSelector },
      'browser-observation': { defaultWithin: 100, expand: (value: string) => value, testId: stableElementSelector } };
    const open = steps.find(step => step.testid === 'cart-toggle')!;
    assert.equal((await executeAction(ACTION_REGISTRY, 'click', open, { capabilities: brokenCapabilities })).status, 'passed');
    const missing = await executeAction(ACTION_REGISTRY, 'expect', {
      do: 'expect', actor: open.actor, testid: 'cart-item', contains: 'Keyboard', within: 100,
    }, { capabilities: brokenCapabilities });
    assert.equal(missing.status, 'failed', 'opening a broken cart must not bypass the required item assertion');
  } finally { await browser.close(); }
});

test('purchase attribution requires a working private history, not a blank view', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/01-purchase-attribution.json');
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
  const steps = feature.criteria[0]!.steps.filter(step => step.actor === 'victim' && step.do === 'expect');
  assert.equal(feature.setup.at(-1)!.in?.contains, 'Coffee Grinder');
  assert.equal(steps.length, 2);
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const items of [[], ['Coffee Grinder'], ['Coffee Grinder', 'Desk Lamp']]) {
      await page.setContent(items.map(item => `<div data-role="order-item">${item}</div>`).join(''));
      const actor = { page, loc: (id: string, options?: { contains?: string }) => page.locator(stableElementSelector(id),
        { hasText: options?.contains }).filter({ visible: true }).first() };
      const results = [];
      for (const step of steps) results.push(await executeAction(ACTION_REGISTRY, 'expect', { ...step, within: 50 }, { capabilities: {
        actors: { get: () => actor }, 'browser-observation': { defaultWithin: 50,
          expand: (value: string) => value, testId: stableElementSelector,
          sleep: async () => new Promise(resolve => setTimeout(resolve, 1)) },
      } }));
      assert.equal(results.every(result => result.status === 'passed'), items.length === 1,
        `history ${JSON.stringify(items)} must pass only when own order is visible and other order is absent`);
    }
  } finally { await browser.close(); }
});

test('role assignment targets the account ID despite role text in every dropdown', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-staff-roles.json');
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent(['customer', 'staff'].map((name, index) => `
      <div data-role="staff-role-row" id="staff-role-account-${encodeURIComponent(name)}" data-account-id="${index + 1}">
        <span>${name}</span><select data-role="staff-role-select"><option>staff</option><option>inventory</option></select>
        <button data-role="staff-role-save" onclick="this.parentElement.dataset.saved = this.parentElement.querySelector('select').value">Save</button>
      </div>`).join(''));
    assert.equal(await page.locator('[data-role="staff-role-row"]').filter({ hasText: 'staff' }).count(), 2,
      'the fixture must reproduce the ambiguous dropdown text');
    const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
      const root = options?.scope
        ? page.locator(stableElementSelector(options.scope.testid), { hasText: options.scope.contains }).first() : page;
      return root.locator(stableElementSelector(id), { hasText: options?.contains }).first();
    } };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': {
      defaultWithin: 1000, expand: (value: string) => value, sleep: async () => {}, testId: stableElementSelector,
    } };
    for (const step of feature.setup.filter(step => step.do === 'fill' || step.testid === 'staff-role-save')) {
      const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities });
      assert.equal(result.status, 'passed', result.summary ?? undefined);
    }
    assert.equal(await page.locator('#staff-role-account-staff').getAttribute('data-saved'), 'inventory');
    assert.equal(await page.locator('#staff-role-account-customer').getAttribute('data-saved'), null);
    const replay = feature.criteria.flatMap(criterion => criterion.steps).find(step => step.do === 'replayAs')!;
    const target = replay.namedTarget as { testid: string; contains?: string; attribute: string };
    assert.equal(target.contains, undefined);
    assert.equal(await actor.loc(target.testid).getAttribute(target.attribute), '2');
  } finally { await browser.close(); }
});

test('role refusal checks permit inventory accounts without fulfilment navigation', async () => {
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-staff-roles.json');
  const criterion = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source })
    .features[0]!.criteria.find(criterion => criterion.id === '621b')!;
  const entry = criterion.steps.findIndex(step => step.testid === 'staff-link');
  assert(entry >= 0);
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const navigation of [false, true]) {
      await page.setContent(`<strong id="current-user">staff</strong>
        ${navigation ? '<button id="staff-link" onclick="document.body.dataset.opened = true">Staff</button>' : ''}`);
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)) };
      const capability = { defaultWithin: 100, expand: (value: string) => value,
        testId: stableElementSelector, sleep: async () => new Promise(resolve => setTimeout(resolve, 1)) };
      for (const step of criterion.steps.slice(entry, entry + 2)) {
        const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: {
          actors: { get: () => actor },
          'browser-interaction': capability, 'browser-observation': capability,
        } });
        assert.equal(result.status, 'passed', result.summary ?? undefined);
      }
      assert.equal(await page.locator('body').getAttribute('data-opened'), navigation ? 'true' : null);
    }
  } finally { await browser.close(); }
});

test('signup uses the visible form or the declared signup-toggle, never the sign-in toggle', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const revealed of [false, true]) {
      await page.setContent(`
        <form id="signup" ${revealed ? 'hidden' : ''}>
          <input id="signup-username"><input id="signup-password">
          <button id="signup-submit">Sign up</button>
        </form>
        <form id="signin" hidden>
          <input id="signin-username"><input id="signin-password">
          <button id="signin-submit">Sign in</button>
        </form>
        ${revealed ? '<button id="signup-toggle">Create account</button>' : ''}
        <button id="signin-toggle">Sign in</button>
        <strong id="current-user" hidden></strong>
        <script>
          window.signInClicks = 0;
          document.querySelector('#signin-toggle').onclick = () => {
            window.signInClicks++;
            document.querySelector('#signin').hidden = false;
          };
          var reveal = document.querySelector('#signup-toggle');
          if (reveal) reveal.onclick = () => { document.querySelector('#signup').hidden = false; };
          document.querySelector('#signup').onsubmit = event => {
            event.preventDefault();
            const current = document.querySelector('#current-user');
            current.textContent = document.querySelector('#signup-username').value;
            current.hidden = false;
          };
        </script>`);
      const actor = { page, loc: (id: string) => page.locator(`#${id}`) };
      const result = await executeAction(ACTION_REGISTRY, 'signUp',
        { do: 'signUp', actor: 'shopper', name: 'Alice' }, {
          capabilities: {
            actors: { get: () => actor },
            'browser-interaction': { defaultWithin: 1000, scopedUser: (name: string) => `${name}-scope`,
              testId: (id: string) => `#${id}` },
          },
        });
      assert.equal(result.status, 'passed', result.summary ?? JSON.stringify(result));
      assert.equal(await page.locator('#current-user').innerText(), 'Alice-scope');
      assert.equal(await page.locator('#signup-password').inputValue(), 'pw-Alice-scope');
      assert.equal(await page.evaluate(() => Reflect.get(window, 'signInClicks')), 0);
    }
  } finally { await browser.close(); }
});

test('signin opens a hidden form before the toggle in DOM order and accepts an already visible form', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const hidden of [true, false]) {
      await page.setContent(`
        <form id="signin" ${hidden ? 'hidden' : ''}>
          <input id="signin-username"><input id="signin-password">
          <button id="signin-submit">Sign in</button>
        </form>
        <button id="signin-toggle">Sign in</button>
        <strong id="current-user" hidden></strong>
        <script>
          window.signInClicks = 0;
          document.querySelector('#signin-toggle').onclick = () => {
            window.signInClicks++; document.querySelector('#signin').hidden = false;
          };
          document.querySelector('#signin').onsubmit = event => {
            event.preventDefault();
            const current = document.querySelector('#current-user');
            current.textContent = document.querySelector('#signin-username').value;
            current.hidden = false;
          };
        </script>`);
      const actor = { page, loc: (id: string) => page.locator(`#${id}`) };
      const result = await executeAction(ACTION_REGISTRY, 'signIn',
        { do: 'signIn', actor: 'shopper', name: 'Alice' }, {
          capabilities: {
            actors: { get: () => actor },
            'browser-interaction': { defaultWithin: 1000, scopedUser: (name: string) => `${name}-scope`,
              testId: (id: string) => `#${id}` },
          },
        });
      assert.equal(result.status, 'passed', result.summary ?? JSON.stringify(result));
      assert.equal(await page.locator('#current-user').innerText(), 'Alice-scope');
      assert.equal(await page.locator('#signin-password').inputValue(), 'pw-Alice-scope');
      assert.equal(await page.evaluate(() => Reflect.get(window, 'signInClicks')), hidden ? 1 : 0);
    }
  } finally { await browser.close(); }
});
