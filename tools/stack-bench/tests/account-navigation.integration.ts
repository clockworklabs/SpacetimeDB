import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

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
