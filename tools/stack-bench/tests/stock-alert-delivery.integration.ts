import assert from 'node:assert/strict';
import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { chromium, type BrowserContext, type Page } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction, type ActionEvidence } from '../src/actions/action-contract.js';
import { createNamedActionsCapability } from '../src/actions/actor-transport-action-executors.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { createServer } from 'node:http';
import { gradeFeature } from '../grader/grade.js';

test('promotion access measures denied management and writes rather than link visibility', async t => {
  // Failure first: a visible denied link is valid; a write is forbidden even if its response says 403.
  let mode = '', codes: string[] = [];
  const requests: Array<{ code: string; authorized: boolean; status: number; committed: boolean }> = [];
  const evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    if (request.url === '/api/promotions' && request.method === 'POST') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      const { code } = JSON.parse(body);
      const authorized = request.headers.authorization === 'Bearer staff';
      const committed = authorized || mode !== 'denied';
      const status = authorized || mode === 'accepted-write' ? 200 : 403;
      if (committed) codes.push(code);
      requests.push({ code, authorized, status, committed });
      response.writeHead(status, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ ok: status === 200 })); return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><main></main><script>
      const user=sessionStorage.getItem('user'), main=document.querySelector('main');
      window.getSessionToken=()=>sessionStorage.getItem('user');
      if (!user) {
        main.innerHTML=['signin','signup'].map(kind=>'<form data-kind="'+kind+'"><input id="'+kind+'-username"><input id="'+kind+'-password" type="password"><button id="'+kind+'-submit">Continue</button></form>').join('');
        for (const form of main.querySelectorAll('form')) form.onsubmit=event=>{
          event.preventDefault();sessionStorage.setItem('user',document.querySelector('#'+form.dataset.kind+'-username').value);location.href='/';
        };
      } else {
        main.innerHTML='<span id="current-user">'+user+'</span><a id="promotions-link" href="/promotions">Promotions</a>';
        if (user==='staff') {
          main.innerHTML+='<a id="staff-link" href="/promotions">Staff</a><section id="staff-area"><form id="promotion-form"><input id="promotion-code"><input id="promotion-discount"><input id="promotion-start"><input id="promotion-end"><input id="promotion-limit"><button id="promotion-submit">Save</button></form><section id="rules"></section></section>';
          document.querySelector('#rules').innerHTML=${JSON.stringify(codes)}.map(code=>'<div data-role="promotion-item">'+code+'</div>').join('');
          document.querySelector('#promotion-form').onsubmit=async event=>{
            event.preventDefault();const input=id=>document.querySelector('#promotion-'+id).value;
            await fetch('/api/promotions',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+user},body:JSON.stringify({code:input('code'),discountPercent:Number(input('discount')),startMicros:4070908800000000,endMicros:4102444740000000,usageLimit:Number(input('limit'))})});
            location.href='/promotions';
          };
        } else main.innerHTML+='<p>Promotion management requires staff access.</p>';
      }
    </script>`);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (mode of ['denied', 'accepted-write', 'write-then-deny']) await t.test(mode, async () => {
      codes = []; requests.length = 0;
      const definition = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
        'tracks/ecommerce/scenarios/progression-promotion-rules.json'), 'utf8'));
      const selected = definition.features[0];
      selected.criteria = selected.criteria.filter((criterion: { id: string }) => criterion.id === '620b');
      for (const step of [...selected.setup, ...selected.criteria[0].steps]) {
        if ('within' in step) step.within = 700;
        if ('settleMs' in step) step.settleMs = 0;
      }
      const grade = await gradeFeature(browser, compileScenarioDefinition(definition).features[0]!,
        { url, level: definition.level, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: `promotion-access-${mode}`, roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 700 });
      const row = { mode, grade, requests: [...requests], codes: [...codes] }; evidence.push(row);
      assert.equal(grade.setupEvidence.status, 'passed', JSON.stringify(row));
      assert.equal(grade.criteria[0]!.evidence.status, mode === 'denied' ? 'passed' : 'failed', JSON.stringify(row));
      assert.equal(requests.filter(request => request.authorized && request.code === 'ACCESS10').length, 1);
      assert.equal(requests.filter(request => !request.authorized && request.code === 'HACK10').length, 1,
        'the customer replay must reach the real endpoint; a link-only failure is not the boundary test');
      assert.equal(codes.includes('HACK10'), mode !== 'denied');
      if (mode === 'write-then-deny') assert.equal((grade.criteria[0]!.evidence.actions.at(-1)?.evidence as ActionEvidence | undefined)?.action.id, 'expect');
    });
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_PROMOTION_ACCESS_EVIDENCE) {
      const { writeFileSync } = await import('node:fs');
      writeFileSync(process.env.STACK_BENCH_PROMOTION_ACCESS_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_PROMOTION_ACCESS_EVIDENCE=<file> node --test --test-name-pattern="promotion access measures" dist/tests/stock-alert-delivery.integration.js', evidence,
      }, null, 2));
    }
  }
});

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
        await customerPage.route('http://controls.test/**', route => route.fulfill({ contentType: 'text/html',
          body: '<strong>Customer</strong>' }));
        await customerPage.goto('http://controls.test/');
        const customer = { name: 'customer', page: customerPage, context,
          loc: (id: string) => customerPage.locator(stableElementSelector(id)),
          writes: [{ url: 'http://controls.test/write', headers: { authorization: 'Bearer customer' } }] };
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
        let reloads = 0;
        let writeReloads = 0;
        let unreloadedWrite = false;
        for (const step of steps) {
          if (step.do === 'reload') {
            reloads++;
            // The scenario also reloads before the write and after the replay; the reload after the write decides.
            if (unreloadedWrite) {
              unreloadedWrite = false;
              writeReloads++;
              await page.waitForFunction(count => performance.getEntriesByType('resource').filter(entry => entry.name.endsWith('/write')).length === count, sequential ? 2 : 1);
              assert.equal(writes, sequential ? 2 : 1);
              assert.equal(kind === 'role' ? await page.locator('#staff-role-select').inputValue()
                : (await page.locator('[data-role="promotion-item"]').innerText()).slice(0, 8), kind === 'role' ? 'staff' : 'ACCESS10');
            }
          }
          if (step.do === 'click' && ['promotion-submit', 'staff-role-save'].includes(String(step.testid))) unreloadedWrite = true;
          const input = ['expect', 'click'].includes(step.do) ? { ...step, within: 300 } : step;
          const result = await executeAction(ACTION_REGISTRY, step.do, input, { capabilities });
          if (result.status !== 'passed') { assert.equal(result.status, 'failed', result.summary ?? undefined); failure = String(step.testid); break; }
        }
        assert.equal(writeReloads, 1, 'the optimistic write must be followed by a reload');
        assert.equal(loads, (layout === 'route' ? 2 : 1) + reloads, 'the control must read the server state after a real reload');
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
      ['631c', 'transition', 'inline-no-toggle', null],
      ['631c', 'transition', 'modal', null],
      ['631c', 'transition', 'page', null],
      ['631c', 'transition', 'settings-page', null],
      ['631c', 'transition', 'settings-toggle', null],
      ['631c', 'transition', 'catalog-settings', null],
      ['631c', 'transition', 'menu-toggle', null],
      ['631c', 'transition', 'menu-settings', null],
      ['631c', 'transition', 'inline-menu', null],
      ['631c', 'transition', 'menu-broken', 'click/before-restock'],
      ['631c', 'transition', 'menu-missing', 'click/before-restock'],
      ['631c', 'transition', 'settings-noop', 'click/before-restock'],
      ['631c', 'transition', 'no-controls', 'click/before-restock'],
      ['631c', 'pending', 'page', 'expect/after-restock'],
      ['631c', 'premature', 'page', 'expectElementCount/before-restock'],
      ['631a', 'transition', 'inline', null],
      ['631a', 'transition', 'modal', null],
      ['631a', 'transition', 'page', null],
      ['631a', 'transition', 'settings-page', null],
      ['631a', 'transition', 'menu-toggle', null],
      ['631a', 'pending', 'page', 'expectElementCount/before-restock'],
      ['631a', 'duplicate', 'page', 'expectElementCount/after-restock'],
      ['631a', 'duplicate', 'settings-page', 'expectElementCount/after-restock'],
      ['631b', 'private', 'page', null],
      ['631b', 'private', 'settings-page', null],
      ['631b', 'private', 'menu-settings', null],
      ['631b', 'pending', 'page', 'expect/before-restock'],
      ['631b', 'leak', 'page', 'expect/before-restock'],
      ['631b', 'leak', 'settings-page', 'expect/before-restock'],
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
      const navigation = { catalog: 0, settings: 0, toggle: 0, account: 0 };
      const observations: { action: string; actor?: string; control?: string; status: string; summary: string | null }[] = [];
      const actors = new Map<string, { name: string; page: Page; context: BrowserContext; record(): void;
        writes: { url: string; headers: { authorization: string } }[];
        loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string | RegExp } }) => ReturnType<Page['locator']> }>();
      try {
        const makeActor = async (name: string) => {
          const context = await browser.newContext();
          contexts.push(context);
          const page = await context.newPage();
          await page.exposeFunction('recordNavigation', (control: keyof typeof navigation) => { navigation[control]++; });
          await page.exposeFunction('readNotifications', async (user: string) => {
            assert.equal(pendingWrite, false, 'the driver must await the write before its fresh read');
            if (layout === 'inline-loading') await new Promise(resolve => setTimeout(resolve, 30));
            if (restocked) readsAfterRestock++;
            const count = deliveries + (elapsed >= deliveryDue ? 1 : 0) + (elapsed >= duplicateDue ? 1 : 0);
            return user === 'stock-subscriber' || mode === 'leak' ? count : 0;
          });
          await context.route('http://app.test/**', route => route.fulfill({ contentType: 'text/html', body: `<form id="signin">
            <input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button>
            </form><strong id="current-user" hidden></strong>
            <button id="notifications-toggle" ${layout.startsWith('settings-') || layout.startsWith('menu-') || ['catalog-settings', 'no-controls', 'inline-no-toggle', 'inline-menu'].includes(layout) ? 'hidden' : ''}>Notifications</button><button id="catalog-link">Catalog</button>
            <button id="notification-settings" ${layout.startsWith('menu-') || ['catalog-settings', 'no-controls', 'inline-menu'].includes(layout) ? 'hidden' : ''}>Settings</button>
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
                if (${layout === 'inline' || layout === 'inline-no-toggle' || layout === 'inline-menu'}) await open();
                if (${layout === 'inline-loading'}) void open();
                current.hidden = false;
              };
              current.onclick = () => {
                window.recordNavigation('account');
                if (${layout === 'menu-toggle' || layout === 'inline-menu'}) document.querySelector('#notifications-toggle').hidden = false;
                if (${layout === 'menu-settings'}) document.querySelector('#notification-settings').hidden = false;
                if (${layout === 'menu-missing'}) current.textContent += ' Menu';
              };
              document.querySelector('#notifications-toggle').onclick = () => {
                window.recordNavigation('toggle');
                document.body.dataset.toggleClicked = 'true';
                if (panel.hidden) return open();
                panel.hidden = true;
              };
              document.querySelector('#catalog-link').onclick = () => {
                window.recordNavigation('catalog');
                if (${layout === 'catalog-settings'}) document.querySelector('#notification-settings').hidden = false;
                if (${!layout.startsWith('inline')}) panel.hidden = true;
              };
              document.querySelector('#notification-settings').onclick = () => {
                window.recordNavigation('settings');
                if (${layout === 'settings-toggle'}) document.querySelector('#notifications-toggle').hidden = false;
                if (${layout === 'settings-page' || layout === 'catalog-settings' || layout === 'menu-settings'}) return open();
              };
              document.querySelector('#overlay-close')?.addEventListener('click', event => {
                panel.hidden = true; event.target.hidden = true;
              });
            </script>` }));
          await page.goto('http://app.test/');
          const actor = { name, page, context, record() {}, writes: [{ url: 'http://app.test/api/session', headers: { authorization: 'Bearer fixture-admin' } }],
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
          observations.push({ action: step.do, actor: step.actor, control: step.testid,
            status: result.status, summary: result.summary ?? null });
          if (result.status !== 'passed') {
            assert.equal(result.status, 'failed', result.summary ?? undefined);
            failed = `${step.do}/${restocked ? 'after-restock' : 'before-restock'}`;
            break;
          }
        }
        const evidenceFile = process.env.STACK_BENCH_NOTIFICATION_NAVIGATION_EVIDENCE;
        if (evidenceFile) {
          mkdirSync(dirname(evidenceFile), { recursive: true });
          const screenshot = join(dirname(evidenceFile), `${id}-${mode}-${layout}.png`);
          await actors.get('subscriber-fresh')!.page.screenshot({ path: screenshot });
          appendFileSync(evidenceFile, `${JSON.stringify({ id, mode, layout, scenario: name,
            expectedFailure, failed, navigation, observations, screenshot })}\n`);
        }
        assert.equal(failed, expectedFailure, `${id}: ${mode}/${layout}`);
        if (['inline', 'inline-loading', 'inline-no-toggle', 'inline-menu', 'modal', 'page'].includes(layout)) {
          assert.equal(navigation.catalog, 0, 'a visible notification destination needs no catalog navigation');
          assert.equal(navigation.settings, 0, 'a visible notification destination needs no settings navigation');
          assert.equal(navigation.account, 0, 'a visible feed or opener needs no account navigation');
        }
        if (['menu-toggle', 'menu-settings'].includes(layout)) assert.ok(navigation.account > 0, 'reach the feed through the account menu');
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
