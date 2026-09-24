import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { gradeFeature } from '../grader/grade.js';
import type { Browser } from 'playwright';

test('rejected login cannot hide an app session that still permits a protected write', async () => {
  const browser = await chromium.launch({ headless: true });
  let bypass = false, writes = 0;
  const app = createServer(async (request, response) => {
    if (request.url === '/api/protected') {
      const accepted = ['Bearer valid-session', 'Bearer bypass-session'].includes(request.headers.authorization ?? '');
      if (accepted) writes++;
      response.writeHead(accepted ? 200 : 401, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ accepted })); return;
    }
    let successful = false, rejected = false;
    if (request.method === 'POST' && request.url === '/signin') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      successful = JSON.parse(body).password === 'correct-password';
      rejected = !successful;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<script>
      ${successful ? "sessionStorage.setItem('app-session', 'valid-session');" : ''}
      ${rejected && bypass ? "sessionStorage.setItem('app-session', 'bypass-session');" : ''}
      window.getSessionToken = () => sessionStorage.getItem('app-session');
      </script>
      ${successful ? '<strong id="current-user">owner</strong>' : ''}
      ${rejected ? '<span id="auth-error">Invalid username or password.</span>' : ''}
      <form id="signin-form"><input id="signin-username" name="username">
      <input id="signin-password" name="password"><input id="signin-submit" type="submit"></form>
      <script>document.querySelector('#signin-form').onsubmit = async event => {
        event.preventDefault(); const form = new FormData(event.currentTarget);
        const result = await fetch('/signin', { method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ username: form.get('username'), password: form.get('password') }) });
        document.open(); document.write(await result.text()); document.close();
      };</script>`);
  });
  try {
    await new Promise<void>(resolve => app.listen(0, '127.0.0.1', resolve));
    const appUrl = `http://127.0.0.1:${(app.address() as { port: number }).port}`;
    const call = (actor: string, authentication: string) => ({ do: 'callAction', actor, action: 'protected', authentication,
      namedAction: { id: 'protected', path: '/api/protected', reducer: 'protected', args: [], method: 'POST' }, settleMs: 0 });
    const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, name: 'login-bypass', features: [{
      id: 1, name: 'application authentication', actors: ['owner', 'impostor'], setup: [
        { do: 'signIn', actor: 'owner', name: 'owner', exact: true, password: 'correct-password' },
        call('owner', 'actor'), { do: 'expectActionOutcome', actor: 'owner', outcome: 'accepted' },
      ], criteria: [{ id: 'refused', desc: 'refused login grants no protected access', points: 0, steps: [
        { do: 'signIn', actor: 'impostor', name: 'owner', exact: true, password: 'wrong-password', expectFailure: true,
          requestPatch: { password: "' OR '1'='1" } },
        { do: 'expect', actor: 'impostor', testid: 'auth-error', within: 500 },
        { do: 'reload', actor: 'impostor', application: true, settleMs: 50 },
        { do: 'expect', actor: 'impostor', testid: 'current-user', absent: true, within: 50 },
        call('impostor', 'optional'),
        { do: 'expectActionOutcome', actor: 'impostor', outcome: 'refused', routeProvenBy: 'owner' },
      ] }],
    }] }).features[0]!;
    for (const defect of [false, true]) {
      bypass = defect; writes = 0;
      const grade = await gradeFeature(browser, feature,
        { url: appUrl, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'login-bypass', roomName: name => name, url: appUrl, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      assert.equal(grade.setupEvidence.status, 'passed', JSON.stringify(grade));
      assert.equal(grade.criteria[0]!.evidence.status, defect ? 'failed' : 'passed', JSON.stringify(grade));
      assert.equal(writes, defect ? 2 : 1, 'protected endpoint must see the actual same-page app credential');
    }
  } finally {
    await browser.close();
    await new Promise<void>((resolve, reject) => app.close(error => error ? reject(error) : resolve()));
  }
});

test('fresh ownership reads catch a server mutation hidden by the old page', async () => {
  const definition = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/02-server-actions.json'), 'utf8')));
  const steps = definition.features.find(feature => feature.id === 204)!.criteria[0]!.steps;
  const readSteps = steps.slice(steps.findLastIndex(step => step.do === 'reload'));
  assert.equal(readSteps[0]?.do, 'reload');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let serverStatus = 'pending';
    await page.route('http://ownership.test/**', route => route.fulfill({ contentType: 'text/html', body:
      `<span id="current-user">direct-owner</span><button id="orders-toggle">Orders</button>
       <div id="order-item">Keyboard <span id="order-status">${serverStatus}</span></div>` }));
    const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
      const scope = options?.scope;
      let locator = scope ? page.locator(stableElementSelector(scope.testid))
        .filter({ hasText: scope.contains }).locator(stableElementSelector(id)) : page.locator(stableElementSelector(id));
      if (options?.contains) locator = locator.filter({ hasText: options.contains });
      return locator.first();
    } };
    const service = { defaultWithin: 100, scopedUser: (name: string) => name,
      expand: (text: string) => text, testId: stableElementSelector, sleep: async () => {} };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
    for (const mutated of [false, true]) {
      serverStatus = 'pending'; await page.goto('http://ownership.test');
      assert.equal(await page.locator('#order-status').innerText(), 'pending');
      serverStatus = mutated ? 'pending' : 'cancelled';
      const outcomes = [];
      for (const step of readSteps) {
        const input = step.do === 'reload' ? { ...step, settleMs: 0 }
          : step.do === 'ensureSignedIn' ? step : { ...step, within: 100 };
        outcomes.push(await executeAction(ACTION_REGISTRY, step.do, input, { capabilities }));
      }
      assert(outcomes.slice(0, -1).every(result => result.status === 'passed'),
        JSON.stringify(outcomes.map(result => [result.action.id, result.status, result.summary])));
      assert.equal(outcomes.at(-1)!.status, mutated ? 'failed' : 'passed');
    }
  } finally { await browser.close(); }
});

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
      const target = {do:'expect',actor:'buyer',testid:'target',contains:'works'};
      const scenario = compileScenarioDefinition({schemaVersion:1,track:'ecommerce',level:1,name:'prerequisite',
        features:[{id:1,name:'probe',actors:['buyer'],setup:[{do:'expectNumber',actor:'buyer',testid:'stock',equals:quantity,within:200}],
          criteria:[{id:'target',desc:'target works',points:1,steps:[target]},
            {id:'other',desc:'another target works',points:2,steps:[target]}]}]});
      const result = await gradeFeature(routedBrowser,scenario.features[0]!,{
        url:'http://prerequisite.test',level:1,headed:false,selectedCheckKeys:[],nullControl:false,
      },{runId:'prerequisite',roomName:name=>name,url:'http://prerequisite.test',actions:[],spacetime:null,nullControl:false});
      assert.equal(result.setupEvidence.status, quantity===100?'passed':'failed', result.setupEvidence.summary ?? 'setup');
      if(quantity===99)assert.equal(result.setupEvidence.finding?.kind,'number-mismatch');
      for (const criterion of result.criteria) {
        assert.equal(criterion.evidence.status, quantity===100?'passed':'blocked');
        if(quantity===99){
          assert.equal(criterion.evidence.phase,'setup');
          assert.deepEqual(criterion.evidence.actions,[]);
        }
      }
      assert.equal(result.max,3);
      assert.equal(result.score,quantity===100?3:0);
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

test('saved views and purchase history work after confirmation or closing and still reject missing content', async () => {
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
  const orderTotal = '<span data-role="order-total">64</span>';
  const cases = [
    { file: 'progression-support-history.json', id: '612c', actor: 'owner', opener: 'support-link',
      target: 'support-ticket', user: 'support-owner', value: 'Owner ticket {user:ticketmarker}' },
    { file: 'progression-customer-profile.json', id: '620c', actor: 'owner', opener: 'profile-link',
      target: 'profile-address-summary', user: 'profile-owner', value: '14 Market Street {user:profilemarker}' },
    { file: 'progression-notification-preferences.json', id: '630c', actor: 'owner', opener: 'notification-settings',
      target: 'notification-order', user: 'notification-owner', value: 'on' },
    { file: 'progression-purchasing.json', actor: 'buyer', opener: 'orders-toggle',
      target: 'order-item', user: 'buyer', value: 'Coffee Grinder', detail: orderTotal },
    { file: '01-purchase-attribution.json', actor: 'victim', opener: 'orders-toggle',
      target: 'order-item', user: 'victim', value: 'Coffee Grinder', detail: orderTotal },
  ];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const item of cases) {
      const feature = load(item.file).features[0]!;
      const criterion = item.id ? feature.criteria.find(criterion => criterion.id === item.id)! : feature.criteria[0]!;
      const steps = criterion.steps.filter(step => step.actor === item.actor && step.testid !== 'buy-now');
      for (const layout of ['inline', 'closed', 'history-dialog', 'confirmation']) for (const missing of [false, true]) {
        const history = layout === 'history-dialog';
        await page.unrouteAll();
        await page.route('http://saved.test/**', route => route.fulfill({ contentType: 'text/html', body: `
          <span id="current-user">${item.user}</span>
          <button id="catalog-link">Catalog</button>
          <button id="${item.opener}" onclick="document.querySelector('#panel').hidden = !document.querySelector('#panel').hidden;
            ${history ? "document.querySelector('#history').showModal()" : ''}">Open</button>
          ${history ? `<dialog id="history"><button data-role="overlay-close" onclick="document.querySelector('#history').close()">Close</button>` : ''}
          <section id="panel" ${layout === 'inline' ? '' : 'hidden'}>
            ${missing ? '' : `<span id="${item.target}" data-state="${item.value}">${item.value}${item.detail ?? ''}</span>`}
          </section>${history ? '</dialog>' : ''}<dialog id="confirmation"><p id="support-reference">Saved reference</p>
            <button id="overlay-close" onclick="document.querySelector('#confirmation').close()">Close</button></dialog>` }));
        await page.goto('http://saved.test/');
        if (layout === 'confirmation') {
          await page.locator('#confirmation').evaluate(dialog => (dialog as HTMLDialogElement).showModal());
          await assert.rejects(page.locator(`#${item.opener}`).click({ timeout: 100 }), /Timeout/,
            'the confirmation dialog must block the view opener');
        }
        const actor = { page, loc: (name: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
          const scope = options?.scope
            ? page.locator(stableElementSelector(options.scope.testid)).filter({ hasText: options.scope.contains }) : page;
          let locator = scope.locator(stableElementSelector(name)).filter({ visible: true });
          if (options?.contains) locator = locator.filter({ hasText: options.contains });
          return locator.first();
        } };
        const service = { defaultWithin: 150, scopedUser: (name: string) => name,
          expand: (text: string) => text, testId: stableElementSelector,
          sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
        const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
        let last = null;
        for (const step of steps) {
          last = await executeAction(ACTION_REGISTRY, step.do,
            { ...step, ...(step.testid ? { within: 150 } : {}) }, { capabilities });
          if (last.status !== 'passed') break;
        }
        assert.equal(last?.status, missing ? 'failed' : 'passed',
          `${item.file}/${layout}/missing=${missing}: ${last?.summary}`);
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
        </section><div id="recommended-list"><span id="recommended-item" hidden>Headphones</span></div>
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

test('conditional navigation opens closed drawers and preserves inline or animated open panels', async () => {
  // Every cart, order and settings toggle up to depth 3 declares the content that shows its view is already open.
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
  const toggles = selected.flatMap(check => {
    const scenario = compileScenarioDefinition(read(check.source));
    const feature = scenario.features.find(feature => feature.id === check.feature)!;
    return [...feature.setup, ...feature.criteria.filter(criterion => !check.criteria || check.criteria.includes(criterion.id))
      .flatMap(criterion => criterion.steps)]
      .filter(step => step.do === 'click' && ['cart-toggle', 'orders-toggle', 'notification-settings'].includes(step.testid ?? ''));
  });
  assert(toggles.length > 0);
  for (const step of toggles) {
    assert.equal(typeof step.unlessVisible, 'string');
    assert(['cart-item', 'cart-total', 'checkout-submit', 'order-item', 'notification-order'].includes(step.unlessVisible as string));
  }
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    for (const layout of ['closed-drawer', 'open-drawer', 'animated-open', 'below-fold', 'clipped'] as const) {
      await page.setContent(`<style>
        #panel { ${layout === 'below-fold' ? 'margin-top:1600px' : layout === 'clipped' ? 'height:100px' : 'position:fixed;right:0;top:0;width:200px;height:200px'} }
        .closed { transform:translateX(100%) }
        .clipped { height:0;overflow:clip }
        @keyframes moving { from { transform:translateY(0) } to { transform:translateY(20px) } }
        ${layout === 'animated-open' ? '[data-role="order-item"] { display:inline-block; animation:moving .5s infinite alternate linear }' : ''}
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
    // The staff role check after reload keeps an open role panel visible and opens a closed one.
    const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-staff-roles.json');
    const roles = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!
      .criteria.find(criterion => criterion.id === '621a')!;
    const entry = roles.steps.findIndex(step => step.testid === 'admin-link');
    assert(entry >= 0);
    for (const open of [true, false]) {
      await page.setContent(`<button id="admin-link" onclick="const panel = document.querySelector('#roles'); panel.hidden = !panel.hidden">Admin</button>
        <section id="roles" ${open ? '' : 'hidden'}><div id="staff-role-account-staff">
        <select id="staff-role-select"><option>inventory</option></select></div></section>`);
      const actor = { page, loc: (id: string, options?: { scope?: { testid: string } }) => {
        const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid)) : page;
        return root.locator(stableElementSelector(id));
      } };
      const capability = { defaultWithin: 300, expand: (value: string) => value,
        testId: stableElementSelector, sleep: async () => {} };
      for (const step of roles.steps.slice(entry, entry + 2)) {
        const result = await executeAction(ACTION_REGISTRY, step.do,
          { ...step, ...(step.do === 'expect' ? { within: 300 } : {}) },
          { capabilities: { actors: { get: () => actor }, 'browser-interaction': capability,
            'browser-observation': capability } });
        assert.equal(result.status, 'passed', `621a open=${open}: ${result.summary}`);
      }
    }
  } finally { await browser.close(); }
});

test('return observation accepts a line marker only within the selected order', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const returned of [false, true]) {
      await page.setContent(`<article data-role="order-item">Keyboard
        <span data-role="order-status">shipped</span>${returned ? '<span>Returned</span>' : ''}</article>
        <article data-role="order-item">Desk Lamp <span>Returned</span></article>`);
      const actor = { page, loc: (id: string, options?: { contains?: string }) =>
        page.locator(stableElementSelector(id)).filter({ hasText: options?.contains, visible: true }).first() };
      const result = await executeAction(ACTION_REGISTRY, 'expect', {
        do: 'expect', actor: 'customer', testid: 'order-item', contains: 'Keyboard',
        containsText: 'returned', ignoreCase: true, within: 50,
      }, { capabilities: { actors: { get: () => actor }, 'browser-observation': {
        defaultWithin: 50, expand: (value: string) => value, testId: stableElementSelector,
        sleep: (ms: number) => page.waitForTimeout(ms),
      } } });
      assert.equal(result.status, returned ? 'passed' : 'failed', result.summary ?? undefined);
    }
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

test('signup and signin reach hidden, direct and shared-dialog forms but reject missing hooks and failed signup', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const [action, layout] of [['signUp', 'inline'], ['signUp', 'direct'], ['signUp', 'shared-dialog'],
      ['signUp', 'missing-hook'], ['signUp', 'rejected'], ['signIn', 'hidden'], ['signIn', 'inline']] as const) {
      const revealed = action === 'signUp' && layout !== 'inline';
      await page.setContent(`
        <form id="signup" ${revealed ? 'hidden' : ''}>
          <input id="signup-username"><input id="signup-password">
          <button id="signup-submit">Sign up</button>
        </form>
        <form id="signin" ${action === 'signIn' && layout === 'inline' ? '' : 'hidden'}>
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
          for (const form of ['signup', 'signin']) document.querySelector('#' + form).onsubmit = event => {
            event.preventDefault();
            if ('${layout}' === 'rejected') return;
            const current = document.querySelector('#current-user');
            current.textContent = document.querySelector('#' + form + '-username').value;
            current.hidden = false;
          };
        </script>`);
      const actor = { page, loc: (id: string) => page.locator(`#${id}`) };
      const result = await executeAction(ACTION_REGISTRY, action,
        { do: action, actor: 'shopper', name: 'Alice' }, {
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
      assert.equal((result.observation as { authenticationPath: string }).authenticationPath, 'local-form');
      assert.equal(await page.locator('#current-user').innerText(), 'Alice-scope');
      assert.equal(await page.locator(action === 'signUp' ? '#signup-password' : '#signin-password').inputValue(), 'pw-Alice-scope');
      assert.equal(await page.evaluate(() => Reflect.get(window, 'signInClicks')),
        layout === 'shared-dialog' || layout === 'hidden' ? 1 : 0);
    }
  } finally { await browser.close(); }
});
