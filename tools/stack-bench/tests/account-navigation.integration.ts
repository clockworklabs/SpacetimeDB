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
import { gradeFeature } from '../grader/grade.js';
import type { Browser } from 'playwright';

test('the real grader distinguishes an app prerequisite failure from its unexecuted assertion', async () => {
  const browser = await chromium.launch({headless:true});
  try {
    const routedBrowser = {newContext: async () => {
      const context = await browser.newContext();
      await context.route('http://prerequisite.test/**', route => route.fulfill({contentType:'text/html',
        body:'<span id="stock">100</span><span id="target">works</span>'}));
      return context;
    }} as unknown as Browser;
    for (const quantity of [100,99]) {
      const scenario = compileScenarioDefinition({schemaVersion:1,track:'ecommerce',level:1,name:'prerequisite',
        features:[{id:1,name:'probe',actors:['buyer'],setup:[{do:'expectNumber',actor:'buyer',testid:'stock',equals:quantity,within:200}],
          criteria:[{id:'target',desc:'target works',points:1,steps:[{do:'expect',actor:'buyer',testid:'target',contains:'works'}]}]}]});
      const result = await gradeFeature(routedBrowser,scenario.features[0]!,{
        url:'http://prerequisite.test',level:1,headed:false,selectedCheckKeys:[],nullControl:false,
      },{runId:'prerequisite',roomName:name=>name,url:'http://prerequisite.test',actions:[],spacetime:null,nullControl:false});
      assert.equal(result.setupEvidence.status, quantity===100?'passed':'failed', result.setupEvidence.summary ?? 'setup');
      assert.equal(result.criteria[0]!.evidence.status, quantity===100?'passed':'blocked');
      assert.equal(result.score,quantity===100?1:0);
      if(quantity===99)assert.deepEqual(result.criteria[0]!.evidence.actions,[]);
    }
  } finally {await browser.close();}
});

test('signout supports a direct button and account dialog but rejects missing or broken behavior', async () => {
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-account-signout.json'), 'utf8'))).features[0]!;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const layout of ['direct', 'account-dialog', 'missing', 'broken', 'wrong-account']) {
      await page.setContent(`
        <button id="current-user" onclick="document.querySelector('dialog').showModal()">ann</button>
        ${layout === 'direct' ? '<button id="signout">Sign out</button>' : ''}
        <dialog>${layout !== 'direct' && layout !== 'missing' ? '<button id="signout">Sign out</button>' : ''}</dialog>
        <form hidden><input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button></form>
        <script>(() => {
          const out = document.querySelector('#signout');
          if (out) out.onclick = () => {
            if ('${layout}' === 'broken') return;
            document.querySelector('dialog').close();
            document.querySelector('#current-user').hidden = true;
            out.hidden = true; document.querySelector('form').hidden = false;
          };
          document.querySelector('form').onsubmit = e => {
            e.preventDefault();
            const current = document.querySelector('#current-user');
            current.textContent = '${layout}' === 'wrong-account' ? 'someone-else' : document.querySelector('#signin-username').value;
            current.hidden = false;
          };
        })();</script>`);
      const actor = {page, loc: (id: string, options: {contains?: string} = {}) => {
        const loc = page.locator(stableElementSelector(id));
        return (options.contains ? loc.filter({hasText:options.contains}) : loc).first();
      }};
      const service = {defaultWithin: 200, scopedUser: (name: string) => name, expand: (text: string) => text,
        testId: stableElementSelector, sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 10)))};
      let status = 'passed';
      for (const step of feature.criteria[0]!.steps) {
        const result = await executeAction(ACTION_REGISTRY, step.do,
          {...step, ...(step.testid ? {within:200} : {})}, {
            capabilities: {actors:{get:()=>actor}, 'browser-interaction':service, 'browser-observation':service},
          });
        status = result.status;
        if (status !== 'passed') break;
      }
      assert.equal(status, ['direct','account-dialog'].includes(layout) ? 'passed' : 'failed', layout);
    }
  } finally { await browser.close(); }
});

test('saved views work after confirmation or closing and still reject missing content', async () => {
  const cases = [
    ['progression-support-history.json', '612c', 'support-link', 'support-ticket', 'support-owner', 'Owner ticket {user:ticketmarker}'],
    ['progression-customer-profile.json', '620c', 'profile-link', 'profile-address-summary', 'profile-owner', '14 Market Street {user:profilemarker}'],
    ['progression-notification-preferences.json', '630c', 'notification-settings', 'notification-order', 'notification-owner', 'on'],
  ];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const [file, id, opener, target, user, value] of cases) {
      const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
        'tracks/ecommerce/scenarios', file!), 'utf8'))).features[0]!;
      for (const layout of ['inline', 'closed', 'confirmation']) for (const missing of [false, true]) {
        await page.unrouteAll();
        await page.route('http://saved.test/**', route => route.fulfill({ contentType: 'text/html', body: `
          <span id="current-user">${user}</span>
          <button id="catalog-link">Catalog</button>
          <button id="${opener}" onclick="document.querySelector('#panel').hidden = !document.querySelector('#panel').hidden">Open</button>
          <section id="panel" ${layout === 'inline' ? '' : 'hidden'}>
            ${missing ? '' : `<span id="${target}" data-state="${value}">${value}</span>`}
          </section><dialog id="confirmation"><p id="support-reference">Saved reference</p>
            <button id="overlay-close" onclick="document.querySelector('dialog').close()">Close</button></dialog>` }));
        await page.goto('http://saved.test/');
        if (layout === 'confirmation') {
          await page.locator('dialog').evaluate(dialog => (dialog as HTMLDialogElement).showModal());
          await assert.rejects(page.locator(`#${opener}`).click({ timeout: 100 }), /Timeout/,
            'the confirmation dialog must block the view opener');
        }
        const actor = { page, loc: (name: string, options?: { contains?: string }) => {
          let locator = page.locator(stableElementSelector(name)).filter({ visible: true });
          if (options?.contains) locator = locator.filter({ hasText: options.contains });
          return locator.first();
        } };
        const service = { defaultWithin: 150, scopedUser: (name: string) => name,
          expand: (text: string) => text, testId: stableElementSelector,
          sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
        const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
        const statuses = [];
        for (const step of feature.criteria.find(criterion => criterion.id === id)!.steps) {
          const result = await executeAction(ACTION_REGISTRY, step.do,
            { ...step, ...(step.testid ? { within: 150 } : {}) }, { capabilities });
          statuses.push(result.status);
          if (result.status !== 'passed') break;
        }
        assert.equal(statuses.at(-1), missing ? 'failed' : 'passed', `${id}/${layout}/missing=${missing}`);
      }
    }
  } finally { await browser.close(); }
});

test('managed support live status observes a distinct change without reopening the observer', async () => {
  const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-managed-support-shared.json'), 'utf8'))).features[0]!;
  const criterion = feature.criteria.find(criterion => criterion.id === '613a')!;
  const last = criterion.steps.findIndex(step => step.do === 'expect' && step.testid === 'support-status'
    && step.contains === 'in progress');
  const steps = criterion.steps.slice(0, last + 1).filter(step => !['support-reply', 'support-reply-submit'].includes(step.testid ?? ''));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const initial of ['open', 'in progress']) for (const live of [false, true]) {
      const context = await browser.newContext();
      let savedStatus = initial;
      let ownerLoads = 0;
      const owner = await context.newPage();
      const staff = await context.newPage();
      await owner.route('http://support.test/**', route => {
        ownerLoads++;
        return route.fulfill({ contentType: 'text/html', body: `<span id="current-user">managed-shared-owner</span>
          <button id="support-link" onclick="document.querySelector('#case').hidden=false">Support</button>
          <section id="case" data-role="support-ticket" hidden>Shared managed case<span id="support-status">${savedStatus}</span></section>` });
      });
      await staff.exposeFunction('saveStatus', async (status: string) => {
        savedStatus = status;
        await staff.locator('#support-status').evaluate((element, value) => { element.textContent = value; }, status);
        if (live) await owner.locator('#support-status').evaluate((element, value) => { element.textContent = value; }, status);
      });
      await staff.setContent(`<section data-role="support-ticket">Shared managed case
        <span id="support-status">${initial}</span>
        <select id="support-status-input"><option>open</option><option>in progress</option></select>
        <button id="support-update" onclick="saveStatus(document.querySelector('select').value)">Update</button></section>`);
      await owner.goto('http://support.test/');
      const actors = new Map(([['owner', owner], ['staff', staff]] as const).map(([name, page]) => [String(name), {
        page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
          const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid))
            .filter({ hasText: options.scope.contains }).first() : page;
          let locator = root.locator(stableElementSelector(id)).filter({ visible: true });
          if (options?.contains) locator = locator.filter({ hasText: options.contains });
          return locator.first();
        },
      }]));
      const service = { defaultWithin: 200, scopedUser: (name: string) => name,
        expand: (value: string) => value, testId: stableElementSelector,
        sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
      const capabilities = { actors: { get: (name: string) => actors.get(name) },
        'browser-interaction': service, 'browser-observation': service };
      let status = '';
      for (const step of steps) {
        const result = await executeAction(ACTION_REGISTRY, step.do,
          { ...step, ...(step.testid ? { within: 200 } : {}) }, { capabilities });
        status = result.status;
        if (status !== 'passed') break;
      }
      assert.equal(status, live ? 'passed' : 'failed', `${initial}/live=${live}`);
      assert.equal(ownerLoads, 2, 'observer loads only for its baseline, never after the live mutation');
      await context.close();
    }
  } finally { await browser.close(); }
});

test('cart and recommendation probes leave blocking overlays before the next catalog action', async () => {
  const read = (file: string) => compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios', file), 'utf8'))).features[0]!.criteria[0]!.steps;
  const cases = [
    { steps: read('progression-cart-checkout.json'), item: 'Headphones', action: 'add-to-cart' },
    { steps: read('02-operational-recommendations.json').slice(0, 4), item: 'Bluetooth Speaker', action: 'buy-now' },
  ];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const item of cases) for (const overlay of [false, true]) for (const broken of [false, true]) {
      await page.setContent(`<button id="catalog-link" onclick="document.querySelector('#panel').hidden=true">Catalog</button>
        <article data-role="item-card">${item.item}<button id="${item.action}" onclick="add()">Add</button></article>
        <button id="cart-toggle" onclick="document.querySelector('#panel').hidden=!document.querySelector('#panel').hidden">Cart</button>
        <section id="panel" hidden ${overlay ? 'style="position:fixed;inset:0;background:white"' : ''}>
          ${overlay ? '<button id="overlay-close" onclick="document.querySelector(\'#panel\').hidden=true">Close</button>' : ''}
          <span id="cart-total">10</span><div data-role="cart-item">${item.item}<span id="cart-quantity">0</span></div>
        </section><span id="recommended-item" hidden>Headphones</span>
        <script>
          var count = 0;
          function add() {
            count++;
            document.querySelector('#cart-quantity').textContent = ${broken} ? '0' : String(count);
            document.querySelector('#recommended-item').hidden = ${broken};
            document.querySelector('#panel').hidden = false;
          }
        </script>`);
      const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
        const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid))
          .filter({ hasText: options.scope.contains }).first() : page;
        let locator = root.locator(stableElementSelector(id)).filter({ visible: true });
        if (options?.contains) locator = locator.filter({ hasText: options.contains });
        return locator.first();
      } };
      const service = { defaultWithin: 150, expand: (value: string) => value, testId: stableElementSelector,
        sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
      const capabilities = { actors: { get: () => actor }, 'browser-interaction': service,
        'browser-observation': service, clock: service };
      let status = '';
      for (const step of item.steps) {
        const result = await executeAction(ACTION_REGISTRY, step.do,
          { ...step, ...(step.testid ? { within: 150 } : {}) }, { capabilities });
        status = result.status;
        if (status !== 'passed') break;
      }
      assert.equal(status, broken ? 'failed' : 'passed', `${item.action}/overlay=${overlay}/broken=${broken}`);
    }
  } finally { await browser.close(); }
});

test('reference order panel stays above a wrapped header and blocks the underlying page', async () => {
  const css = readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/ecommerce/spacetime/client/src/index.css'), 'utf8');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.setContent(`<style>${css}</style>
      <header class="header" style="height:180px"><button id="underlying">Catalog</button></header>
      <div class="backdrop"></div><section class="panel"><div class="panel-header">
      <button id="close" onclick="document.body.dataset.closed='yes'">Close</button></div></section>`);
    assert.equal(await page.locator('#underlying').evaluate(element => {
      const rect = element.getBoundingClientRect();
      return document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)?.className;
    }), 'backdrop');
    await assert.rejects(page.locator('#underlying').click({ timeout: 200 }), /Timeout/);
    await page.locator('#close').click({ timeout: 1000 });
    assert.equal(await page.getAttribute('body', 'data-closed'), 'yes');
  } finally { await browser.close(); }
});

test('declared subview openers accept inline content and tabs without accepting broken views', async () => {
  const read = (name: string) => compileScenarioDefinition(JSON.parse(readFileSync(
    join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios', name), 'utf8')));
  const support = read('progression-support-triage.json').features[0]!;
  const reviews = read('01-review-visibility.json').features[0]!;
  const sales = read('02-operational-category-totals.json').features[0]!;
  const cases = [
    { opener: 'support-queue-link', target: 'support-assignee',
      step: support.setup.find(step => step.testid === 'support-queue-link')! },
    { opener: 'review-toggle', target: 'review-rating',
      step: reviews.criteria[0]!.steps.find(step => step.testid === 'review-toggle')! },
    { opener: 'sales-link', target: 'category-row',
      step: sales.criteria[0]!.steps.find(step => step.testid === 'sales-link')! },
  ];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(150);
    const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
    const service = { defaultWithin: 150, expand: (value: string) => value,
      testId: stableElementSelector, sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
    for (const item of cases) {
      assert(item.step, `missing navigation step for ${item.opener}`);
      for (const layout of ['inline', 'tab', 'broken'] as const) {
        await page.setContent(`${item.opener === 'support-queue-link' ? '<span data-role="support-ticket">Personal ticket</span>' : ''}<button id="${item.opener}" onclick="document.body.dataset.clicked='yes';
          ${layout === 'broken' ? '' : "document.querySelector('#panel').hidden = !document.querySelector('#panel').hidden"}">Open</button>
          <section id="panel" ${layout === 'inline' ? '' : 'hidden'}><span data-role="${item.target}">Expected content</span></section>`);
        const check = { do: 'expect', actor: item.step.actor, testid: item.target, contains: 'Expected content', within: 100 };
        if (layout === 'tab') assert.equal((await executeAction(ACTION_REGISTRY, 'expect', check, { capabilities })).status,
          'failed', 'the old direct-access assumption fails for a valid closed tab');
        const opened = await executeAction(ACTION_REGISTRY, 'click', item.step, { capabilities });
        assert.equal(opened.status, 'passed', opened.summary ?? undefined);
        const result = await executeAction(ACTION_REGISTRY, 'expect', check, { capabilities });
        assert.equal(result.status, layout === 'broken' ? 'failed' : 'passed', `${item.opener}: ${layout}`);
        assert.equal(await page.getAttribute('body', 'data-clicked'), layout === 'inline' ? null : 'yes');
      }
    }
    // The live totals observer must stay on the same view after the purchase.
    const live = sales.criteria.find(criterion => criterion.id === '5b')!.steps;
    const purchase = live.findIndex(step => step.testid === 'buy-now');
    assert(purchase > 0);
    assert(!live.slice(purchase + 1).some(step => step.do === 'click' || step.do === 'reload'));
  } finally { await browser.close(); }
});

test('promotions follow the staff path and delivery setup returns from persistent settings', async () => {
  const read = (name: string) => compileScenarioDefinition(JSON.parse(readFileSync(
    join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios', name), 'utf8'))).features[0]!;
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(150);
    const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
    const service = { defaultWithin: 150, expand: (value: string) => value,
      testId: stableElementSelector, sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service };
    for (const file of ['progression-promotion-checkout.json', 'progression-promotion-reporting.json']) {
      await page.setContent(`<button id="staff-link" onclick="document.querySelector('#staff').hidden=false">Staff</button>
        <section id="staff" hidden><button id="promotions-link" onclick="document.querySelector('#promotion-code').hidden=false">Promotions</button>
        <input id="promotion-code" hidden></section>`);
      const steps = read(file).setup.slice(1, 4);
      assert.deepEqual(steps.map(step => step.testid), ['staff-link', 'promotions-link', 'promotion-code']);
      const old = await executeAction(ACTION_REGISTRY, 'click', { do: 'click', actor: 'staff', testid: 'promotions-link' }, { capabilities });
      assert.equal(old.status, 'failed');
      for (const step of steps) assert.equal((await executeAction(ACTION_REGISTRY, step.do, step, { capabilities })).status, 'passed');
      assert.notEqual(await page.locator('#promotion-code').inputValue(), '');
    }
    const delivery = read('progression-delivery-notifications.json').setup;
    const saved = delivery.findIndex(step => step.testid === 'notification-save');
    assert.deepEqual(delivery.slice(saved + 1, saved + 3).map(step => step.testid), ['overlay-close', 'catalog-link']);
    for (const overlay of [false, true]) {
      await page.setContent(`<button id="catalog-link" onclick="document.querySelector('#catalog').hidden=false;document.querySelector('#settings').hidden=true">Catalog</button>
        <section id="catalog" hidden><button id="buy-now" onclick="document.body.dataset.bought='yes'">Buy</button></section>
        <section id="settings" ${overlay ? 'style="position:fixed;inset:0;background:white"' : ''}>
        ${overlay ? '<button id="overlay-close" onclick="document.querySelector(\'#settings\').hidden=true">Close</button>' : ''}</section>`);
      const buy = { do: 'click', actor: 'owner', testid: 'buy-now' };
      assert.equal((await executeAction(ACTION_REGISTRY, 'click', buy, { capabilities })).status, 'failed');
      for (const step of delivery.slice(saved + 1, saved + 3))
        assert.equal((await executeAction(ACTION_REGISTRY, 'click', step, { capabilities })).status, 'passed');
      assert.equal((await executeAction(ACTION_REGISTRY, 'click', buy, { capabilities })).status, 'passed');
      assert.equal(await page.getAttribute('body', 'data-bought'), 'yes');
    }
  } finally { await browser.close(); }
});

test('conditional navigation opens translated drawers but preserves below-fold inline content', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    for (const layout of ['closed-drawer', 'open-drawer', 'below-fold', 'clipped'] as const) {
      await page.setContent(`<style>
        #panel { ${layout === 'below-fold' ? 'margin-top:1600px' : layout === 'clipped' ? 'height:100px' : 'position:fixed;right:0;top:0;width:200px;height:200px'} }
        .closed { transform:translateX(100%) }
        .clipped { height:0;overflow:clip }
      </style><button id="orders-toggle" onclick="document.body.dataset.clicked='true';document.querySelector('#panel').classList.toggle('closed',false);document.querySelector('#wrapper').classList.remove('clipped')">Orders</button>
      <div id="wrapper" class="${layout === 'clipped' ? 'clipped' : ''}"><section id="panel" class="${layout === 'closed-drawer' ? 'closed' : ''}"><span data-role="order-item">Keyboard</span><button id="cancel-order">Cancel</button></section></div>`);
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
      assert.equal(await actor.loc('order-item').isVisible(), true, 'all layouts reproduce Playwright visibility');
      const result = await executeAction(ACTION_REGISTRY, 'click', {
        do: 'click', actor: 'customer', testid: 'orders-toggle', unlessVisible: 'order-item', within: 1000,
      }, { capabilities: { actors: { get: () => actor }, 'browser-interaction': {
        defaultWithin: 1000, expand: (value: string) => value, testId: stableElementSelector,
      } } });
      assert.equal(result.status, 'passed', result.summary ?? undefined);
      assert.equal(await page.locator('body').getAttribute('data-clicked'), ['closed-drawer', 'clipped'].includes(layout) ? 'true' : null);
      await actor.loc('cancel-order').click({ timeout: 1000 });
      if (layout === 'below-fold') assert((await page.locator('#panel').boundingBox())!.y < 600, 'inline content was scrolled into view');
    }
  } finally { await browser.close(); }
});

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

test('purchase history handles confirmation dialogs without hiding missing orders', async t => {
  const root = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios');
  const load = (file: string) => compileScenarioDefinition(JSON.parse(readFileSync(join(root, file), 'utf8')));
  // Every order-history entry point uses the same disclosed close control.
  const visit = (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) {
      for (let i = 0; i < value.length; i++) {
        const step: { do?: string; testid?: string; actor?: string } | null = value[i];
        if (step?.do === 'click' && step.testid === 'orders-toggle') {
          assert.equal(value[i - 1]?.testid, 'overlay-close');
          assert.equal(value[i - 1]?.actor, step.actor);
        }
        visit(step);
      }
    } else for (const child of Object.values(value)) visit(child);
  };
  for (const file of readdirSync(root).filter(file => file.endsWith('.json'))) visit(load(file));
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  for (const file of ['progression-purchasing.json', '01-purchase-attribution.json']) {
    const feature = load(file).features[0]!;
    const actorName = file.startsWith('progression') ? 'buyer' : 'victim';
    const steps = feature.criteria[0]!.steps.filter(step => step.actor === actorName && step.testid !== 'buy-now');
    for (const layout of ['inline', 'history-dialog', 'confirmation'] as const) for (const missing of [false, true]) {
      await page.setContent(`<button id="orders-toggle" onclick="document.querySelector('#orders').hidden=false; ${layout === 'history-dialog' ? "document.querySelector('#history').showModal()" : ''}">Orders</button>
        ${layout === 'history-dialog' ? '<dialog id="history"><button data-role="overlay-close" onclick="document.querySelector(\'#history\').close()">Close</button>' : ''}
        <section id="orders" ${layout === 'inline' ? '' : 'hidden'}>${missing ? '' : '<div data-role="order-item">Coffee Grinder<span data-role="order-total">64</span></div>'}</section>
        ${layout === 'history-dialog' ? '</dialog>' : ''}
        <dialog id="confirmation">Order confirmed<button data-role="overlay-close" onclick="document.querySelector('#confirmation').close()">Close</button></dialog>`);
      if (layout === 'confirmation') {
        await page.locator('#confirmation').evaluate(element => (element as HTMLDialogElement).showModal());
        await assert.rejects(page.locator('#orders-toggle').click({ timeout: 100 }), /Timeout/);
      }
      const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
        const scope = options?.scope ? page.locator(stableElementSelector(options.scope.testid)).filter({ hasText: options.scope.contains }) : page;
        return scope.locator(stableElementSelector(id)).filter({ hasText: options?.contains, visible: true }).first();
      } };
      const service = { defaultWithin: 150, expand: (value: string) => value, testId: stableElementSelector,
        sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
      const results = [];
      for (const step of steps) {
        const result = await executeAction(ACTION_REGISTRY, step.do, { ...step, within: 150 }, {
          capabilities: { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service },
        });
        results.push(result);
        if (result.status !== 'passed') break;
      }
      assert.equal(results.at(-1)!.status, missing ? 'failed' : 'passed', `${file}/${layout}/${missing}: ${results.at(-1)!.summary}`);
    }
  }
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

test('signup reaches direct and shared-dialog forms but rejects missing hooks and failed signup', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const layout of ['inline', 'direct', 'shared-dialog', 'missing-hook', 'rejected']) {
      const revealed = layout !== 'inline';
      await page.setContent(`
        <form id="signup" ${revealed ? 'hidden' : ''}>
          <input id="signup-username"><input id="signup-password">
          <button id="signup-submit">Sign up</button>
        </form>
        <form id="signin" hidden>
          <input id="signin-username"><input id="signin-password">
          <button id="signin-submit">Sign in</button>
        </form>
        ${revealed ? `<button id="${layout === 'missing-hook' ? 'signup-trigger' : 'signup-toggle'}" ${layout === 'shared-dialog' ? 'hidden' : ''}>Create account</button>` : ''}
        <button id="signin-toggle">Sign in</button>
        <strong id="current-user" hidden></strong>
        <script>
          window.signInClicks = 0;
          document.querySelector('#signin-toggle').onclick = () => {
            window.signInClicks++;
            document.querySelector('#signin').hidden = false;
            if ('${layout}' === 'shared-dialog') document.querySelector('#signup-toggle').hidden = false;
          };
          var reveal = document.querySelector('#signup-toggle');
          if (reveal) reveal.onclick = () => { document.querySelector('#signup').hidden = false; };
          document.querySelector('#signup').onsubmit = event => {
            event.preventDefault();
            if ('${layout}' === 'rejected') return;
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
            'browser-interaction': { defaultWithin: 300, scopedUser: (name: string) => `${name}-scope`,
              testId: (id: string) => `#${id}` },
          },
        });
      if (layout === 'missing-hook' || layout === 'rejected') {
        assert.equal(result.status, 'failed', layout);
        assert.equal(await page.locator('#current-user').isVisible(), false);
        continue;
      }
      assert.equal(result.status, 'passed', result.summary ?? JSON.stringify(result));
      assert.equal(await page.locator('#current-user').innerText(), 'Alice-scope');
      assert.equal(await page.locator('#signup-password').inputValue(), 'pw-Alice-scope');
      assert.equal(await page.evaluate(() => Reflect.get(window, 'signInClicks')), layout === 'shared-dialog' ? 1 : 0);
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


test('delayed filters and optional navigation use update deadlines without hiding broken behavior', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const actor = { page, loc: (id: string, options: { contains?: string; scope?: { testid: string; contains?: string } } = {}) => {
      let root = options.scope ? page.locator(stableElementSelector(options.scope.testid)) : page.locator('body');
      if (options.scope?.contains) root = root.filter({ hasText: options.scope.contains });
      let locator = root.locator(stableElementSelector(id));
      if (options.contains) locator = locator.filter({ hasText: options.contains });
      return locator.first();
    } };
    const service = { defaultWithin: 1000, expand: (value: string) => value,
      testId: stableElementSelector, sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
    for (const broken of [false, true]) {
      await page.setContent('<div data-role="search-results"><div data-role="item-card">Coffee Grinder</div></div>');
      if (!broken) await page.evaluate(() => { setTimeout(() => document.querySelector('[data-role="item-card"]')!.remove(), 150); });
      const result = await executeAction(ACTION_REGISTRY, 'waitUntilAbsent', { do: 'waitUntilAbsent', actor: 'visitor',
        testid: 'item-card', contains: 'Coffee Grinder', in: { testid: 'search-results' }, within: 1000 }, { capabilities });
      assert.equal(result.status, broken ? 'failed' : 'passed', result.summary ?? undefined);
      if (broken) assert.match(result.summary!, /Coffee Grinder/);
    }
    for (const inline of [false, true]) {
      await page.setContent(`<button data-role="low-stock-link" style="display:none" onclick="document.body.dataset.clicked='yes'">Stock</button>`);
      await page.evaluate(inline => { setTimeout(() => {
        if (inline) document.body.insertAdjacentHTML('beforeend', '<div data-role="low-stock-item">Air Purifier</div>');
        else (document.querySelector('button') as HTMLElement).style.display = 'block';
      }, 150); }, inline);
      const result = await executeAction(ACTION_REGISTRY, 'click', { do: 'click', actor: 'admin', testid: 'low-stock-link',
        ifAvailable: true, unlessVisible: 'low-stock-item', within: 1000 }, { capabilities });
      assert.equal(result.status, 'passed', result.summary ?? undefined);
      assert.equal(await page.getAttribute('body', 'data-clicked'), inline ? null : 'yes');
    }
  } finally { await browser.close(); }
});
