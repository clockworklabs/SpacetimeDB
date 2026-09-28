import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { gradeFeature } from '../grader/grade.js';

test('catalog ranking distinguishes exact name hooks from surrounding decoration', async () => {
  // Failure cases: outside icons must pass; inside text, wrong order, missing,
  // duplicate and changed names must fail the explicit name-only interface.
  const names = ['Air Purifier', 'Bluetooth Speaker', 'Coffee Grinder', 'Desk Lamp',
    'Espresso Machine', 'Gaming Mouse', 'Headphones', 'Induction Cooktop', 'Keyboard', 'Laptop Stand'];
  const scenario = readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-catalog-ranking.json'), 'utf8');
  const feature = compileScenarioDefinition(JSON.parse(scenario)).features[0]!;
  let mode = 'plain';
  const evidence: unknown[] = [];
  const server = createServer((_request, response) => {
    const rows = [...names];
    if (mode === 'wrong-order') [rows[0], rows[1]] = [rows[1]!, rows[0]!];
    if (mode === 'missing') rows.pop();
    if (mode === 'duplicate') rows[9] = rows[0]!;
    if (mode === 'changed-name') rows[0] = 'Air Purifier Pro';
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(`<section id="item-list">${rows.map(name =>
      `<article data-role="item-card"><button><span data-role="item-name">${name}${mode === 'inside-icon' ? ' ↗' : ''}</span>${mode === 'outside-icon' ? '<span aria-hidden="true"> ↗</span>' : ''}</button></article>`
    ).join('')}</section>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (mode of ['plain', 'outside-icon', 'inside-icon', 'wrong-order', 'missing', 'duplicate', 'changed-name']) {
      const expected = mode === 'plain' || mode === 'outside-icon' ? 'passed' : 'failed';
      const grade = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'catalog-name-contract', roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 250 });
      evidence.push({ mode, expected, grade });
      assert.equal(grade.criteria[0]!.evidence.status, expected, JSON.stringify(evidence.at(-1)));
    }
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE) {
      const file = process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({
        rerun: 'STACK_BENCH_ITEM_NAVIGATION_EVIDENCE=<file> node --test --test-name-pattern="catalog ranking distinguishes" dist/tests/item-navigation.integration.js',
        browserVersion: browser.version(), names, scenario: JSON.parse(scenario), evidence,
      }, null, 2));
    }
  }
});

test('restock race accepts an open catalog and still requires catalog contents', async () => {
  const definition = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-restock-race.json'), 'utf8')));
  const feature = definition.features[0]!;
  const steps = [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)]
    .filter(step => step.do === 'click' && step.testid === 'catalog-link');
  const browser = await chromium.launch({ headless: true });
  const evidence: unknown[] = [];
  try {
    const page = await browser.newPage();
    const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)).filter({ visible: true }) };
    const service = { defaultWithin: 150, expand: (text: string) => text, testId: stableElementSelector,
      sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, ms)) };
    const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
    for (const layout of ['inline', 'closed', 'missing']) for (const step of steps) {
      await page.setContent(`${layout === 'closed' ? '<button id="catalog-link" onclick="document.querySelector(\'#item-list\').hidden=false">Catalog</button>' : ''}
        ${layout === 'missing' ? '' : `<section id="item-list" ${layout === 'closed' ? 'hidden' : ''}>Products</section>`}`);
      const clicked = await executeAction(ACTION_REGISTRY, 'click', { ...step, within: 150 }, { capabilities });
      const observed = clicked.status === 'passed' ? await executeAction(ACTION_REGISTRY, 'expect',
        { do: 'expect', actor: step.actor, testid: 'item-list', contains: 'Products', within: 150 }, { capabilities }) : clicked;
      evidence.push({ layout, step, clicked, observed });
      assert.equal(observed.status, layout === 'missing' ? 'failed' : 'passed', JSON.stringify(evidence.at(-1)));
    }
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE) writeFileSync(process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE,
      JSON.stringify({ rerun: 'node --test --test-name-pattern="restock race accepts" dist/tests/item-navigation.integration.js', evidence }, null, 2));
  }
});

test('catalog management reaches a declared inner tab and verifies the created product', async t => {
  // Failure first: a valid inner tab must work; broken navigation and missing fields must not pass.
  let layout = '', products: Array<{ name: string; variants: string[] }> = [];
  const visits: string[] = [], evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    const path = new URL(request.url!, 'http://fixture.test').pathname;
    visits.push(`${request.method} ${path}`);
    if (path === '/save') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      const form = new URLSearchParams(body);
      products.push({ name: form.get('name')!, variants: form.get('variants')!.split(',').map(v => v.trim()) });
      response.writeHead(303, { Location: '/admin/catalog' }); response.end(); return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><main></main><script>
      const layout=${JSON.stringify(layout)}, path=${JSON.stringify(path)}, products=${JSON.stringify(products)};
      const user=sessionStorage.getItem('user'), main=document.querySelector('main');
      main.innerHTML='<input id="search-input"><section id="item-list">'+products.map(p=>'<article data-role="item-card"><span data-role="item-name">'+p.name+'</span>'+p.variants.map(v=>'<span data-role="item-variant">'+v+'</span>').join('')+'</article>').join('')+'</section>';
      if (!user) {
        main.innerHTML+='<form id="login"><input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button></form>';
        document.querySelector('#login').onsubmit=e=>{e.preventDefault();sessionStorage.setItem('user',document.querySelector('#signin-username').value);location.href='/';};
      } else {
        main.innerHTML+='<a id="current-user" href="/account">'+user+'</a>';
        const area=path.startsWith('/admin')||layout==='already-open';
        if (!area&&(layout!=='menu-tab'||path==='/account')) main.innerHTML+='<a id="admin-link" href="/admin">Admin</a>';
        const tab=['menu-tab','broken-tab','missing-tab'].includes(layout);
        if (area&&tab&&path!=='/admin/catalog'&&layout!=='missing-tab') main.innerHTML+=layout==='broken-tab'
          ?'<button id="catalog-management-link">Products</button>':'<a id="catalog-management-link" href="/admin/catalog">Products</a>';
        if (area&&(!tab||path==='/admin/catalog')) main.innerHTML+='<form method="post" action="/save"><input id="catalog-name" name="name"><input id="catalog-category" name="category">'+(layout==='missing-field'?'':'<input id="catalog-price" name="price">')+'<input id="catalog-variants" name="variants"><button id="catalog-save">Save</button></form>';
      }
    </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (layout of ['direct', 'menu-tab', 'already-open', 'broken-tab', 'missing-tab', 'missing-field']) {
      await t.test(layout, async () => {
        products = []; visits.length = 0;
        const definition = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
          'tracks/ecommerce/scenarios/progression-catalog-management.json'), 'utf8'));
        const selected = definition.features[0];
        for (const step of [...selected.setup, ...selected.criteria.flatMap((c: { steps: Record<string, unknown>[] }) => c.steps)]) {
          if ('within' in step || ['click', 'fill', 'expect'].includes(step.do)) step.within = 700;
          if ('settleMs' in step) step.settleMs = 0;
        }
        const grade = await gradeFeature(browser, compileScenarioDefinition(definition).features[0]!,
          { url, level: definition.level, headed: false, selectedCheckKeys: [], nullControl: false },
          { runId: `catalog-management-${layout}`, roomName: name => name, url, actions: [],
            spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 700 });
        const passed = ['direct', 'menu-tab', 'already-open'].includes(layout);
        const row = { layout, grade, visits: [...visits], products: [...products] }; evidence.push(row);
        assert.equal(grade.criteria.every(c => c.evidence.status === 'passed'), passed, JSON.stringify(row));
        assert.equal(products.length, passed ? 1 : 0, 'navigation must not add or repeat product writes');
        if (passed) assert.deepEqual(products[0], { name: 'Travel Mug', variants: ['Black', 'Silver'] });
        if (layout === 'menu-tab') assert.ok(visits.includes('GET /account') && visits.includes('GET /admin/catalog'));
        if (['direct', 'already-open'].includes(layout)) assert.equal(visits.includes('GET /account'), false);
      });
    }
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_CATALOG_MANAGEMENT_EVIDENCE) {
      writeFileSync(process.env.STACK_BENCH_CATALOG_MANAGEMENT_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_CATALOG_MANAGEMENT_EVIDENCE=<file> node --test --test-name-pattern="catalog management reaches" dist/tests/item-navigation.integration.js', evidence,
      }, null, 2));
    }
  }
});

test('catalog search reaches its controls when the home page also has an item list', async () => {
  let layout = '', routes: string[] = [];
  const evidence: unknown[] = [];
  const server = createServer((request, response) => {
    routes.push(request.url ?? '');
    const searchable = layout === 'direct' || request.url === '/catalog';
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html>
      ${layout === 'separate' && !searchable ? '<a id="catalog-link" href="/catalog">Browse catalog</a>' : ''}
      <section id="item-list"><article data-role="item-card">Desk Lamp</article></section>
      ${searchable ? `<input id="search-input"><section id="search-results"></section>
      <script>document.querySelector('input').oninput = event => {
        document.querySelector('#search-results').innerHTML = 'mirrorless camera'.includes(event.target.value.toLowerCase())
          ? '<article data-role="item-card">Mirrorless Camera</article>' : '';
      };</script>` : ''}`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  let result = 'failed';
  try {
    const definition = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
      'tracks/ecommerce/scenarios/01-catalog-search.json'), 'utf8'));
    const feature = compileScenarioDefinition(definition).features[0]!;
    for (layout of ['separate', 'direct', 'missing-link']) {
      routes = [];
      const grade = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'catalog-search-navigation', roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      evidence.push({ layout, grade, routes: [...routes], setup: feature.setup, criteria: feature.criteria });
      assert.equal(grade.criteria[0]!.evidence.status, layout === 'missing-link' ? 'failed' : 'passed',
        JSON.stringify({ layout, grade, routes }));
      assert.equal(routes.includes('/catalog'), layout === 'separate', JSON.stringify({ layout, routes }));
    }
    result = 'passed';
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE) {
      const file = process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({ result,
        rerun: 'node --test dist/tests/item-navigation.integration.js', evidence }, null, 2));
    }
  }
});

test('filter setup supports automatic updates and an optional Apply control', async () => {
  const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-faceted-filters.json'), 'utf8'));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const explicitApply of [false, true]) {
      // The setup zeroes stock directly, then reloads before filtering.
      await page.route('http://filters.test/', route => route.fulfill({ contentType: 'text/html', body: `
        <select id="category-filter"><option>All</option><option>Home</option></select>
        <input id="minimum-price"><input id="maximum-price">
        ${explicitApply ? '<button id="filter-apply">Apply</button>' : ''}
        <output id="search-results">Unfiltered</output>
        <script>
          function apply() {
            document.querySelector('output').textContent = [
              document.querySelector('select').value,
              document.querySelector('#minimum-price').value,
              document.querySelector('#maximum-price').value,
            ].join('|');
          }
          if (${explicitApply}) document.querySelector('button').onclick = apply;
          else for (const event of ['input', 'change']) document.body.addEventListener(event, apply);
        </script>` }));
      await page.goto('http://filters.test/');
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)) };
      const writes: unknown[] = [];
      const sleep = async () => {};
      for (const step of scenario.features[0].setup) {
        const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: {
          actors: { get: () => actor },
          'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
            testId: stableElementSelector, sleep },
          clock: { sleep },
          'database-write': { setStock: async (input: unknown) => { writes.push(input); return input; } },
        } });
        assert.equal(result.status, 'passed', result.summary ?? undefined);
      }
      assert.equal(writes.length, 2);
      assert.equal(await page.locator('#search-results').innerText(), 'Home|50|200');
      await page.unroute('http://filters.test/');
    }
  } finally { await browser.close(); }
});

test('openItem uses the declared name hook on links, buttons, and clickable cards', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const control of ['button', 'a', 'card']) {
      await page.setContent(`
        <article data-role="item-card">Other item<button>View</button></article>
        <article data-role="item-card" id="selected-card">
          <${control === 'card' ? 'h3' : control} data-role="item-name">Desk Lamp</${control === 'card' ? 'h3' : control}>
          <button id="buy">Buy now</button>
        </article>
        <section id="item-detail" hidden></section>
        <script>
          document.querySelector('#buy').onclick = () => { throw Error('must not buy'); };
          document.querySelector(${JSON.stringify(control === 'card'
            ? '#selected-card' : '[data-role="item-name"]')}).onclick = event => {
              event.preventDefault();
              const detail = document.querySelector('#item-detail');
              detail.textContent = 'Desk Lamp details'; detail.hidden = false;
            };
        </script>`);
      const actor = { page, loc: (id: string, options?: { contains?: string }) => {
        const locator = page.locator(stableElementSelector(id));
        return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
      } };
      const result = await executeAction(ACTION_REGISTRY, 'openItem',
        { do: 'openItem', actor: 'visitor', item: 'Desk Lamp' }, { capabilities: {
          actors: { get: () => actor },
          'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
            testId: stableElementSelector },
        } });
      assert.equal(result.status, 'passed', result.summary ?? JSON.stringify(result));
      assert.equal(await page.locator('#item-detail').innerText(), 'Desk Lamp details');
    }
  } finally { await browser.close(); }
});

test('openItem preserves inline variants and otherwise requires successful detail navigation', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const placement of ['inline', 'detail', 'broken']) {
      await page.setContent(`
        <article data-role="item-card">Other item<span data-role="item-variant">Other variant</span></article>
        <article data-role="item-card">
          <button data-role="item-name">Travel Mug</button>
          <span data-role="item-variant" ${placement === 'inline' ? '' : 'hidden'}>Black</span>
        </article>
        <section id="item-detail" hidden><span data-role="item-variant">Black</span></section>
        <script>
          document.querySelector('[data-role="item-name"]').onclick = () => {
            document.body.dataset.clicked = 'true';
            if (${JSON.stringify(placement)} === 'detail') document.querySelector('#item-detail').hidden = false;
          };
        </script>`);
      const actor = { page, loc: (id: string, options?: { contains?: string }) => {
        const locator = page.locator(stableElementSelector(id));
        return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
      } };
      const result = await executeAction(ACTION_REGISTRY, 'openItem', {
        do: 'openItem', actor: 'visitor', item: 'Travel Mug', unlessVisible: 'item-variant', within: 1000,
      }, { capabilities: {
        actors: { get: () => actor },
        'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
          testId: stableElementSelector },
      } });
      assert.equal(result.status, placement === 'broken' ? 'failed' : 'passed', result.summary ?? undefined);
      assert.equal(await page.locator('body').getAttribute('data-clicked'), placement === 'inline' ? null : 'true');
      if (placement === 'detail') assert.equal(await page.locator('#item-detail').isVisible(), true);
    }
  } finally { await browser.close(); }
});
