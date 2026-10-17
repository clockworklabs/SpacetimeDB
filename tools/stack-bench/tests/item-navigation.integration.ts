import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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

test('warehouse inventory accepts consistent views and rejects missing or conflicting stock', async t => {
  // Failure first: exercise the real 7b criterion against complete and defective
  // projections. Authentication and 7a are outside this already-open admin view.
  const scenarioPath = 'tracks/ecommerce/scenarios/01-warehouse-admin-staff.json';
  const fixturePath = 'tracks/ecommerce/composition/fixtures/operations.json';
  const scenario = readFileSync(join(STACK_BENCH_ROOT, scenarioPath), 'utf8');
  const fixtureSource = readFileSync(join(STACK_BENCH_ROOT, fixturePath), 'utf8');
  const fixture = JSON.parse(fixtureSource) as {
    warehouses: string[]; items: Array<{ name: string; stock: Record<string, number> }>;
  };
  const compiled = compileScenarioDefinition(JSON.parse(scenario)).features[0]!;
  const feature = { ...compiled, actors: ['admin'], setup: [],
    criteria: compiled.criteria.filter(criterion => criterion.id === '7b').map(criterion => ({
      ...criterion, steps: criterion.steps.map(step => ({ ...step, within: 250 })),
    })) };
  const cases = [
    { mode: 'single', expected: 'passed' },
    { mode: 'reordered-labelled', expected: 'passed' },
    { mode: 'split-inline-name', expected: 'passed' },
    { mode: 'block-separated-name', expected: 'passed' },
    { mode: 'duplicate-items', expected: 'passed' },
    { mode: 'duplicate-locations', expected: 'passed' },
    { mode: 'nested-warehouse-label', expected: 'passed' },
    { mode: 'nested-holding-label', expected: 'passed' },
    { mode: 'nested-item-label', expected: 'passed' },
    { mode: 'nested-label-quantity', expected: 'passed' },
    { mode: 'nested-label-wrong-quantity', expected: 'failed' },
    { mode: 'nested-label-wrong-warehouse', expected: 'failed' },
    { mode: 'nested-holding-number-only', expected: 'passed' },
    { mode: 'missing-item-equal-count', expected: 'failed' },
    { mode: 'wrong-non-mouse-total', expected: 'failed' },
    { mode: 'wrong-item-copy-first', expected: 'failed' },
    { mode: 'wrong-item-copy-last', expected: 'failed' },
    { mode: 'missing-location-equal-count', expected: 'failed' },
    { mode: 'wrong-location', expected: 'failed' },
    { mode: 'swapped-holdings', expected: 'failed' },
    { mode: 'wrong-location-copy', expected: 'failed' },
    { mode: 'missing-warehouse', expected: 'failed' },
    { mode: 'missing-number', expected: 'failed' },
    { mode: 'missing-copy-number', expected: 'failed' },
    { mode: 'malformed-number', expected: 'failed' },
    { mode: 'hidden-item', expected: 'failed' },
    { mode: 'empty', expected: 'failed' },
    { mode: 'outside-panel', expected: 'failed' },
    { mode: 'hidden-panel', expected: 'failed' },
    { mode: 'unknown-item', expected: 'failed' },
    { mode: 'substring-item', expected: 'failed' },
    { mode: 'unknown-warehouse', expected: 'failed' },
    { mode: 'nested-number-only', expected: 'passed' },
    { mode: 'item-wrapper-empty', expected: 'passed' },
    { mode: 'item-wrapper-copies', expected: 'passed' },
    { mode: 'item-wrapper-labelled', expected: 'passed' },
    { mode: 'item-wrapper-wrong-name', expected: 'failed' },
    { mode: 'item-wrapper-wrong-number', expected: 'failed' },
    { mode: 'hidden-number', expected: 'failed' },
    { mode: 'late-snapshot', expected: 'inconclusive' },
  ];
  let mode = '';
  const evidence: unknown[] = [];
  const server = createServer((_request, response) => {
    const items: Array<{ name: string; total?: number | string; hidden?: boolean }> = fixture.items.map(item => ({
      name: item.name, total: Object.values(item.stock).reduce((sum, value) => sum + value, 0),
    }));
    const locations = fixture.items.flatMap(item => fixture.warehouses.map(warehouse => ({
      name: item.name, warehouse, quantity: item.stock[warehouse]!,
    })));
    let warehouses = [...fixture.warehouses];
    if (mode === 'reordered-labelled') { items.reverse(); locations.reverse(); warehouses.reverse(); }
    if (mode === 'duplicate-items') items.push(...items.map(item => ({ ...item })));
    if (mode === 'duplicate-locations') locations.push(...locations.map(location => ({ ...location })));
    if (mode === 'missing-item-equal-count') { items.pop(); items.push({ ...items[0]! }); }
    if (mode === 'wrong-non-mouse-total') items[0]!.total = Number(items[0]!.total) + 1;
    if (mode === 'wrong-item-copy-first') items.unshift({ ...items[0]!, total: Number(items[0]!.total) + 1 });
    if (mode === 'wrong-item-copy-last') items.push({ ...items[0]!, total: Number(items[0]!.total) + 1 });
    if (mode === 'missing-location-equal-count') { locations.pop(); locations.push({ ...locations[0]! }); }
    if (mode === 'wrong-location') locations[0]!.quantity++;
    if (mode === 'swapped-holdings') {
      [locations[0]!.quantity, locations[1]!.quantity] = [locations[1]!.quantity, locations[0]!.quantity];
    }
    if (mode === 'wrong-location-copy') locations.push({ ...locations[0]!, quantity: locations[0]!.quantity + 1 });
    if (mode === 'missing-warehouse') warehouses = warehouses.filter(warehouse => warehouse !== 'West');
    if (mode === 'missing-number') delete items[0]!.total;
    if (mode === 'missing-copy-number') items.push({ name: items[0]!.name });
    if (mode === 'malformed-number') items[0]!.total = 'unknown';
    if (mode === 'hidden-item') items.at(-1)!.hidden = true;
    if (mode === 'empty') { items.length = 0; locations.length = 0; warehouses = []; }
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    let html = `<!doctype html><main><span id="current-user">admin</span><section id="admin-panel">
      <h1>Warehouse inventory</h1><section aria-label="Items">${items.map(item =>
        `<article data-role="admin-item-row" ${item.hidden ? 'hidden' : ''}><strong>${item.name}</strong>
        ${item.total === undefined ? '' : `<span data-role="admin-stock">${item.total}</span>`}
        ${mode === 'reordered-labelled' ? '<small> units available · Inventory summary</small>' : ''}</article>`).join('')}</section>
      ${warehouses.map(warehouse => `<section data-role="admin-warehouse-item"><h2>${warehouse}</h2>
        ${locations.filter(location => location.warehouse === warehouse).map(location =>
          `<article data-role="admin-location-row">${mode === 'reordered-labelled'
            ? `${location.warehouse} warehouse · ${location.name}` : `${location.name} · ${location.warehouse}`}
          <span data-role="admin-location-qty">${location.quantity}</span></article>`).join('')}</section>`).join('')}
      </section></main>`;
    if (mode === 'outside-panel') html = html.replace('<section id="admin-panel">', '<section id="admin-panel"></section><section>');
    if (mode === 'split-inline-name') html = html.replaceAll('Headphones', '<span>Head</span>phones');
    if (mode === 'block-separated-name') html = html.replace('<strong>Air Purifier</strong>', '<div>Air</div><div>Purifier</div>');
    if (mode === 'hidden-panel') html = html.replace('<section id="admin-panel">', '<section id="admin-panel" hidden>');
    if (mode === 'unknown-item') html = html.replace('<h1>', '<article data-role="admin-item-row">Unknown item<span data-role="admin-stock">100</span></article><h1>');
    if (mode === 'substring-item') html = html.replaceAll('Air Purifier', 'Air PurifierPlus');
    if (mode === 'unknown-warehouse') html = html.replace('<h1>', '<section data-role="admin-warehouse-item">North</section><h1>');
    if (mode === 'nested-number-only') html = html.replace('<span data-role="admin-stock">100</span>',
      '<article data-role="admin-item-row">Air Purifier<span data-role="admin-stock">100</span></article>');
    if (mode.startsWith('item-wrapper-')) html = html.replace(
      /<article data-role="admin-item-row" >(<strong>Air Purifier<\/strong>\s*<span data-role="admin-stock">100<\/span>\s*)<\/article>/,
      `<article data-role="admin-item-row">${mode === 'item-wrapper-wrong-name' ? 'Keyboard' : mode === 'item-wrapper-labelled' ? 'Air Purifier' : ''}${mode === 'item-wrapper-wrong-number' ? 'Air Purifier<span data-role="admin-stock">101</span>' : ''}<article data-role="admin-item-row">$1${mode === 'item-wrapper-labelled' ? ' units available' : ''}</article>${mode === 'item-wrapper-copies' ? '<article data-role="admin-item-row">$1</article>' : ''}</article>`);
    if (mode === 'hidden-number') html = html.replace('<span data-role="admin-stock">100</span>', '<span data-role="admin-stock" hidden>100</span>');
    if (mode === 'nested-item-label') html = html.replace('<strong>Air Purifier</strong>', '<span data-role="admin-warehouse-item">Air Purifier · East</span>');
    if (mode.startsWith('nested-') && mode !== 'nested-number-only') {
      html = html.replace(/(<article data-role="admin-location-row">)([^<]+) · (East|West)(\s*)(<span data-role="admin-location-qty">\d+<\/span>)/g,
        (_match, opening, item, warehouse, spacing, quantity) => {
          if (mode === 'nested-holding-number-only') return `${opening}<article data-role="admin-location-row">${item} · ${warehouse}${quantity}</article>`;
          const label = mode === 'nested-holding-label' ? `${item} · ${warehouse}` : warehouse;
          const prefix = mode === 'nested-holding-label' ? '' : `${item} · `;
          return `${opening}${prefix}<span data-role="admin-warehouse-item">${label}${mode === 'nested-label-quantity' ? ` ${quantity}` : ''}</span>${spacing}${mode === 'nested-label-quantity' ? '' : quantity}`;
        });
      if (mode === 'nested-label-wrong-quantity') html = html.replace('<span data-role="admin-location-qty">60</span>', '<span data-role="admin-location-qty">61</span>');
      if (mode === 'nested-label-wrong-warehouse') html = html.replace('<span data-role="admin-warehouse-item">East</span>', '<span data-role="admin-warehouse-item">West</span>');
    }
    if (mode === 'late-snapshot') html += `<script>
      const control = document.querySelector('[data-role="admin-stock"]');
      const measure = control.getBoundingClientRect.bind(control);
      control.getBoundingClientRect = () => {
        const end = performance.now() + 350; while (performance.now() < end) {}
        control.getBoundingClientRect = measure; return measure();
      };
    </script>`;
    response.end(html);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const entry of cases) await t.test(entry.mode, async () => {
      mode = entry.mode;
      const grade = await gradeFeature(browser, feature,
        { url, level: 2, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: `warehouse-view-${mode}`, roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 250 });
      const row = { ...entry, grade }; evidence.push(row);
      assert.equal(grade.setupEvidence?.status, 'passed', JSON.stringify(row));
      assert.equal(grade.criteria[0]!.evidence.status, entry.expected, JSON.stringify(row));
      if (entry.expected === 'failed') {
        assert.equal(grade.criteria[0]!.evidence.code, 'application_failure', JSON.stringify(row));
      }
      if (entry.expected === 'inconclusive') {
        assert.equal(grade.criteria[0]!.evidence.finding?.kind, 'observation-window-missed', JSON.stringify(row));
      }
    });
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE) {
      const file = process.env.STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      const files = [scenarioPath, fixturePath, 'dist/tests/item-navigation.integration.js',
        'dist/src/actions/browser-action-executors.js', 'dist/src/composition/definition-compiler.js'];
      writeFileSync(file, JSON.stringify({
        rerun: 'STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE=<file> node --test --test-name-pattern="warehouse inventory accepts" dist/tests/item-navigation.integration.js',
        browserVersion: browser.version(), fixture, scenario: JSON.parse(scenario),
        deadlinePolicy: 'A first read that overruns is inconclusive. Expiry preserves the last timely mismatch, even if a later snapshot matches; late evidence never establishes a pass.',
        sourceSha256: Object.fromEntries(files.map(path => [path,
          createHash('sha256').update(readFileSync(join(STACK_BENCH_ROOT, path))).digest('hex')])),
        cases, evidence,
      }, null, 2));
    }
  }
});

test('warehouse totals validate every visible copy against one recorded baseline', async t => {
  const scenarioPath = 'tracks/ecommerce/scenarios/02-strengthened.json';
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT, scenarioPath), 'utf8')));
  const evidence: unknown[] = [];
  const browser = await chromium.launch({ headless: true });
  const cases = [
    ['2a', 'single', 'passed'], ['2a', 'copies', 'passed'], ['2a', 'hidden-copy', 'passed'],
    ['2a', 'outside-copy', 'passed'], ['2a', 'missing-total', 'failed'],
    ['2a', 'bad-baseline', 'failed'], ['2a', 'nonnumeric', 'failed'], ['2a', 'nested-borrow', 'passed'],
    ['2a', 'nested-empty', 'passed'], ['2a', 'nested-copies', 'passed'],
    ['2a', 'nested-labelled', 'passed'],
    ['2a', 'nested-wrong-name', 'failed'], ['2a', 'nested-wrong-number', 'failed'],
    ['2a', 'wrong-copy', 'failed'], ['2a', 'reverse', 'failed'], ['2a', 'refused', 'failed'],
    ['2a', 'missing-warehouse', 'failed'], ['2a', 'wrong-identity', 'failed'],
    ['201a', 'copies', 'passed'], ['201a', 'unauthorized-effect', 'failed'],
    ['2a', 'late-record', 'inconclusive'], ['2a', 'late-expect', 'inconclusive'],
    ['2a', 'late-correction', 'failed'],
  ];
  try {
    for (const [criterion, mode, expected] of cases) await t.test(`${criterion}/${mode}`, async () => {
      const source = scenario.features.flatMap(feature => feature.criteria).find(item => item.id === criterion)!.steps;
      const steps = source.filter(step => step.testid === 'warehouse-total'
        || step.do === 'expect' && step.testid === 'admin-warehouse-item');
      const boundary = steps.findIndex(step => step.do === 'expectNumber');
      const page = await browser.newPage();
      const recorded = new Map<string, number>();
      const actions: unknown[] = [];
      let result: Awaited<ReturnType<typeof executeAction>> | undefined;
      const row = (name: string, value: number | string | null, extra = '') => `<section data-role="admin-warehouse-item" ${extra}>${name}
        ${value === null ? '' : `<span data-role="warehouse-total" data-warehouse="${name}">${value}</span>`}</section>`;
      let extra = '';
      if (['copies', 'wrong-copy', 'unauthorized-effect'].includes(mode!)) extra = row('East', 600) + row('West', 400);
      if (mode === 'missing-total') extra = row('East', null);
      if (mode === 'bad-baseline' || mode === 'late-correction') extra = row('East', 601);
      if (mode === 'nonnumeric') extra = row('East', 'unknown');
      if (mode === 'hidden-copy') extra = row('East', 999, 'hidden');
      if (mode === 'nested-borrow') extra = `<section data-role="admin-warehouse-item">East ${row('East', 600)}</section>`;
      if (mode === 'nested-empty') extra = `<section data-role="admin-warehouse-item">${row('East', 600)}</section>`;
      if (mode === 'nested-copies') extra = `<section data-role="admin-warehouse-item">East ${row('East', 600)}${row('East', 600)}</section>`;
      if (mode === 'nested-labelled') extra = `<section data-role="admin-warehouse-item">East ${row('East warehouse', 600).replace('data-warehouse="East warehouse"', 'data-warehouse="East"')}</section>`;
      if (mode === 'nested-wrong-name') extra = `<section data-role="admin-warehouse-item">East ${row('West', 400)}</section>`;
      if (mode === 'nested-wrong-number') extra = `<section data-role="admin-warehouse-item">East <span data-role="warehouse-total">601</span>${row('East', 600)}</section>`;
      try {
        await page.setContent(`<section id="admin-panel">
          ${row(mode === 'wrong-identity' ? 'Eastside' : 'East', 600)}
          ${mode === 'missing-warehouse' ? '' : row('West', 400)}${extra}</section>
          ${mode === 'outside-copy' ? row('East', 999) : ''}<button id="apply-transfer">Apply</button>
          <script>
          let delayRead = ${mode === 'late-record'};
          const control = document.querySelector('[data-role="warehouse-total"]');
          const measure = control.getBoundingClientRect.bind(control);
          window.totalSnapshots = 0;
          control.getBoundingClientRect = () => {
            window.totalSnapshots++;
            if (${mode === 'late-correction'} && window.totalSnapshots === 2) {
              document.querySelectorAll('[data-warehouse="East"]')[1].textContent = '600'; delayRead = true;
            }
            if (delayRead) { delayRead = false; const end = performance.now() + 250; while (performance.now() < end) {} }
            return measure();
          };
          document.querySelector('button').onclick = () => {
            delayRead = ${mode === 'late-expect'};
            for (const node of document.querySelectorAll('#admin-panel [data-role="warehouse-total"]')) {
              const delta = ${criterion === '201a'} ? ${mode === 'unauthorized-effect' ? 10 : 0} : ${mode === 'refused' ? 0 : mode === 'reverse' ? -10 : 10};
              node.textContent = Number(node.textContent) + (node.dataset.warehouse === 'East' ? -delta : delta);
            }
            if (${mode === 'wrong-copy'}) document.querySelectorAll('[data-warehouse="East"]')[1].textContent = '600';
          };</script>`);
        const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
          const scope = options?.scope;
          const root = scope ? page.locator(stableElementSelector(scope.testid)).filter({ hasText: scope.contains }).first() : page;
          const locator = root.locator(stableElementSelector(id)).filter({ visible: true });
          return (options?.contains ? locator.filter({ hasText: options.contains }) : locator).first();
        } };
        const service = { defaultWithin: 150, recorded, expand: (value: string) => value, testId: stableElementSelector,
          sleep: (ms: number) => new Promise(resolve => setTimeout(resolve, Math.min(ms, 10))) };
        const capabilities = { actors: { get: () => actor }, 'browser-interaction': service, 'browser-observation': service };
        for (const [index, step] of steps.entries()) {
          if (index === boundary) await page.locator('#apply-transfer').click();
          result = await executeAction(ACTION_REGISTRY, step.do, { ...step, within: 150 }, { capabilities });
          actions.push(result);
          if (result.status !== 'passed') break;
        }
        const snapshots = await page.evaluate(() => (window as unknown as { totalSnapshots: number }).totalSnapshots);
        evidence.push({ criterion, mode, expected, snapshots, recorded: [...recorded], actions });
        assert.equal(result?.status, expected, JSON.stringify(evidence.at(-1)));
        if (expected === 'failed') {
          assert.equal(result?.code, 'application_failure');
          assert.equal(result?.action.id, ['wrong-copy', 'reverse', 'refused', 'unauthorized-effect'].includes(mode!)
            ? 'expectNumber' : 'recordNumber', 'fail at the missing or wrong owned total, not row cardinality');
          if (mode === 'late-correction') {
            assert.equal(snapshots, 2, 'one timely mismatch must precede the delayed correct snapshot');
            assert.equal(result?.finding?.kind, 'number-mismatch');
          }
        }
        if (expected === 'inconclusive') {
          assert.equal(result?.finding?.kind, 'observation-window-missed');
          assert.equal(result?.action.id, mode === 'late-record' ? 'recordNumber' : 'expectNumber');
        }
      } finally { await page.close(); }
    });
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE) {
      const file = process.env.STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({
        rerun: 'STACK_BENCH_WAREHOUSE_VIEW_EVIDENCE=<file> node --test --test-name-pattern="warehouse totals validate" dist/tests/item-navigation.integration.js',
        fixture: 'Compiled 2a/201a numeric browser slices. A button models the confirmed effect; native database and authorization checks are outside this fixture.',
        deadlinePolicy: 'A first read that overruns is inconclusive. Expiry preserves the last timely mismatch, even if a later snapshot matches; late evidence never establishes a pass.',
        sourceSha256: Object.fromEntries([scenarioPath, 'dist/tests/item-navigation.integration.js',
          'dist/src/actions/browser-action-executors.js'].map(path => [path,
          createHash('sha256').update(readFileSync(join(STACK_BENCH_ROOT, path))).digest('hex')])), evidence,
      }, null, 2) + '\n');
    }
  }
});

test('catalog ranking distinguishes exact name hooks from surrounding decoration', async () => {
  // Failure cases: scoped fallback requires the delivered contract and cannot
  // conceal hidden results, wrong visible primary contents, or a stale observer.
  const names = ['Air Purifier', 'Bluetooth Speaker', 'Coffee Grinder', 'Desk Lamp',
    'Espresso Machine', 'Gaming Mouse', 'Headphones', 'Induction Cooktop', 'Keyboard', 'Laptop Stand'];
  const scenario = readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-catalog-ranking.json'), 'utf8');
  const compiled = compileScenarioDefinition(JSON.parse(scenario)).features[0]!;
  const feature = { ...compiled, setup: compiled.setup.map(step => ({ ...step, within: 250 })),
    criteria: compiled.criteria.map(criterion => ({ ...criterion,
      steps: criterion.steps.map(step => ({ ...step, within: 250 })),
    })) };
  const liveSource = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-core.json'), 'utf8'));
  const live = compileScenarioDefinition(liveSource).features.find(candidate => candidate.id === 2)!;
  // Exercise the registered two-client ranking steps without unrelated auth/stock setup.
  const liveFeature = { ...live, actors: ['buyer', 'visitor'], setup: [],
    criteria: live.criteria.filter(criterion => criterion.id === '2c').map(criterion => ({
      ...criterion, steps: criterion.steps.filter(step => step.do !== 'expectAgreement')
        .map(step => ({ ...step, within: 500 })),
    })) };
  let mode = 'plain';
  let purchased = false;
  const evidence: unknown[] = [];
  const server = createServer((request, response) => {
    if (request.url === '/buy') {
      purchased = true; response.writeHead(204); response.end(); return;
    }
    if (request.url === '/ranking') {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(purchased ? [names[2], ...names.filter(name => name !== names[2])] : names));
      return;
    }
    const rows = [...names];
    if (mode.endsWith('wrong-order')) [rows[0], rows[1]] = [rows[1]!, rows[0]!];
    if (mode.endsWith('missing')) rows.pop();
    if (mode.endsWith('duplicate')) rows[9] = rows[0]!;
    if (mode === 'changed-name') rows[0] = 'Air Purifier Pro';
    const cards = (values: string[]) => values.map(name =>
      `<article data-role="item-card"><button><span data-role="item-name">${name}${mode === 'inside-icon' ? ' ↗' : ''}</span>${mode === 'outside-icon' ? '<span aria-hidden="true"> ↗</span>' : ''}</button><button data-role="buy-now" onclick="fetch('/buy')">Buy</button></article>`).join('');
    const fallback = mode.startsWith('fallback') || mode === 'no-gate' || mode === 'hidden-only'
      || mode === 'live' || mode === 'stale';
    let html = mode === 'fallback-absent' ? ''
      : `<section id="item-list" ${fallback ? 'hidden' : ''}>${cards(mode === 'primary-wrong' ? [...rows].reverse() : rows)}</section>`;
    if (fallback || mode === 'primary-wrong') html += `<section id="search-results" ${mode === 'hidden-only' ? 'hidden' : ''}>${cards(rows)}</section>`;
    if (mode === 'nested') html = `<section id="search-results">${html}</section>`;
    if (mode === 'live' || mode === 'stale') html += `<script>
      const update = ${cards.toString()};
      const mode = ${JSON.stringify(mode)};
      let previous = ${JSON.stringify(JSON.stringify(names))};
      if (mode === 'live') setInterval(async () => {
        const rows = await (await fetch('/ranking')).json(), next = JSON.stringify(rows);
        if (next !== previous) document.querySelector('#search-results').innerHTML = update(rows);
        previous = next;
      }, 30);
    </script>`;
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(html);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (mode of ['plain', 'outside-icon', 'inside-icon', 'wrong-order', 'missing', 'duplicate', 'changed-name',
      'fallback', 'fallback-absent', 'no-gate', 'nested', 'hidden-only', 'primary-wrong',
      'fallback-wrong-order', 'fallback-missing', 'fallback-duplicate', 'live', 'stale']) {
      purchased = false;
      const expected = ['plain', 'outside-icon', 'fallback', 'fallback-absent', 'nested', 'live'].includes(mode) ? 'passed' : 'failed';
      const gated = !['plain', 'outside-icon', 'inside-icon', 'wrong-order', 'missing', 'duplicate', 'changed-name', 'no-gate'].includes(mode);
      const grade = await gradeFeature(browser, mode === 'live' || mode === 'stale' ? liveFeature : feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'catalog-name-contract', roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 250,
          contractIds: gated ? ['ecommerce.progression.faceted-search-hooks'] : [] });
      evidence.push({ mode, expected, gated, grade });
      if (mode === 'stale') assert.equal(purchased, true, 'the stale observer case must reach the purchase');
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
        browserVersion: browser.version(), names, scenario: JSON.parse(scenario), liveSource, evidence,
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

test('filter setup supports input, change, and explicit Apply without accepting wrong results', async t => {
  const files = ['progression-faceted-filters.json', 'progression-faceted-pagination.json', 'progression-search-ordering.json'];
  const catalog = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/composition/fixtures/operations.json'), 'utf8')).items;
  const evidence: unknown[] = [];
  const browser = await chromium.launch({ headless: true });
  try {
    for (const file of files) for (const mode of ['input', 'change', 'apply', 'wrong-filter', 'wrong-sort', 'empty']) {
      // Sorting does not belong to 401a; its negative controls concern filtering.
      if (file === files[0] && mode === 'wrong-sort') continue;
      if (file === files[1] && mode === 'wrong-filter') continue; // All seeded prices already exceed 1.
      await t.test(`${file}/${mode}`, async () => {
        const feature = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
          'tracks/ecommerce/scenarios', file), 'utf8'))).features[0]!;
        // 402b's purchase and database receipt are separate prerequisites. Seed
        // their proven result here, then run its actual post-purchase browser slice.
        const ranked = file === files[2];
        const setup = ranked ? feature.setup.slice(feature.setup.findIndex(step => step.do === 'reload')) : feature.setup;
        const page = await browser.newPage();
        const writes: unknown[] = [], actions: unknown[] = [];
        let setupFilters = '';
        let result: Awaited<ReturnType<typeof executeAction>> | undefined;
        try {
          await page.route('http://filters.test/', route => route.fulfill({ contentType: 'text/html', body: `
            <input id="search-input"><select id="category-filter"><option value="">All</option><option>Home</option></select>
            <input id="minimum-price"><input id="maximum-price"><input type="checkbox" id="in-stock-filter">
            ${mode === 'apply' ? '<button id="filter-apply">Apply</button>' : ''}
            <output id="filter-state"></output><section id="item-list"><section id="search-results"></section></section>
            <button id="search-next-page">Next</button><button id="search-previous-page">Previous</button>
            <script>
              const items = ${JSON.stringify(catalog)}, mode = ${JSON.stringify(mode)}, ranked = ${ranked};
              const $ = id => document.getElementById(id);
              let filters = { category: '', min: '', max: '', stock: false }, page = 0, search = '';
              function render() {
                const active = search || mode !== 'wrong-filter' && (filters.category || filters.min || filters.max || filters.stock);
                let rows = items.filter(item => (!search || item.name.toLowerCase().includes(search.toLowerCase()))
                  && (mode === 'wrong-filter' || ((!filters.category || item.category === filters.category)
                  && (!filters.min || Number(item.price) >= Number(filters.min))
                  && (!filters.max || Number(item.price) <= Number(filters.max))
                  && (!filters.stock || item.name !== 'Coffee Grinder' && item.stock.East + item.stock.West > 0))));
                rows.sort((a,b) => !active && ranked ? Number(b.name === 'Headphones') - Number(a.name === 'Headphones') || a.name.localeCompare(b.name) : a.name.localeCompare(b.name));
                if (mode === 'wrong-sort') rows.reverse();
                if (mode === 'empty') rows = [];
                $('filter-state').textContent = [filters.category, filters.min, filters.max].join('|');
                $('search-results').innerHTML = rows.slice(page*10, page*10+10).map(item => '<article data-role="item-card"><span data-role="item-name">'+item.name+'</span></article>').join('');
              }
              function apply() {
                filters = { category: $('category-filter').value, min: $('minimum-price').value,
                  max: $('maximum-price').value, stock: $('in-stock-filter').checked }; page = 0; render();
              }
              for (const id of ['category-filter','minimum-price','maximum-price','in-stock-filter'])
                $(id).addEventListener(mode === 'change' ? 'change' : 'input', () => { if (mode !== 'apply') apply(); });
              if ($('filter-apply')) $('filter-apply').onclick = apply;
              $('search-input').onkeydown = event => { if (event.key === 'Enter') { search = $('search-input').value; page = 0; render(); } };
              $('search-next-page').onclick = () => { page++; render(); };
              $('search-previous-page').onclick = () => { page--; render(); };
              render();
            </script>` }));
          await page.goto('http://filters.test/');
          const actor = { page, loc: (id: string, options?: { contains?: string; scope?: { testid: string; contains?: string } }) => {
            const scope = options?.scope;
            const root = scope ? page.locator(stableElementSelector(scope.testid)).filter({ hasText: scope.contains }).first() : page;
            const locator = root.locator(stableElementSelector(id)).filter({ visible: true });
            return (options?.contains ? locator.filter({ hasText: options.contains }) : locator).first();
          } };
          const sleep = async (ms: number) => { if (ms) await new Promise(resolve => setTimeout(resolve, Math.min(ms, 10))); };
          const service = { defaultWithin: 250, expand: (value: string) => value, testId: stableElementSelector, sleep };
          const capabilities = { actors: { get: () => actor }, 'browser-interaction': service,
            'browser-observation': service, clock: { sleep },
            'database-write': { setStock: async (input: unknown) => { writes.push(input); return input; } } };
          for (const [index, step] of [...setup, ...feature.criteria[0]!.steps].entries()) {
            result = await executeAction(ACTION_REGISTRY, step.do,
              { ...step, ...(step.testid ? { within: 250 } : {}), ...(step.do === 'reload' ? { settleMs: 0 } : {}) }, { capabilities });
            actions.push(result);
            if (index === setup.length - 1) setupFilters = await page.locator('#filter-state').innerText();
            if (result.status !== 'passed') break;
          }
          const expected = ['input', 'change', 'apply'].includes(mode) ? 'passed' : 'failed';
          evidence.push({ file, mode, expected, setupFilters, actions, writes });
          assert.equal(result?.status, expected, JSON.stringify(evidence.at(-1)));
          if (expected === 'passed') {
            assert.equal(setupFilters, file === files[0] ? 'Home|50|200' : '|1|', 'the edit batch must be committed before its assertions');
            assert.equal(await page.locator('#filter-state').innerText(), file === files[0] ? 'Home|50|200' : ranked ? '||' : '|1|');
          } else assert.equal(result?.code, 'application_failure');
          assert.equal(writes.length, file === files[0] ? 2 : 0);
        } finally { await page.close(); }
      });
    }
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE) {
      const file = process.env.STACK_BENCH_ITEM_NAVIGATION_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({
        rerun: 'STACK_BENCH_ITEM_NAVIGATION_EVIDENCE=<file> node --test --test-name-pattern="filter setup supports" dist/tests/item-navigation.integration.js',
        fixture: 'Operations catalog; Coffee Grinder has zero stock for 401a; one prior Headphones purchase for 402b. Browser slices only.',
        sourceSha256: Object.fromEntries([...files.map(name => `tracks/ecommerce/scenarios/${name}`),
          'dist/tests/item-navigation.integration.js', 'dist/src/actions/browser-action-executors.js'].map(path => [path,
          createHash('sha256').update(readFileSync(join(STACK_BENCH_ROOT, path))).digest('hex')])), evidence,
      }, null, 2) + '\n');
    }
  }
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
