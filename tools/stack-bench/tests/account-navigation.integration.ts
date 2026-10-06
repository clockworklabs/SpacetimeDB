import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import type { ActionEvidence } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { gradeFeature } from '../grader/grade.js';
import type { Browser } from 'playwright';

test('staff and role navigation follows declared account and role entry controls', async t => {
  // Use the real scenarios. The HTTP fixture proves navigation, not database restart durability.
  let layout = '', kind = '', assignedRole = 'staff', secondRole = 'staff';
  const visits: string[] = [], accountVisitors: string[] = [], evidence: unknown[] = [];
  const roleWrites: { actor: string; role: string; status: number }[] = [];
  const server = createServer(async (request, response) => {
    const path = new URL(request.url!, 'http://fixture.test').pathname;
    visits.push(`${request.method} ${path}`);
    if (path === '/account') accountVisitors.push(new URL(request.url!, 'http://fixture.test').searchParams.get('user') ?? '');
    if (['/api/staff/1/role', '/api/staff/2/role'].includes(path) && request.method === 'PUT') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      const actor = (request.headers.authorization ?? '').replace('Bearer ', '');
      const role = JSON.parse(body).role;
      const status = actor === 'admin' ? 200 : 403;
      if ((status === 200 || layout === 'disabled-role-unsafe') && layout !== 'wrong-row') {
        if (path === '/api/staff/2/role') secondRole = role; else assignedRole = role;
      }
      roleWrites.push({ actor, role, status });
      response.writeHead(status, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(status === 200 ? { ok: true } : { error: 'Administrator required' })); return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><main></main><script>
      const layout=${JSON.stringify(layout)}, kind=${JSON.stringify(kind)}, path=${JSON.stringify(path)};
      const user=sessionStorage.getItem('user');
      window.getSessionToken=()=>user;
      const roleBoundary=layout==='disabled-role-save'||layout==='disabled-role-unsafe';
      const main=document.querySelector('main');
      if (!user) {
        main.innerHTML='<form><input id="signin-username"><input id="signin-password" type="password"><button id="signin-submit">Sign in</button></form>';
        main.querySelector('form').onsubmit=event=>{
          event.preventDefault(); sessionStorage.setItem('user',document.querySelector('#signin-username').value); location.href='/';
        };
      } else {
        const authorized=user==='staff'||user==='admin';
        const accountOpen=path==='/account';
        const behindMenu=layout==='menu'||layout==='menu-noop'||layout==='missing-target'
          ||((layout==='customer-hidden'||layout==='customer-leak')&&user==='customer');
        const expose=(!behindMenu||accountOpen)&&layout!=='missing-target';
        const allowed=authorized||layout==='customer-leak';
        main.innerHTML=(layout==='menu-noop'?'<button id="current-user">'+user+'</button>':'<a id="current-user" href="/account?user='+encodeURIComponent(user)+'">'+user+'</a>');
        if (kind==='staff') {
          const open=path==='/staff'||layout==='already-open'&&authorized;
          if (expose&&allowed&&!open) main.innerHTML+='<a id="staff-link" href="/staff">Staff area</a>';
          if (open&&allowed) main.innerHTML+='<section id="staff-area">Staff tools</section>';
        } else if (user==='admin') {
          const area=path==='/admin'||path==='/roles'||layout==='already-open';
          const tab=layout==='role-tab'||layout==='broken-tab'||layout==='missing-tab';
          const rows=area&&(!tab||path==='/roles');
          if (expose&&!area) main.innerHTML+='<a id="admin-link" href="/admin">Admin area</a>';
          if (area&&!rows&&layout!=='missing-tab') main.innerHTML+=layout==='broken-tab'
            ?'<button id="staff-roles-link">Users</button>':'<a id="staff-roles-link" href="/roles">Users</a>';
          if (rows) for (const [account, id, role] of [['staff','1',${JSON.stringify(assignedRole)}],['staff2','2',${JSON.stringify(secondRole)}]]) {
            const form=document.createElement('form');
            form.id='staff-role-account-'+account; form.dataset.role='staff-role-row';
            form.dataset.accountId=id; form.dataset.submitState='idle';
            form.innerHTML=account+'<select data-role="staff-role-select" name="role"><option>staff</option><option>inventory</option><option>admin</option></select><button data-role="staff-role-save">Save</button>';
            const select=form.querySelector('select'); select.value=role;
            form.onsubmit=async event=>{
              event.preventDefault(); form.dataset.submitState='pending';
              try {
                const result=await fetch('/api/staff/'+id+'/role',{method:'PUT',headers:{'Content-Type':'application/json',Authorization:'Bearer '+user},body:JSON.stringify({role:select.value})});
                await result.json(); form.dataset.submitState=result.ok?'succeeded':'failed';
              } catch { form.dataset.submitState='failed'; }
            };
            main.appendChild(form);
          }
        } else if (roleBoundary&&user==='staff') {
          main.innerHTML+='<section id="staff-area"><select disabled><option>staff</option></select><button data-role="staff-role-save" disabled>Save</button></section>';
        }
      }
    </script>`);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const fixture of [
      { kind: 'staff', layout: 'direct', passed: true },
      { kind: 'staff', layout: 'menu', passed: true },
      { kind: 'staff', layout: 'already-open', passed: true },
      { kind: 'staff', layout: 'menu-noop', passed: false },
      { kind: 'staff', layout: 'missing-target', passed: false },
      { kind: 'staff', layout: 'customer-hidden', passed: true },
      { kind: 'staff', layout: 'customer-leak', passed: false },
      { kind: 'roles', layout: 'direct', passed: true },
      { kind: 'roles', layout: 'menu', passed: true },
      { kind: 'roles', layout: 'already-open', passed: true },
      { kind: 'roles', layout: 'role-tab', passed: true },
      { kind: 'roles', layout: 'broken-tab', passed: false },
      { kind: 'roles', layout: 'missing-tab', passed: false },
      { kind: 'roles', layout: 'wrong-row', passed: false },
      // Disabled controls do not grant authority. A refused response must not conceal a write.
      { kind: 'roles', layout: 'disabled-role-save', passed: true },
      { kind: 'roles', layout: 'disabled-role-unsafe', passed: false },
    ]) await t.test(`${fixture.kind}: ${fixture.layout}`, async () => {
      ({ layout, kind } = fixture); assignedRole = 'staff'; secondRole = 'staff'; visits.length = 0; accountVisitors.length = 0; roleWrites.length = 0;
      const definition = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
        `tracks/ecommerce/scenarios/progression-staff-${kind === 'staff' ? 'access' : 'roles'}.json`), 'utf8'));
      const selected = definition.features[0];
      selected.criteria = selected.criteria.filter((criterion: { id: string }) =>
        (kind === 'staff' ? ['601a', '601b'] : layout.startsWith('disabled-role-') ? ['621b'] : ['621c', '621a']).includes(criterion.id));
      if (kind === 'roles') for (const criterion of selected.criteria) {
        criterion.steps = criterion.steps.filter((step: { do: string }) => step.do !== 'restartBackend');
      }
      for (const step of [...selected.setup, ...selected.criteria.flatMap((criterion: { steps: Record<string, unknown>[] }) => criterion.steps)]) {
        if ('within' in step || ['click', 'fill', 'expect'].includes(step.do)) step.within = 700;
        if ('settleMs' in step) step.settleMs = 0;
      }
      const feature = compileScenarioDefinition(definition).features[0]!;
      const grade = await gradeFeature(browser, feature,
        { url, level: definition.level, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: `staff-navigation-${kind}-${layout}`, roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 700 });
      const row = { ...fixture, grade, visits: [...visits], accountVisitors: [...accountVisitors], assignedRole, secondRole, roleWrites: [...roleWrites] };
      evidence.push(row);
      assert.equal(grade.criteria.every(criterion => criterion.evidence.status === 'passed'), fixture.passed, JSON.stringify(row));
      if (layout.startsWith('disabled-role-')) {
        assert.ok(roleWrites.some(write => write.actor === 'staff' && write.status === 403), 'reach the actual unauthorized write');
        assert.equal(secondRole, layout === 'disabled-role-unsafe' ? 'inventory' : 'staff');
      }
      if (layout === 'customer-leak') {
        assert.equal(grade.criteria.find(criterion => criterion.id === '601a')!.evidence.status, 'passed', 'prove the authorized route before testing denial');
        assert.equal(grade.criteria.find(criterion => criterion.id === '601b')!.evidence.status, 'failed');
      }
      if (fixture.passed && ['direct', 'already-open'].includes(layout)) {
        assert.equal(accountVisitors.some(user => user === 'staff' || user === 'admin'), false,
          'do not navigate an authorized actor away from a usable entry or target');
      }
      if (fixture.passed && layout === 'menu') assert.ok(visits.includes('GET /account'));
      if (fixture.passed && layout === 'role-tab') assert.ok(visits.includes('GET /roles'));
    });
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_STAFF_NAVIGATION_EVIDENCE) {
      const { writeFileSync } = await import('node:fs');
      writeFileSync(process.env.STACK_BENCH_STAFF_NAVIGATION_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_STAFF_NAVIGATION_EVIDENCE=<file> node --test --test-name-pattern="staff and role navigation" dist/tests/account-navigation.integration.js',
        evidence,
      }, null, 2));
    }
  }
});

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
    for (const layout of ['direct', 'account-dialog', 'missing', 'broken', 'wrong-account']) {
      const page = await browser.newPage();
      await page.route('http://signout.test/**', route => route.fulfill({ contentType: 'text/html', body: `
        <button id="current-user" onclick="document.querySelector('dialog').showModal()">ann</button>
        ${layout === 'direct' ? '<button id="signout">Sign out</button>' : ''}
        <dialog>${layout !== 'direct' && layout !== 'missing' ? '<button id="signout">Sign out</button>' : ''}</dialog>
        <form hidden><input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button></form>
        <script>(() => {
          if (sessionStorage.getItem('user') === null) sessionStorage.setItem('user','ann');
          const out = document.querySelector('#signout');
          const signedOut=sessionStorage.getItem('user')==='';
          document.querySelector('#current-user').hidden=signedOut;
          if(out) out.hidden=signedOut;
          document.querySelector('form').hidden=!signedOut;
          if (out) out.onclick = () => {
            if ('${layout}' === 'broken') return;
            sessionStorage.setItem('user','');
            document.querySelector('dialog').close();
            document.querySelector('#current-user').hidden = true;
            out.hidden = true; document.querySelector('form').hidden = false;
          };
          document.querySelector('form').onsubmit = e => {
            e.preventDefault();
            const current = document.querySelector('#current-user');
            current.textContent = '${layout}' === 'wrong-account' ? 'someone-else' : document.querySelector('#signin-username').value;
            sessionStorage.setItem('user',current.textContent);
            current.hidden = false;
          };
        })();</script>` }));
      await page.goto('http://signout.test/');
      const actor = {page, loc: (id: string, options: {contains?: string} = {}) => {
        const loc = page.locator(stableElementSelector(id));
        return (options.contains ? loc.filter({hasText:options.contains}) : loc).first();
      }};
      const service = {applicationUrl: 'http://signout.test/', defaultWithin: 200, scopedUser: (name: string) => name, expand: (text: string) => text,
        testId: stableElementSelector, sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 10)))};
      let status = 'passed';
      const results = [];
      for (const step of feature.criteria[0]!.steps) {
        const result = await executeAction(ACTION_REGISTRY, step.do,
          {...step, ...(step.testid ? {within:200} : {})}, {
            capabilities: {actors:{get:()=>actor}, 'browser-interaction':service, 'browser-observation':service},
          });
        results.push(result);
        status = result.status;
        if (status !== 'passed') break;
      }
      assert.equal(status, ['direct','account-dialog'].includes(layout) ? 'passed' : 'failed', JSON.stringify({ layout, results }));
      await page.close();
    }
  } finally { await browser.close(); }
});

test('saved views and purchase history work after confirmation or closing and still reject missing content', async t => {
  const root = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios');
  const load = (file: string) => compileScenarioDefinition(JSON.parse(readFileSync(join(root, file), 'utf8')));
  const orderTotal = '<span data-role="order-total">64</span>';
  const cases = [
    { file: 'progression-support-history.json', id: '612c', actor: 'owner', opener: 'support-link',
      target: 'support-ticket', user: 'support-owner', value: 'Owner ticket {user:ticketmarker}' },
    { file: 'progression-managed-support-privacy.json', id: '613b', actor: 'owner', opener: 'support-link',
      target: 'support-ticket', user: 'managed-private-owner', value: 'Private managed case {user:casemarker}', navigationOnly: true },
    { file: 'progression-managed-support-shared.json', id: '613c', actor: 'owner', opener: 'support-link',
      target: 'support-status', user: 'managed-shared-owner', value: 'in progress', ticket: 'Shared managed case', navigationOnly: true },
    { file: 'progression-customer-profile.json', id: '620c', actor: 'owner', opener: 'profile-link',
      target: 'profile-address-summary', user: 'profile-owner', value: '14 Market Street {user:profilemarker}' },
    { file: 'progression-notification-preferences.json', id: '630c', actor: 'owner', opener: 'notification-settings',
      target: 'notification-order', user: 'notification-owner', value: 'on' },
    { file: 'progression-purchasing.json', actor: 'buyer', opener: 'orders-toggle',
      target: 'order-item', user: 'buyer', value: 'Coffee Grinder', detail: orderTotal },
    { file: '01-purchase-attribution.json', actor: 'victim', opener: 'orders-toggle',
      target: 'order-item', user: 'victim', value: 'Coffee Grinder', detail: orderTotal },
    { file: '01-cart.json', id: '4b', actor: 'reload', opener: 'cart-toggle',
      target: 'cart-item', user: 'omar', value: 'Laptop Stand', detail: '<span id="cart-total">10</span>', restoredEntry: true },
  ];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const item of cases) await t.test(item.file, async () => {
      const feature = load(item.file).features[0]!;
      const criterion = item.id ? feature.criteria.find(criterion => criterion.id === item.id)! : feature.criteria[0]!;
      // The fixture starts signed in. Exercise the actual view entry after the cart session is restored.
      const selected = item.restoredEntry ? criterion.steps.slice(criterion.steps.findIndex(step => step.do === 'ensureSignedIn') + 1) : criterion.steps;
      const actorSteps = selected.filter(step => step.actor === item.actor && step.testid !== 'buy-now');
      // Managed cases prove only entry through the first content assertion here;
      // registered backend controls retain responsibility for writes and privacy.
      const steps = item.navigationOnly ? actorSteps.slice(0, actorSteps.findIndex(step => step.do === 'expect') + 1) : actorSteps;
      const support = item.opener === 'support-link';
      for (const layout of ['inline', 'inline-load-on-click', 'inline-load-on-click-menu', 'closed', 'history-dialog', 'confirmation', 'menu', 'missing-menu', 'broken-menu',
        ...(support ? ['restored-dialog'] : [])]) for (const missing of [false, true]) {
        if (layout.startsWith('inline-load-on-click') && !support) continue;
        const history = ['history-dialog', 'restored-dialog'].includes(layout);
        const menu = ['menu', 'missing-menu', 'broken-menu', 'inline-load-on-click-menu'].includes(layout);
        await page.unrouteAll();
        await page.route('http://saved.test/**', route => route.fulfill({ contentType: 'text/html', body: `
          ${layout === 'missing-menu' ? '' : `<button id="current-user" onclick="${layout === 'broken-menu' ? '' : "document.querySelector('#menu').hidden=false"}">${item.user}</button>`}
          <button id="catalog-link">Catalog</button>
          <nav id="menu" ${menu ? 'hidden' : ''}><button id="${item.opener}" onclick="${layout.startsWith('inline-load-on-click')
            ? `document.querySelectorAll('[data-role=support-ticket], #${item.target}').forEach(node => node.hidden = false)`
            : `document.querySelector('#panel').hidden = ${history ? 'false' : "!document.querySelector('#panel').hidden"}`};
            ${history ? "document.querySelector('#history').showModal()" : ''}">Open</button></nav>
          ${history ? `<dialog id="history"><button data-role="overlay-close" onclick="document.querySelector('#history').close()">Close</button>` : ''}
          <section id="panel" ${['inline', 'inline-load-on-click', 'inline-load-on-click-menu', 'restored-dialog'].includes(layout) || support && layout === 'confirmation' ? '' : 'hidden'}>
            ${support ? '<input id="support-email">' : ''}
            ${item.ticket ? `<div data-role="support-ticket" ${layout.startsWith('inline-load-on-click') ? 'hidden' : ''}>${item.ticket}` : ''}
            ${missing ? '' : `<span id="${item.target}" ${layout.startsWith('inline-load-on-click') ? 'hidden' : ''} data-state="${item.value}">${item.value}${item.detail ?? ''}</span>`}
            ${item.ticket ? '</div>' : ''}
          </section>${history ? '</dialog>' : ''}<dialog id="confirmation"><p id="support-reference">Saved reference</p>
            <button id="overlay-close" onclick="document.querySelector('#confirmation').close()">Close</button></dialog>
          ${layout === 'restored-dialog' ? '<script>document.querySelector("#history").showModal()</script>' : ''}
          ${support && layout === 'confirmation' ? '<script>document.querySelector("#confirmation").showModal()</script>' : ''}` }));
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
        assert.equal(last?.status, missing || ['missing-menu', 'broken-menu'].includes(layout) ? 'failed' : 'passed',
          `${item.file}/${layout}/missing=${missing}: ${last?.summary}`);
        if (support && missing && !['missing-menu', 'broken-menu'].includes(layout)) {
          assert.equal(last?.action.id, 'expect', `${item.file}/${layout}: missing content must reach its assertion`);
        }
      }
    });
  } finally { await browser.close(); }
});

test('support history reaches persistence and privacy checks through restored support dialogs', async t => {
  // Failure cases: restored dialogs must reach assertions; missing history, another
  // account's tickets, retained logout access, and rejected intake must still fail.
  // This HTTP fixture omits restartBackend; it does not prove database durability.
  let mode = '';
  const tickets: { user: string; subject: string }[] = [], evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    const address = new URL(request.url!, 'http://fixture.test');
    if (address.pathname === '/history') {
      const user = address.searchParams.get('user') ?? '';
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(mode === 'lost-history' ? [] : tickets.filter(ticket =>
        ticket.user === user || mode === 'private-leak' && user.includes('support-other')
          || mode === 'logout-leak' && !user))); return;
    }
    if (address.pathname === '/ticket' && request.method === 'POST') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      if (mode !== 'reject-all') tickets.push(JSON.parse(body));
      response.writeHead(mode === 'reject-all' ? 403 : 200, { 'Content-Type': 'application/json' });
      response.end('{}'); return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><main></main><script>
      const mode=${JSON.stringify(mode)}, main=document.querySelector('main');
      const user=localStorage.getItem('user')||'';
      main.innerHTML=user
        ? '<button id="current-user">'+(mode==='wrong-account'&&location.hash?'someone-else':user)+'</button><nav '+(mode==='menu'?'hidden':'')+'><button id="signout">Sign out</button><button id="support-link">Support</button></nav>'
        : '<input id="signup-username"><input id="signup-password"><button id="signup-submit">Register</button><input id="signin-username"><input id="signin-password"><button id="signin-submit">Login</button><button id="support-link">Support</button>';
      main.innerHTML+='${mode === 'confirmation' ? '<section id="support-surface" hidden>' : '<dialog><button id="overlay-close">Close</button>'}<input id="support-email"><input id="support-subject"><textarea id="support-message"></textarea><button id="support-submit">Send</button><span id="support-reference"></span><section id="history"></section>${mode === 'confirmation' ? '</section><dialog><p>Saved</p><button id="overlay-close">Close</button></dialog>' : '</dialog>'}';
      const dialog=document.querySelector('dialog');
      const refresh=async()=>{
        const rows=await (await fetch('/history?user='+encodeURIComponent(user))).json();
        document.querySelector('#history').replaceChildren(...rows.map(row=>{
          const node=document.createElement('div'); node.dataset.role='support-ticket'; node.textContent=row.subject; return node;
        }));
      };
      const open=()=>{if(mode==='confirmation')document.querySelector('#support-surface').hidden=false;else dialog.showModal();location.hash='support';void refresh();};
      document.querySelector('#support-link').onclick=open;
      document.querySelector('#overlay-close').onclick=()=>{dialog.close();history.replaceState(null,'','/');};
      document.querySelector('#support-submit').onclick=async()=>{
        const result=await fetch('/ticket',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({user,subject:document.querySelector('#support-subject').value})});
        await result.json();
        if(result.ok){document.querySelector('#support-reference').textContent='Saved';await refresh();}
      };
      if(user){
        document.querySelector('#current-user').onclick=()=>document.querySelector('nav').hidden=false;
        document.querySelector('#signout').onclick=()=>{localStorage.removeItem('user');location.reload();};
      }else for(const kind of ['signup','signin']) document.querySelector('#'+kind+'-submit').onclick=()=>{
        localStorage.setItem('user',document.querySelector('#'+kind+'-username').value);location.reload();
      };
      if(location.hash==='#support'&&mode!=='direct'&&mode!=='menu'){open();if(mode==='confirmation')dialog.showModal();}
    </script>`);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const fixture of ['direct', 'menu', 'restored-dialog', 'confirmation', 'lost-history', 'private-leak', 'logout-leak', 'wrong-account', 'reject-all']) {
      await t.test(fixture, async () => {
        mode = fixture; tickets.length = 0;
        const definition = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
          'tracks/ecommerce/scenarios/progression-support-history.json'), 'utf8'));
        const selected = definition.features[0];
        for (const criterion of selected.criteria) {
          criterion.steps = criterion.steps.filter((step: { do: string }) => step.do !== 'restartBackend');
        }
        for (const step of [...selected.setup, ...selected.criteria.flatMap((criterion: { steps: Record<string, unknown>[] }) => criterion.steps)]) {
          if ('within' in step || ['click', 'fill', 'expect', 'expectNotReceived'].includes(step.do)) step.within = 500;
          if ('settleMs' in step) step.settleMs = 0;
        }
        const grade = await gradeFeature(browser, compileScenarioDefinition(definition).features[0]!,
          { url, level: 2, headed: false, selectedCheckKeys: [], nullControl: false },
          { runId: `support-navigation-${fixture}`, roomName: name => name, url, actions: [],
            spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 500 });
        evidence.push({ fixture, grade, tickets: [...tickets] });
        assert.equal(grade.setupEvidence.status, fixture === 'reject-all' ? 'failed' : 'passed', JSON.stringify(grade));
        if (fixture === 'reject-all') {
          assert.equal((grade.setupEvidence.actions.at(-1)?.evidence as ActionEvidence).action.id, 'expect');
          assert.ok(grade.criteria.every(criterion => criterion.evidence.status === 'blocked'));
        } else if (['direct', 'menu', 'restored-dialog', 'confirmation'].includes(fixture)) {
          assert.ok(grade.criteria.every(criterion => criterion.evidence.status === 'passed'), JSON.stringify(grade));
        } else {
          const id = fixture === 'private-leak' ? '612b' : fixture === 'logout-leak' ? '612d' : '612c';
          const result = grade.criteria.find(criterion => criterion.id === id)!.evidence;
          assert.equal(result.status, 'failed', JSON.stringify(grade));
          assert.equal((result.actions.at(-1)?.evidence as ActionEvidence).action.id, fixture === 'wrong-account' ? 'ensureSignedIn' : 'expect', JSON.stringify(result));
        }
      });
    }
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_SUPPORT_NAVIGATION_EVIDENCE) {
      const { writeFileSync } = await import('node:fs');
      writeFileSync(process.env.STACK_BENCH_SUPPORT_NAVIGATION_EVIDENCE, JSON.stringify({
        rerun: 'node --test --test-name-pattern="support history reaches" dist/tests/account-navigation.integration.js',
        limitation: 'restartBackend omitted; real backend durability requires registered qualification controls', evidence,
      }, null, 2));
    }
  }
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
          <section id="case" hidden><input id="support-email"><div data-role="support-ticket">Shared managed case<span id="support-status">${savedStatus}</span></div></section>` });
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

test('checkout observations reopen the committed cart after redirects and reloads', async t => {
  // Failure cases precede the scenario fix: immediate/delayed redirect, an open
  // toggle, and an account menu behind confirmation. Wrong/missing counts fail.
  // This fixture measures browser navigation only. Existing native mutation
  // controls retain responsibility for the complete dbExpectCheckout invariant.
  const load = (file: string) => compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios', file), 'utf8'))).features[0]!;
  const duplicate = load('01-duplicate-checkout.json').criteria.find(c => c.id === '203b')!.steps;
  const crash = load('progression-checkout-crash.json').setup;
  const cases = [
    { name: 'normal', source: duplicate, before: 'control-before', layouts: ['redirect', 'delayed', 'open', 'menu-confirmation', 'wrong', 'missing'] },
    { name: 'recovered', source: duplicate, before: 'recovered-before', layouts: ['redirect'] },
    { name: 'crash-setup', source: crash, before: 'recovered-before', layouts: ['delayed', 'open'] },
    { name: 'lost-reply-reload', source: duplicate, before: null, layouts: ['menu-confirmation', 'wrong', 'missing'] },
  ];
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_CART_NAVIGATION_EVIDENCE)
      writeFileSync(process.env.STACK_BENCH_CART_NAVIGATION_EVIDENCE, JSON.stringify(evidence, null, 2) + '\n');
  });
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  for (const item of cases) for (const layout of item.layouts) await t.test(`${item.name}/${layout}`, async () => {
    const proof = item.before ? item.source.findIndex(s => s.do === 'dbExpectCheckout' && s.before === item.before) : -1;
    const start = item.before ? item.source.slice(0, proof).findLastIndex(s => s.do === 'click' && s.testid === 'checkout-submit')
      : item.source.findIndex(s => s.do === 'loseCheckoutResponse') + 1;
    const count = item.source.findIndex((s, i) => i > start && s.do === 'expectNumber' && s.testid === 'cart-count' && s.equals === 0);
    assert(start >= 0 && count > start);
    const steps = item.source.slice(start, Math.max(count, proof) + 1);
    let committed = item.before === null, loads = 0, writes = 0, toggles = 0;
    let recordCommit!: () => void, deliverReply!: () => void;
    const commit = new Promise<void>(resolve => { recordCommit = resolve; });
    const reply = new Promise<void>(resolve => { deliverReply = resolve; });
    if (committed) recordCommit();
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(250);
    await page.exposeFunction('recordToggle', () => { toggles++; });
    await page.route('http://checkout.test/**', async route => {
      if (route.request().method() === 'POST') {
        writes++; committed = true; recordCommit();
        if (layout === 'delayed') await reply;
        // Reload deliberately abandons the old document's pending reply.
        await route.fulfill({ contentType: 'application/json', body: '{}' });
        return;
      }
      loads++;
      const open = !committed || layout === 'open';
      await route.fulfill({ contentType: 'text/html', body: `<!doctype html>
        <button id="current-user" onclick="document.querySelector('nav').hidden=false">${item.name === 'crash-setup' ? 'recovered' : 'lost-reply'}</button>
        <button id="catalog-link">Catalog</button>
        <nav ${committed && layout === 'menu-confirmation' ? 'hidden' : ''}><button id="cart-toggle" onclick="recordToggle();document.querySelector('#cart').hidden=!document.querySelector('#cart').hidden">Cart</button></nav>
        <section id="cart" ${open ? '' : 'hidden'}><span id="cart-total">0</span>
          ${committed && layout === 'missing' ? '' : `<span id="cart-count">${committed && layout !== 'wrong' ? 0 : 1}</span>`}
          <button id="checkout-submit" onclick="fetch('/checkout',{method:'POST'}).then(()=>{
            if(${layout === 'open'})document.querySelector('#cart-count').textContent='0';
            else document.querySelector('#cart').hidden=true;
          })">Checkout</button></section>
        <dialog><button id="overlay-close" onclick="document.querySelector('dialog').close()">Close</button></dialog>
        ${committed && layout === 'menu-confirmation' ? '<script>document.querySelector("dialog").showModal()</script>' : ''}` });
    });
    try {
      await page.goto('http://checkout.test/');
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
      const service = { defaultWithin: 250, expand: (value: string) => value, testId: stableElementSelector,
        sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
      const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
      const actions: ActionEvidence[] = [];
      for (const step of steps) {
        if (step.do === 'dbExpectCheckout') { await commit; continue; }
        if (step.do === 'ensureSignedIn') continue; // The fixture remains signed in on every load.
        if (step.do === 'expectNumber' && layout === 'delayed') {
          // Hold the reply across any visibility guard. Its redirect races the
          // count observation, unless the scenario has replaced the old document.
          if (loads === 1) assert.equal(await page.locator('#cart-count').innerText(), '1');
          deliverReply();
        }
        const result = await executeAction(ACTION_REGISTRY, step.do,
          { ...step, ...(step.testid ? { within: 250 } : {}) }, { capabilities });
        actions.push(result);
        if (result.status !== 'passed') break;
      }
      const expected = ['wrong', 'missing'].includes(layout) ? 'failed' : 'passed';
      evidence.push({ name: item.name, layout, expected, loads, writes, toggles, actions });
      assert.equal(actions.at(-1)?.status, expected, JSON.stringify(evidence.at(-1)));
      if (layout === 'open') assert.equal(toggles, 0, 'do not close an already open cart');
      if (expected === 'failed') assert.equal(actions.at(-1)?.action.id, 'expectNumber', 'invalid counts must reach the value assertion');
      assert.equal(writes, item.before === null ? 0 : 1, 'navigation cannot issue another checkout');
    } finally { deliverReply(); await context.close(); }
  });
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

test('restock setup uses an open form and waits for delayed navigation without accepting broken forms', async () => {
  // Failure cases: closing an inline form, skipping a closed/delayed drawer,
  // accepting a broken opener or a form blocked by another modal.
  const names = ['03-deferred-access.json', '03-deferred-integrity.json',
    '03-scheduled-restock-apply.json', '03-scheduled-restock-cancel.json', '03-scheduled-restocks.json'];
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    page.setDefaultTimeout(250);
    const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
    let sleeps = 0;
    const service = { defaultWithin: 700, expand: (value: string) => value, testId: stableElementSelector,
      sleep: async (ms: number) => { sleeps++; await new Promise(resolve => setTimeout(resolve, ms)); } };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service };
    for (const name of names) {
      const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
        'tracks/ecommerce/scenarios', name), 'utf8'))).features[0]!;
      const step = feature.setup.find(step => step.testid === 'restocks-link')!;
      for (const layout of ['inline', 'delayed-inline', 'tab', 'delayed-tab', 'drawer', 'broken', 'blocked']) {
        await page.setContent(`<style>#panel.closed { position:fixed;right:0;top:0;transform:translateX(100%); }</style>
          ${layout.endsWith('inline') || layout === 'blocked' ? '' : `<button id="restocks-link" ${layout === 'delayed-tab' ? 'hidden' : ''}>Restocks</button>`}
          <section id="panel" ${['tab', 'delayed-tab', 'delayed-inline', 'broken'].includes(layout) ? 'hidden' : ''}
            class="${layout === 'drawer' ? 'closed' : ''}"><input id="schedule-restock-item">
            <button id="schedule-restock-submit" onclick="document.body.dataset.submitted='yes'">Schedule</button></section>
          ${layout === 'blocked' ? '<dialog id="blocker">Blocked</dialog>' : ''}
          <script>{
            const panel=document.querySelector('#panel'), opener=document.querySelector('#restocks-link');
            if(opener)opener.onclick=()=>{document.body.dataset.clicked='yes';
              ${layout === 'broken' ? '' : "panel.hidden=!panel.hidden;panel.classList.remove('closed');"}
              ${layout === 'drawer' ? 'panel.hidden=false;' : ''}};
            ${layout === 'delayed-inline' ? 'setTimeout(()=>panel.hidden=false,150);' : ''}
            ${layout === 'delayed-tab' ? 'setTimeout(()=>opener.hidden=false,150);' : ''}
            ${layout === 'blocked' ? 'blocker.showModal();' : ''}
          }</script>`);
        sleeps = 0;
        const opened = await executeAction(ACTION_REGISTRY, 'click', step, { capabilities });
        assert.equal(opened.status, 'passed', `${name}: ${layout}: ${opened.summary}`);
        if (layout === 'inline') assert.equal(sleeps, 0, 'an already open form needs no optional-link wait');
        const filled = await executeAction(ACTION_REGISTRY, 'fill',
          { do: 'fill', actor: 'admin', testid: 'schedule-restock-item', text: 'Webcam' }, { capabilities });
        const submitted = filled.status !== 'passed' ? filled : await executeAction(ACTION_REGISTRY, 'click',
          { do: 'click', actor: 'admin', testid: 'schedule-restock-submit', within: 250 }, { capabilities });
        const invalid = layout === 'broken' || layout === 'blocked';
        assert.equal(submitted.status, invalid ? 'failed' : 'passed', `${name}: ${layout}: ${submitted.summary}`);
        assert.equal(await page.getAttribute('body', 'data-submitted'), invalid ? null : 'yes');
        assert.equal(await page.getAttribute('body', 'data-clicked'),
          ['tab', 'delayed-tab', 'drawer', 'broken'].includes(layout) ? 'yes' : null);
      }
    }
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
    // After a reload a browser can still be animating to the restored scroll position when the
    // check runs. A link at the top of the page is there to click once the page holds still.
    await page.setContent(`<header><button id="current-user" onclick="document.body.dataset.clicked='true'">admin</button>
      <a id="admin-link" href="#admin">Admin</a></header><div style="height:4000px"></div>`);
    await page.evaluate(() => {
      let top = 0;
      const step = () => { scrollTo({ top: top += 60, behavior: 'instant' }); if (top < 1500) requestAnimationFrame(step); };
      requestAnimationFrame(step);
    });
    await page.waitForFunction(() => scrollY > 100);
    const moving = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }).first() };
    const reached = await executeAction(ACTION_REGISTRY, 'click', {
      do: 'click', actor: 'admin', testid: 'current-user', ifAvailable: true, unlessVisible: 'admin-link',
    }, { capabilities: { actors: { get: () => moving }, 'browser-interaction': {
      defaultWithin: 5000, expand: (value: string) => value, testId: stableElementSelector,
    } } });
    assert.deepEqual(reached.observation, { clicked: false, testid: 'current-user', visible: 'admin-link' });
    assert.equal(await page.locator('body').getAttribute('data-clicked'), null);
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

// Failure cases precede the signup-flow correction: registration need not create a session.
// A later login must use the same saved credentials, and must not erase a refused signup.
test('account creation accepts explicit login without masking refused or wrong-account signup', async () => {
  const browser = await chromium.launch({ headless: true });
  let mode = 'auto', signupPending = false;
  const accounts = new Map<string, { password: string; role: string }>();
  const requests: { path: string; username: string; role?: string; signupPending: boolean }[] = [];
  const evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    response.setHeader('Content-Type', 'application/json');
    if (request.method === 'POST') {
      let text = ''; for await (const chunk of request) text += String(chunk);
      const body = JSON.parse(text) as { username: string; password: string; role?: string };
      requests.push({ path: request.url!, username: body.username, role: body.role, signupPending });
      if (request.url === '/signup') {
        signupPending = true;
        if (mode === 'delayed-manual') await new Promise(resolve => setTimeout(resolve, 150));
        signupPending = false;
        if (mode === 'patched-silent-refusal') { response.writeHead(409).end('{}'); return; }
        if (mode === 'reject' || mode === 'rejected-observed' || mode === 'patched-reject' || accounts.has(body.username)) {
          response.writeHead(409).end(JSON.stringify({ error: 'Registration refused' })); return;
        }
        if (mode === 'missing') { response.end('{}'); return; }
        accounts.set(body.username, { password: body.password, role: body.role ?? 'customer' });
        response.end(JSON.stringify(mode === 'auto' || mode === 'wrong-account'
          ? { user: mode === 'wrong-account' ? 'other-account' : body.username }
          : mode === 'missing-identity-observed' ? { activeWithoutIdentity: true } : {})); return;
      }
      if (request.url === '/signin') {
        const account = accounts.get(body.username);
        if (!account || account.password !== body.password) {
          response.writeHead(401).end(JSON.stringify({ error: 'Invalid credentials' })); return;
        }
        response.end(JSON.stringify({ user: body.username })); return;
      }
      if (request.url === '/privileged') {
        const account = accounts.get(body.username);
        response.writeHead(account?.role === 'admin' ? 200 : 403).end('{}'); return;
      }
    }
    response.setHeader('Content-Type', 'text/html');
    response.end(`<form id="signup-form"><input id="signup-username"><input id="signup-password">
      <button id="signup-submit">Create account</button></form>
      <button id="signin-toggle" ${mode.startsWith('restored-') ? '' : 'hidden'}>Sign in</button>
      <form id="signin-form" ${mode.startsWith('restored-') ? 'hidden' : ''}><input id="signin-username"><input id="signin-password">
      <button id="signin-submit">Sign in</button></form>
      <strong id="current-user" hidden></strong><span id="auth-error" hidden></span>
      <script>
      // Session restoration can complete when the login panel opens after registration.
      document.querySelector('#signin-toggle').onclick = () => {
        const user = document.querySelector('#current-user');
        user.textContent = '${mode}' === 'restored-wrong-account' ? 'other-account' : document.querySelector('#signup-username').value;
        user.hidden = false;
        document.querySelector('#signin-toggle').hidden = true;
      };
      for (const action of ['signup','signin']) document.querySelector('#'+action+'-form').onsubmit = async event => {
        event.preventDefault();
        const response = await fetch('/'+action, {method:'POST', headers:{'Content-Type':'application/json'},
          body:JSON.stringify({username:document.querySelector('#'+action+'-username').value,
            password:document.querySelector('#'+action+'-password').value})});
        const data = await response.json();
        if (data.activeWithoutIdentity) { document.querySelector('#signup-form').hidden=true; document.querySelector('#signin-form').hidden=true; }
        if (data.error) { const error=document.querySelector('#auth-error'); error.textContent=data.error; error.hidden=false; }
        if (data.user) { const user=document.querySelector('#current-user'); user.textContent=data.user; user.hidden=false; }
      };
      </script>`);
  });
  try {
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
    for (mode of ['patched-silent-refusal', 'missing-identity-observed', 'auto', 'manual', 'delayed-manual', 'reject', 'missing', 'duplicate', 'wrong-account',
      'manual-observed', 'rejected-observed', 'patched-manual', 'patched-reject', 'restored-correct-account', 'restored-wrong-account']) {
      accounts.clear(); requests.length = 0; signupPending = false;
      if (mode === 'duplicate') accounts.set('Alice-scope', { password: 'fixture-password', role: 'customer' });
      const page = await browser.newPage();
      try {
        await page.goto(url);
        const actor = { page, loc: (id: string, options?: { contains?: string }) => {
          const locator = page.locator(`#${id}`);
          return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
        } };
        const interaction = {
          defaultWithin: 250, scopedUser: (name: string) => `${name}-scope`, testId: (id: string) => `#${id}`,
          expand: (value: unknown) => value,
          sleep: (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms)),
        };
        const capabilities = { actors: { get: () => actor }, 'browser-interaction': interaction,
          'browser-observation': interaction };
        const result = await executeAction(ACTION_REGISTRY, 'signUp', {
          do: 'signUp', actor: 'shopper', name: 'Alice', password: 'fixture-password',
          ...(mode === 'duplicate' ? { expectFailure: true } : {}),
          ...(mode.endsWith('-observed') ? { awaitSignedIn: false } : {}),
          ...(mode.startsWith('patched-') ? { requestPatch: { fields: { role: 'admin' } } } : {}),
        }, { capabilities });
        // expectFailure submits only; observe the actual refusal before reading server receipts.
        if (mode === 'duplicate') await page.locator('#auth-error').waitFor({ state: 'visible', timeout: 1000 });
        const loginRequests = requests.filter(r => r.path === '/signin');
        const observation = { mode, result, requests: [...requests], accountCreated: accounts.has('Alice-scope'),
          currentUser: await page.locator('#current-user').innerText(), privilegedStatus: null as number | null };
        evidence.push(observation);
        assert.equal(requests.filter(r => r.path === '/signup').length, 1, `${mode}: do not retry registration`);
        assert.equal(loginRequests.some(r => r.signupPending), false, `${mode}: visible login form is not signup completion`);
        if (mode.startsWith('restored-')) {
          assert.equal(accounts.get('Alice-scope')?.password, 'fixture-password');
          assert.equal(loginRequests.length, 0, 'a restored session must not submit another login');
          assert.equal(observation.currentUser, mode === 'restored-wrong-account' ? 'other-account' : 'Alice-scope');
          assert.equal(result.status, mode === 'restored-wrong-account' ? 'failed' : 'passed',
            `restored identity is an app observation, not a harness exception: ${JSON.stringify(result)}`);
        } else if (mode === 'patched-silent-refusal') {
          assert.equal(result.status, 'failed', 'a known rejected patch must not gain an error observation from a later login');
          assert.equal(accounts.size, 0);
          assert.equal(loginRequests.length, 0, 'never sign in after a proved HTTP registration refusal');
          assert.equal(await page.locator('#auth-error').isVisible(), false);
        } else if (mode === 'reject' || mode === 'missing') {
          assert.equal(result.status, 'failed', `${mode}: ${JSON.stringify(result)}`);
          assert.equal(accounts.size, 0);
          assert.equal(loginRequests.length, mode === 'reject' ? 0 : 1);
        } else if (mode === 'missing-identity-observed') {
          assert.equal(result.status, 'passed', JSON.stringify(result));
          assert.equal(accounts.get('Alice-scope')?.password, 'fixture-password');
          assert.equal(loginRequests.length, 0, 'no sign-in navigation when the app exposes no sign-in control');
          assert.equal(await page.locator('#signin-username').isVisible(), false);
          const identity = await executeAction(ACTION_REGISTRY, 'expect', {
            do: 'expect', actor: 'shopper', testid: 'current-user', contains: 'Alice-scope', within: 100,
          }, { capabilities });
          assert.equal(identity.status, 'failed', 'missing active identity must fail at the scored observation');
        } else if (mode === 'rejected-observed') {
          assert.equal(result.status, 'passed', JSON.stringify(result));
          assert.equal(accounts.size, 0);
          assert.equal(loginRequests.length, 0);
          const identity = await executeAction(ACTION_REGISTRY, 'expect', {
            do: 'expect', actor: 'shopper', testid: 'current-user', contains: 'Alice-scope', within: 100,
          }, { capabilities });
          assert.equal(identity.status, 'failed', 'signup refusal remains caught by the scored identity observation');
        } else if (mode === 'duplicate' || mode === 'patched-reject') {
          assert.equal(result.status, 'passed', `${mode}: ${JSON.stringify(result)}`);
          assert.equal(loginRequests.length, 0, `${mode}: refusal must not be followed by login`);
          assert.equal(await page.locator('#auth-error').isVisible(), true);
          assert.equal(await page.locator('#current-user').isVisible(), false);
          if (mode === 'patched-reject') assert.equal(accounts.size, 0);
        } else {
          assert.equal(result.status, 'passed', `${mode}: ${JSON.stringify(result)}`);
          assert.equal(accounts.get('Alice-scope')?.password, 'fixture-password');
          assert.equal(loginRequests.length, mode === 'auto' || mode === 'wrong-account' ? 0 : 1);
          const identity = await executeAction(ACTION_REGISTRY, 'expect', {
            do: 'expect', actor: 'shopper', testid: 'current-user', contains: 'Alice-scope', within: 100,
          }, { capabilities });
          assert.equal(identity.status, mode === 'wrong-account' ? 'failed' : 'passed', JSON.stringify(identity));
          if (mode === 'patched-manual') {
            assert.equal(requests.find(r => r.path === '/signup')?.role, 'admin');
            assert.equal(loginRequests[0]?.role, undefined, 'patch must be removed before ordinary login');
            assert.ok((result.observation as { requestPatch?: unknown }).requestPatch, 'retain original interception receipt');
            observation.privilegedStatus = await page.evaluate(async () => (await fetch('/privileged', {
              method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username: 'Alice-scope' }),
            })).status);
            assert.equal(observation.privilegedStatus, 200, 'a trusted signup role remains observable by the later authority probe');
          }
        }
      } finally { await page.close(); }
    }
  } finally {
    await browser.close();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_SIGNUP_EVIDENCE) {
      const { writeFileSync } = await import('node:fs');
      writeFileSync(process.env.STACK_BENCH_SIGNUP_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_SIGNUP_EVIDENCE=<file> node --test --test-name-pattern="account creation accepts explicit login" dist/tests/account-navigation.integration.js',
        evidence,
      }, null, 2));
    }
  }
});

test('mixed history reads the committed cart before preparing its next server operation', async t => {
  // Direct harness writes bypass UI callbacks. These real scenario slices must
  // refresh the document before using cart rows as input to the next operation.
  const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/mixed-operation-history.json');
  const all = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8'))).features[0]!.criteria[0]!.steps;
  const reads = all.flatMap((step, index) => step.do === 'expectNumber' && step.testid === 'cart-quantity' ? [index] : []);
  assert.equal(reads.length, 3);
  const browser = await chromium.launch({ headless: true });
  const evidence: unknown[] = [];
  try {
    for (const [position, end] of reads.entries()) for (const layout of ['correct', 'confirmation', 'restored-cart', 'wrong', 'missing', 'restored-missing']) {
      await t.test(`cart preparation ${position + 1}/${layout}`, async () => {
        const start = all.slice(0, end).findLastIndex(step => step.do === 'dbExpectOperation') + 1;
        const steps = all.slice(start, end + 1);
        const actorName = all[end]!.actor!;
        const user = actorName === 'b' ? 'history-17-b' : 'history-17-a';
        let stored = 0, loads = 0, writes = 0;
        const page = await browser.newPage();
        const actions: ActionEvidence[] = [];
        try {
          await page.route('http://history-cart.test/**', async route => {
            if (route.request().method() === 'POST') {
              writes++; stored = layout === 'wrong' ? 2 : 1;
              await route.fulfill({ contentType: 'application/json', body: '{}' });
              return;
            }
            loads++;
            const restored = stored > 0 && ['restored-cart', 'restored-missing'].includes(layout);
            const confirmation = stored > 0 && layout === 'confirmation';
            const cartTag = restored ? 'dialog' : 'section';
            await route.fulfill({ contentType: 'text/html', body: `<!doctype html>
              <span id="current-user">${user}</span>
              <button id="cart-toggle" onclick="document.querySelector('#cart').hidden=!document.querySelector('#cart').hidden">Cart</button>
              <${cartTag} id="cart" ${restored ? '' : 'hidden'}><span id="cart-total">64</span>
                ${stored && !['missing', 'restored-missing'].includes(layout) ? `<div data-role="cart-item">Coffee Grinder<input data-role="cart-quantity" value="${stored}"></div>` : '<span id="empty-cart">Empty cart</span>'}
                ${restored ? '<button id="overlay-close" onclick="document.querySelector(\'#cart\').close()">Close</button>' : ''}
              </${cartTag}>
              ${confirmation ? '<dialog id="confirmation"><button id="overlay-close" onclick="document.querySelector(\'#confirmation\').close()">Close</button></dialog>' : ''}
              ${restored || confirmation ? `<script>document.querySelector('#${restored ? 'cart' : 'confirmation'}').showModal()</script>` : ''}` });
          });
          await page.goto('http://history-cart.test/');
          // This is a real HTTP write, with no app event handler to update its cache.
          await page.evaluate(async () => { await fetch('/cart-add', { method: 'POST' }); });
          assert.equal(stored, layout === 'wrong' ? 2 : 1);
          const actor = { page, name: actorName,
            loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
              const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid))
                .filter({ hasText: options.scope.contains }).first() : page;
              const loc = root.locator(stableElementSelector(id)).filter({ visible: true });
              return (options?.contains ? loc.filter({ hasText: options.contains }) : loc).first();
            } };
          const service = { defaultWithin: 250, testId: stableElementSelector,
            scopedUser: (name: string) => name, expand: (value: string) => value,
            sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 20))) };
          const capabilities = { actors: { get: () => actor },
            'browser-interaction': service, 'browser-observation': service };
          for (const step of steps) {
            const result = await executeAction(ACTION_REGISTRY, step.do,
              { ...step, ...(step.testid ? { within: 250 } : {}) }, { capabilities });
            actions.push(result);
            if (result.status !== 'passed') break;
          }
          assert.equal(actions.at(-1)?.action.id, 'expectNumber', 'reach the original cart quantity assertion');
          assert.equal(actions.at(-1)?.status, ['wrong', 'missing', 'restored-missing'].includes(layout) ? 'failed' : 'passed');
          assert.equal(writes, 1, 'observation must not add another cart item');
        } finally {
          await page.close();
          evidence.push({ position: position + 1, layout, steps, expected: ['wrong', 'missing', 'restored-missing'].includes(layout) ? 'failed' : 'passed',
            stored, loads, writes, actions, pageClosed: page.isClosed() });
        }
      });
    }
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_HISTORY_CART_EVIDENCE) {
      const { createHash } = await import('node:crypto');
      const hash = (file: string | URL) => createHash('sha256').update(readFileSync(file)).digest('hex');
      writeFileSync(process.env.STACK_BENCH_HISTORY_CART_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_HISTORY_CART_EVIDENCE=<file> node --test --test-name-pattern="mixed history reads" dist/tests/account-navigation.integration.js',
        fixture: 'Server cart read only on document load; direct POST commits quantity 1 or 2. Closed cart, restored cart, restored confirmation, and missing row variants.',
        source, sourceSha256: hash(source), testSha256: hash(new URL(import.meta.url)),
        driverSha256: hash(new URL('../src/actions/browser-action-executors.js', import.meta.url)),
        browserClosed: !browser.isConnected(), evidence,
      }, null, 2) + '\n');
    }
  }
});

test('restored operational dialogs reach the stored value and order assertions', async t => {
  const browser = await chromium.launch({ headless: true });
  const evidence: unknown[] = [];
  try {
    for (const kind of ['admin', 'staff', 'orders'] as const) {
      const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios',
        kind === 'admin' ? '01-restock-race.json' : '02-fulfilment-ship.json');
      const feature = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
      const all = kind === 'admin' ? feature.setup! : feature.criteria[0]!.steps;
      const actorName = kind === 'orders' ? 'customer' : kind;
      const start = all.findIndex(step => step.do === 'reload' && step.actor === actorName);
      const observation = kind === 'admin' ? 'admin-stock' : kind === 'staff' ? 'queue-item' : 'order-status';
      const end = all.findIndex((step, index) => index > start && step.actor === actorName && step.testid === observation);
      assert.ok(start >= 0 && end > start, 'the real scenario must contain the reload and readback');
      const reloadSteps = all.slice(start, end + 1).filter(step => step.actor === actorName);
      const initial = feature.setup.filter(step => step.actor === actorName);
      const signedIn = initial.findIndex(step => step.do === 'signIn');
      const opened = initial.findIndex((step, index) => index > signedIn
        && step.do === 'click' && step.testid === `${kind}-link`);
      for (const phase of kind === 'orders' ? ['reload'] : ['reload', 'sign-in-result']) {
        // Sign-in may render the destination directly. Exercise its actual setup
        // entry pair, then the same real readback used by the reload case.
        const steps = phase === 'reload' ? reloadSteps
          : [...initial.slice(signedIn + 1, opened + 1), all[end]!];
        if (phase !== 'reload') assert.ok(signedIn >= 0 && opened > signedIn);
        for (const defective of [false, true]) await t.test(`${kind}/${phase}: ${defective ? 'wrong stored value' : 'correct restored dialog'}`, async () => {
          const page = await browser.newPage();
          const actions: ActionEvidence[] = [];
          try {
            // The route restores an open native dialog. Required entry links and the
            // account are still present behind it; the target surface is ready inside.
            const content = kind === 'admin'
              ? `<section id="admin-panel"><div id="admin-item-row">Bluetooth Speaker<span id="admin-stock">${defective ? 100 : 99}</span></div></section>`
              : kind === 'staff'
                ? `<section id="fulfilment-panel"><div id="queue-item">${defective ? 'Different item' : 'Keyboard'}</div></section>`
                : `<section id="order-list"><div id="order-item">Keyboard<span id="order-status">${defective ? 'pending' : 'shipped'}</span></div></section>`;
            await page.route('http://restored-tools.test/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
              <button id="current-user">${kind === 'orders' ? 'fq-ship' : kind}</button>
              <button id="admin-link">Admin</button><button id="staff-link">Staff</button>
              <button id="catalog-link">Catalog</button><button id="orders-toggle">Orders</button>
              <dialog><button id="overlay-close" onclick="document.querySelector('dialog').close()">Close</button>${content}</dialog>
              <script>document.querySelector('dialog').showModal()</script>` }));
            await page.goto('http://restored-tools.test/');
            const actor = { page, name: actorName,
              loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
                const root = options?.scope ? page.locator(stableElementSelector(options.scope.testid))
                  .filter({ hasText: options.scope.contains }).first() : page;
                const loc = root.locator(stableElementSelector(id)).filter({ visible: true });
                return (options?.contains ? loc.filter({ hasText: options.contains }) : loc).first();
              } };
            const capability = { defaultWithin: 200, testId: stableElementSelector,
              scopedUser: (name: string) => name, expand: (value: string) => value,
              recorded: new Map([['stored-before-serial-purchase', 100]]),
              sleep: async () => {} };
            const capabilities = { actors: { get: () => actor },
              'browser-interaction': capability, 'browser-observation': capability };
            let last: ActionEvidence | undefined;
            for (const [index, step] of steps.entries()) {
              last = await executeAction(ACTION_REGISTRY, step.do,
                { ...step, ...(step.testid ? { within: 200 } : {}) }, { capabilities });
              actions.push(last);
              if (index < steps.length - 1) assert.equal(last.status, 'passed',
                `${kind} must reach its readback; ${step.do}/${step.testid ?? ''}: ${last.summary}`);
              if (last.status !== 'passed') break;
            }
            assert.equal(last?.status, defective ? 'failed' : 'passed', last?.summary ?? undefined);
          } finally {
            await page.close();
            const { createHash } = await import('node:crypto');
            evidence.push({ kind, phase, defective, source, sourceSha256: createHash('sha256')
              .update(readFileSync(source)).digest('hex'), steps, expected: defective ? 'failed' : 'passed',
            actions, pageClosed: page.isClosed() });
          }
        });
      }
    }
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_OPERATIONAL_NAVIGATION_EVIDENCE) {
      const { writeFileSync } = await import('node:fs');
      const { createHash } = await import('node:crypto');
      writeFileSync(process.env.STACK_BENCH_OPERATIONAL_NAVIGATION_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_OPERATIONAL_NAVIGATION_EVIDENCE=<file> node --test --test-name-pattern="restored operational dialogs" dist/tests/account-navigation.integration.js',
        fixture: 'Native restored dialog; admin stock 99/100, fulfilment Keyboard/Different item, order shipped/pending',
        testSha256: createHash('sha256').update(readFileSync(new URL(import.meta.url))).digest('hex'),
        driverSha256: createHash('sha256').update(readFileSync(new URL('../src/actions/browser-action-executors.js', import.meta.url))).digest('hex'),
        browserClosed: !browser.isConnected(), evidence,
      }, null, 2));
    }
  }
});
