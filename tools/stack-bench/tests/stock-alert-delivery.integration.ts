import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium, type BrowserContext, type Page } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createNamedActionsCapability } from '../src/actions/actor-transport-action-executors.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('promotion and role access controls reject optimistic writes that disappear on reload', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const kind of ['promotion', 'role'] as const) for (const persists of [true, false])
      for (const layout of kind === 'promotion' && persists ? ['inline', 'toggle', 'route'] : ['inline'])
        for (const sequential of kind === 'promotion' && persists ? [false, true] : [false]) {
      const context = await browser.newContext();
      try {
        let saved = kind === 'role' ? 'inventory' : '';
        let writes = 0;
        let loads = 0;
        await context.route('http://controls.test/**', async route => {
          if (route.request().method() === 'POST') {
            writes++;
            if (persists) saved = route.request().postData()!;
            await route.fulfill({ status: 200, body: '' });
            return;
          }
          loads++;
          await route.fulfill({ contentType: 'text/html', body: `
            <form id="signin"><input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button></form>
            <strong id="current-user" hidden></strong>
            <button id="admin-link">Admin</button><button id="staff-link">Staff</button><button id="promotions-link">Promotions</button>
            <section id="promotion-panel" ${layout === 'toggle' || layout === 'route' && !route.request().url().endsWith('/promotions') ? 'hidden' : ''}>
            <input id="promotion-code"><input id="promotion-discount"><input id="promotion-start"><input id="promotion-end"><input id="promotion-limit">
            <button id="promotion-submit">Save promotion</button><section id="promotions">${kind === 'promotion' && saved ? `<div data-role="promotion-item">${saved}</div>` : ''}</section>
            </section>
            <div data-role="staff-role-account-staff"><select id="staff-role-select"><option>inventory</option><option>staff</option></select><button id="staff-role-save">Save role</button></div>
            <script>
              document.querySelector('#staff-role-select').value = ${JSON.stringify(kind === 'role' ? saved : 'inventory')};
              document.querySelector('#signin').onsubmit = event => { event.preventDefault(); const current = document.querySelector('#current-user'); current.textContent = document.querySelector('#signin-username').value; current.hidden = false; };
              document.querySelector('#promotions-link').onclick = () => {
                if (${layout === 'route'}) location.href = '/promotions';
                else if (${layout === 'toggle'}) document.querySelector('#promotion-panel').hidden = !document.querySelector('#promotion-panel').hidden;
              };
              document.querySelector('#promotion-submit').onclick = async () => {
                const code = document.querySelector('#promotion-code').value;
                document.querySelector('#promotions').innerHTML = '<div data-role="promotion-item">' + code + '<span data-role="promotion-discount">10</span><span data-role="promotion-start">2099-01-01</span><span data-role="promotion-end">2099-12-31</span><span data-role="promotion-limit">2</span></div>';
                await fetch('/write', { method: 'POST', body: code });
              };
              document.querySelector('#staff-role-save').onclick = async () => { await fetch('/write', { method: 'POST', body: document.querySelector('#staff-role-select').value }); };
            </script>` });
        });
        const page = await context.newPage();
        await page.goto('http://controls.test/');
        const actor = { name: kind === 'role' ? 'replayAdmin' : 'staff', page, writes: [],
          loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => {
            const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid), { hasText: options.scope.contains }).first() : page;
            return root.locator(stableElementSelector(id), { hasText: options?.contains }).first();
          } };
        const capability = { defaultWithin: 300, recorded: new Map<string, number>(),
          expand: (value: string) => value, scopedUser: (value: string) => value,
          testId: stableElementSelector, sleep: async () => {} };
        const customerPage = await context.newPage();
        await customerPage.setContent('<strong>Customer</strong>');
        const customer = { name: 'customer', page: customerPage,
          loc: (id: string) => customerPage.locator(stableElementSelector(id)),
          writes: [{ headers: { authorization: 'Bearer customer' } }] };
        let replayRequests = 0;
        const named = createNamedActionsCapability({ actions: [], backend: 'postgres', url: 'http://controls.test',
          lastCalls: { get: () => null, set: () => {} }, sleep: async () => {}, now: Date.now,
          fetchImpl: async (url, options) => {
            assert.equal(url, 'http://controls.test/api/promotions');
            assert.equal(JSON.parse(String(options.body)).code, 'HACK10');
            assert.equal(options.headers?.authorization, 'Bearer customer');
            replayRequests++;
            return { status: 403, ok: false, text: async () => '' };
          } });
        const capabilities = { actors: { get: (name: string) => name === 'customer' ? customer : actor },
          'browser-interaction': capability, 'browser-observation': capability, 'named-actions': named,
          'transport-observation': { ...capability, verification: { verified: () => {}, unverified: () => {} } } };
        const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
          'tracks/ecommerce/scenarios', kind === 'role' ? 'progression-staff-roles.json' : 'progression-promotion-rules.json'), 'utf8')));
        const criterion = scenario.features[0]!.criteria.find(c => c.id === (kind === 'role' ? '621b' : '620b'))!;
        const steps = [...(sequential ? scenario.features[0]!.criteria.find(c => c.id === '620a')!.steps : []),
          ...(kind === 'promotion' ? criterion.steps : criterion.steps.slice(0, criterion.steps.findIndex(step => step.actor !== actor.name)))];
        let failure: string | null = null;
        for (const step of steps) {
          if (step.do === 'reload') {
            await page.waitForFunction(count => performance.getEntriesByType('resource').filter(entry => entry.name.endsWith('/write')).length === count, sequential ? 2 : 1);
            assert.equal(writes, sequential ? 2 : 1);
            assert.equal(kind === 'role' ? await page.locator('#staff-role-select').inputValue()
              : (await page.locator('[data-role="promotion-item"]').innerText()).slice(0, 8), kind === 'role' ? 'staff' : 'ACCESS10');
          }
          const input = ['expect', 'click'].includes(step.do) ? { ...step, within: 300 } : step;
          const result = await executeAction(ACTION_REGISTRY, step.do, input, { capabilities });
          if (result.status !== 'passed') { assert.equal(result.status, 'failed', result.summary ?? undefined); failure = String(step.testid); break; }
        }
        assert.equal(loads, layout === 'route' ? 3 : 2, 'the control must read the server state after a real reload');
        assert.equal(failure, persists ? null : kind === 'role' ? 'staff-role-select' : 'promotion-item', `${kind}/${layout}/sequential=${sequential}`);
        assert.equal(replayRequests, kind === 'promotion' && persists ? 1 : 0);
      } finally { await context.close(); }
    }
  } finally { await browser.close(); }
});

test('stock alerts accept fresh load-on-open views and reject missing, premature, late duplicate, or private delivery', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    // Vary valid layouts on the positive path; failure cases need only one layout.
    for (const [id, mode, layout, expectedFailure] of [
      ['631c', 'transition', 'inline', null],
      ['631c', 'transition', 'inline-loading', null],
      ['631c', 'transition', 'modal', null],
      ['631c', 'transition', 'page', null],
      ['631c', 'pending', 'page', 'expect/after-restock'],
      ['631c', 'premature', 'page', 'expectElementCount/before-restock'],
      ['631a', 'transition', 'inline', null],
      ['631a', 'transition', 'modal', null],
      ['631a', 'transition', 'page', null],
      ['631a', 'pending', 'page', 'expectElementCount/before-restock'],
      ['631a', 'duplicate', 'page', 'expectElementCount/after-restock'],
      ['631b', 'private', 'page', null],
      ['631b', 'pending', 'page', 'expect/before-restock'],
      ['631b', 'leak', 'page', 'expect/before-restock'],
    ] as const) {
      const contexts: BrowserContext[] = [];
      const deliveries = mode === 'premature' || id !== '631c' && mode !== 'pending' ? 1 : 0;
      let elapsed = 0;
      let deliveryDue = Infinity;
      let duplicateDue = Infinity;
      let pendingWrite = false;
      let restocked = false;
      let readsAfterRestock = 0;
      let freshClients = 0;
      let waitMs = 0;
      const actors = new Map<string, { name: string; page: Page;
        writes: { headers: { authorization: string } }[];
        loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => ReturnType<Page['locator']> }>();
      try {
        const makeActor = async (name: string) => {
          const context = await browser.newContext();
          contexts.push(context);
          const page = await context.newPage();
          await page.exposeFunction('readNotifications', async (user: string) => {
            assert.equal(pendingWrite, false, 'the driver must await the write before its fresh read');
            if (layout === 'inline-loading') await new Promise(resolve => setTimeout(resolve, 30));
            if (restocked) readsAfterRestock++;
            const count = deliveries + (elapsed >= deliveryDue ? 1 : 0) + (elapsed >= duplicateDue ? 1 : 0);
            return user === 'stock-subscriber' || mode === 'leak' ? count : 0;
          });
          await page.setContent(`<form id="signin">
            <input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button>
            </form><strong id="current-user" hidden></strong>
            <button id="notifications-toggle">Notifications</button><button id="catalog-link">Catalog</button>
            ${layout === 'modal' ? '<button id="overlay-close" hidden>Close</button>' : ''}
            <section id="notifications" data-role="notifications-panel" aria-busy="true" style="min-height:24px" hidden>Loading</section>
            <div data-role="admin-location-row" data-restock-input='{"itemId":1,"warehouseId":2,"quantity":1}'>Air Purifier East</div>
            <button id="admin-link">Admin</button>
            <script>
              var panel = document.querySelector('#notifications');
              var current = document.querySelector('#current-user');
              async function open() {
                panel.hidden = false; panel.setAttribute('aria-busy', 'true');
                const count = await window.readNotifications(current.textContent);
                panel.innerHTML = '<div data-role="notification-item"><span data-role="stock-alert-delivery">Air Purifier</span></div>'.repeat(count);
                if (!count && current.textContent === 'stock-subscriber') {
                  panel.innerHTML = '<div data-role="notification-item">Air Purifier request pending</div>';
                }
                panel.setAttribute('aria-busy', 'false');
                document.querySelector('#overlay-close')?.removeAttribute('hidden');
              }
              document.querySelector('#signin').onsubmit = async event => {
                event.preventDefault(); current.textContent = document.querySelector('#signin-username').value;
                if (${layout === 'inline'}) await open();
                if (${layout === 'inline-loading'}) void open();
                current.hidden = false;
              };
              document.querySelector('#notifications-toggle').onclick = () => {
                document.body.dataset.toggleClicked = 'true';
                if (panel.hidden) return open();
                panel.hidden = true;
              };
              document.querySelector('#catalog-link').onclick = () => {
                if (${!layout.startsWith('inline')}) panel.hidden = true;
              };
              document.querySelector('#overlay-close')?.addEventListener('click', event => {
                panel.hidden = true; event.target.hidden = true;
              });
            </script>`);
          const actor = { name, page, writes: [{ headers: { authorization: 'Bearer fixture-admin' } }],
            loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => {
              const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid),
                { hasText: options.scope.contains }).first() : page;
              return root.locator(stableElementSelector(id), { hasText: options?.contains }).first();
            } };
          actors.set(name, actor);
          return name;
        };
        await makeActor('subscriber');
        await makeActor('other');
        await makeActor('admin');
        const sleep = async (ms: number) => { elapsed += ms; await new Promise(resolve => setTimeout(resolve, 1)); };
        const capability = { defaultWithin: 500, recorded: new Map<string, number>(),
          expand: (value: string) => value, scopedUser: (value: string) => value,
          testId: stableElementSelector, sleep,
          clients: { fresh: async (_actor: unknown, name: string) => {
            freshClients++;
            return makeActor(`${name}-fresh`);
          } } };
        const named = createNamedActionsCapability({ actions: [], backend: 'postgres', url: 'http://app.test',
          lastCalls: { get: () => null, set: () => {} }, sleep, now: () => elapsed,
          fetchImpl: async (url, options) => {
            assert.equal(url, 'http://app.test/api/admin/restock');
            assert.deepEqual(JSON.parse(String(options.body)), { itemId: 1, warehouseId: 2, quantity: 1 });
            pendingWrite = true;
            await new Promise(resolve => setTimeout(resolve, 30));
            if (mode !== 'pending' && mode !== 'premature') {
              if (id === '631c') deliveryDue = elapsed + 5000;
              if (mode === 'duplicate') duplicateDue = elapsed + 5000;
            }
            pendingWrite = false;
            restocked = true;
            return { status: 200, ok: true, text: async () => '' };
          } });
        const capabilities = { actors: { get: (name: string) => actors.get(name) },
          'browser-interaction': capability, 'browser-observation': capability,
          'named-actions': named, 'transport-observation': { ...capability,
            verification: { verified: () => {} } },
          clock: { sleep: async (ms: number) => {
            assert.equal(pendingWrite, false, 'the observation window must start after the write completes');
            waitMs += ms; await sleep(ms);
          } } };
        const name = id === '631c' ? 'progression-stock-alert-delivery.json' : 'progression-stock-alerts.json';
        const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
          'tracks/ecommerce/scenarios', name), 'utf8')));
        const criterion = scenario.features[0]!.criteria.find(criterion => criterion.id === id)!;
        let failed: string | null = null;
        for (const step of criterion.steps) {
          const input = ['expect', 'expectElementCount', 'click'].includes(step.do)
            ? { ...step, within: 500 } : step;
          const result = await executeAction(ACTION_REGISTRY, step.do, input, { capabilities });
          if (result.status !== 'passed') {
            assert.equal(result.status, 'failed', result.summary ?? undefined);
            failed = `${step.do}/${restocked ? 'after-restock' : 'before-restock'}`;
            break;
          }
        }
        assert.equal(failed, expectedFailure, `${id}: ${mode}/${layout}`);
        if (layout === 'inline-loading') {
          assert.equal(await actors.get('subscriber-fresh')!.page.locator('body')
            .getAttribute('data-toggle-clicked'), null, 'an open loading panel must not be toggled closed');
        }
        if (restocked) {
          assert(readsAfterRestock > 0, 'delivery must be read again, without a push update');
          assert(freshClients >= 2, 'the second observation must use another independent client');
          assert.equal(waitMs, 10000, 'delayed delivery or persistent duplicates are sampled after the authored interval');
        }
      } finally { await Promise.all(contexts.map(context => context.close())); }
    }
  } finally { await browser.close(); }
});
