import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import test from 'node:test';

import { readGradeArtifactPayload } from '../src/evidence/artifacts.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { resolveRecipeSelection } from '../src/composition/recipe-selection.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compiledEntrypoint, STACK_BENCH_ROOT } from '../src/package-root.js';

const GRADER = compiledEntrypoint('grader', 'grade.js');

const first = <Value>(values: readonly Value[]): Value => {
  const value = values[0];
  assert(value);
  return value;
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const actionIds = (actions: ReadonlyArray<{ evidence: unknown }>): string[] => actions.map(entry => {
  if (!record(entry.evidence) || !record(entry.evidence.action)
    || typeof entry.evidence.action.id !== 'string') {
    throw new Error('grade action evidence has no action id');
  }
  return entry.evidence.action.id;
});

function startBlankApp(html: string = '<!doctype html><html><body></body></html>', catalogHtml?: string) {
  const source = `
    import { createServer } from 'node:http';
    const server = createServer((request, response) => {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end(request.url === '/catalog' ? ${JSON.stringify(catalogHtml ?? html)} : ${JSON.stringify(html)});
    });
    server.listen(0, '127.0.0.1', () => console.log(server.address().port));
    process.on('SIGTERM', () => server.close(() => process.exit(0)));
  `;
  const child = spawn(process.execPath, ['--input-type=module', '--eval', source], {
    stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
  });
  const port = new Promise<number>((resolve, reject) => {
    const deadline = setTimeout(() => reject(new Error('blank app did not start')), 10_000);
    child.once('error', reject);
    child.stdout.once('data', data => {
      clearTimeout(deadline);
      resolve(Number(data.toString().trim()));
    });
  });
  return { child, port };
}

function run(file: string, argv: readonly string[]): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    execFile(process.execPath, [file, ...argv], {
      encoding: 'utf8', timeout: 60_000, maxBuffer: 8 * 1024 * 1024,
    }, (error, stdout, stderr) => {
      if (error) reject(new Error(`${error.message}\n${stdout}\n${stderr}`));
      else resolve({ stdout, stderr });
    });
  });
}

test('the live grader executes and reports exactly one selected stable check', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selected-grade-'));
  const app = join(root, 'app');
  const out = join(root, 'grade.json');
  mkdirSync(app, { recursive: true });
  const server = startBlankApp();
  try {
    const port = await server.port;
    const track = loadTrack('ecommerce');
    const binding = resolveRecipeRelease(track, 1);
    const check = binding.release.checkCatalog.find(candidate => candidate.criterionId === '2a');
    assert(check);
    assert(check.source);
    const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
    await run(GRADER, [
      '--url', `http://127.0.0.1:${port}`,
      '--level', '1', '--track', 'ecommerce', '--backend', 'postgres', '--app', app,
      '--spec', join(track.dir, check.source), '--out', out,
      '--expected-recipe-sha256', binding.release.contentSha256,
      '--selected-check', check.stableKey, '--selection-sha256', selection.sha256,
    ]);
    const report = readGradeArtifactPayload(out);
    assert(report.selection);
    assert.equal(report.selection.sha256, selection.sha256);
    assert.deepEqual(report.selection.checks.map(item => item.stableKey), [check.stableKey]);
    assert.equal(report.features.length, 1);
    const feature = first(report.features);
    const criterion = first(feature.criteria);
    assert.deepEqual(feature.criteria.map(item => item.stableKey), [check.stableKey]);
    assert.equal(feature.setupEvidence.status, 'passed');
    assert.equal(criterion.evidence.status, 'failed');
    assert.equal(criterion.evidence.phase, 'assertion');
    assert(criterion.evidence.actions.length > 0, 'the selected check must execute');
    assert.equal(report.max, check.points);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('catalog values are checked on the catalog page, including through navigation', async t => {
  // Failure cases: a landing page must not hide a working catalog from the
  // grader; navigation must not excuse a missing item, wrong price, or dead link.
  const parent = join(STACK_BENCH_ROOT, 'results', 'diagnostics');
  mkdirSync(parent, { recursive: true });
  const root = mkdtempSync(join(parent, 'catalog-navigation-'));
  t.diagnostic(`Evidence: ${root}; repeat: node --test --test-name-pattern="catalog values are checked" dist/tests/selected-grade.integration.js`);
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, { id: 'ecommerce.progression-catalog' });
  const check = binding.release.checkCatalog.find(c => c.stableKey === 'ecommerce.feature.catalog.catalog-values.2a');
  assert(check?.source);
  const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
  for (const fixture of [
    { name: 'initial-catalog', linked: false, item: 'Air Purifier', price: 189, opens: true, expected: 'passed' },
    { name: 'linked-catalog', linked: true, item: 'Air Purifier', price: 189, opens: true, expected: 'passed' },
    { name: 'wrong-price', linked: true, item: 'Air Purifier', price: 1.89, opens: true, expected: 'failed' },
    { name: 'missing-item', linked: true, item: 'Desk Lamp', price: 189, opens: true, expected: 'failed' },
    { name: 'dead-link', linked: true, item: 'Air Purifier', price: 189, opens: false, expected: 'failed' },
  ]) await t.test(fixture.name, async () => {
    const app = join(root, fixture.name);
    mkdirSync(app);
    const catalog = `<!doctype html><section id="item-list"><article data-role="item-card">
        <span data-role="item-name">${fixture.item}</span><span data-role="item-price">${fixture.price}</span>
        <span data-role="item-stock">100</span></article></section>`;
    const html = fixture.linked ? `<!doctype html><h1>Storefront</h1><a id="catalog-link" href="${fixture.opens ? '/catalog' : '#'}">Shop</a>` : catalog;
    writeFileSync(join(app, 'index.html'), html);
    writeFileSync(join(app, 'catalog.html'), catalog);
    writeFileSync(join(app, 'expected.json'), JSON.stringify(fixture, null, 2));
    const server = startBlankApp(html, catalog);
    const out = join(app, 'grade.json');
    try {
      const port = await server.port;
      const execution = await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1', '--track', 'ecommerce',
        '--backend', 'postgres', '--app', app, '--spec', join(track.dir, check.source!), '--out', out,
        '--recipe', binding.release.id, '--selected-check', check.stableKey,
        '--expected-recipe-sha256', binding.release.contentSha256, '--selection-sha256', selection.sha256]);
      writeFileSync(join(app, 'stdout.log'), execution.stdout);
      writeFileSync(join(app, 'stderr.log'), execution.stderr);
      const report = readGradeArtifactPayload(out);
      assert.equal(first(first(report.features).criteria).evidence.status, fixture.expected);
      assert.equal(report.max, 1);
    } finally {
      server.child.kill('SIGTERM');
    }
  });
});

test('purchase observers reach a separate catalog before the write and stay there', async t => {
  // A working linked catalog must pass. A dead link must fail. A stale observer
  // must fail even if returning to the catalog would show the updated stock.
  const root = mkdtempSync(join(STACK_BENCH_ROOT, 'results', 'diagnostics', 'purchase-navigation-'));
  t.diagnostic(`Evidence: ${root}; repeat: node --test --test-name-pattern="purchase observers" dist/tests/selected-grade.integration.js`);
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2, { id: 'ecommerce.progression-catalog' });
  const check = binding.release.checkCatalog.find(c => c.stableKey === 'ecommerce.spec.live-state.purchase-stock.3b');
  assert(check?.source);
  const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
  for (const fixture of [
    { name: 'root', linked: false, opens: true, live: true, expected: 'passed' },
    { name: 'linked', linked: true, opens: true, live: true, expected: 'passed' },
    { name: 'dead-link', linked: true, opens: false, live: true, expected: 'failed' },
    { name: 'stale-observer', linked: true, opens: true, live: false, expected: 'failed' },
    { name: 'late-first-read', linked: true, opens: true, live: true, delayGuest: true, expected: 'failed' },
  ]) await t.test(fixture.name, async () => {
    const app = join(root, fixture.name);
    mkdirSync(app);
    let stock = 100, navigationsAfterPurchase = 0, purchases = 0;
    const pendingReads: Array<() => void> = [];
    const delayGuest = 'delayGuest' in fixture;
    const catalog = `<section id="item-list"><article data-role="item-card">
      <span data-role="item-name">Espresso Machine</span><span data-role="item-stock">100</span>
      <button id="buy-now" onclick="fetch('/stock',{method:'POST'})">Buy</button></article></section>`;
    const html = (inCatalog: boolean) => `<!doctype html>
      <input id="signup-username"><input id="signup-password"><button id="signup-submit">Sign up</button>
      <div id="current-user" hidden></div>
      <a id="catalog-link" href="${fixture.opens ? '/catalog' : '#'}">Shop</a>
      ${inCatalog ? catalog : '<h1>Home</h1>'}
      <script>
        const current = document.querySelector('#current-user');
        if(sessionStorage.user){current.hidden=false;current.textContent=sessionStorage.user;}
        document.querySelector('#signup-submit').onclick=()=>{
          sessionStorage.user=document.querySelector('#signup-username').value;
          current.textContent=sessionStorage.user;current.hidden=false;
        };
        const lateGuest=${delayGuest && inCatalog}&&!sessionStorage.user;
        if(lateGuest)document.querySelector('[data-role="item-stock"]').textContent='';
        async function update(){const response=await fetch('/stock'+(lateGuest?'?wait=1':''));const value=await response.text();
          const element=document.querySelector('[data-role="item-stock"]');if(element)element.textContent=value;}
        update();if(${fixture.live}&&!lateGuest)setInterval(update,100);
      </script>`;
    const server = createServer((request, response) => {
      if (request.url?.startsWith('/stock')) {
        if (request.method === 'POST') { stock--; purchases++; pendingReads.splice(0).forEach(read => read()); }
        response.writeHead(200, { 'content-type': 'text/plain', 'cache-control': 'no-store' });
        if (request.url === '/stock?wait=1' && !purchases) {
          pendingReads.push(() => response.end(String(stock)));
          return;
        }
        response.end(String(stock));
        return;
      }
      if (purchases) navigationsAfterPurchase++;
      response.writeHead(200, { 'content-type': 'text/html' });
      response.end(html(!fixture.linked || request.url === '/catalog'));
    });
    writeFileSync(join(app, 'index.html'), html(!fixture.linked));
    writeFileSync(join(app, 'catalog.html'), html(true));
    writeFileSync(join(app, 'expected.json'), JSON.stringify(fixture, null, 2));
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
      const address = server.address();
      assert(address && typeof address !== 'string');
      const out = join(app, 'grade.json');
      const execution = await run(GRADER, ['--url', `http://127.0.0.1:${address.port}`, '--level', '2',
        '--track', 'ecommerce', '--backend', 'postgres', '--app', app,
        '--spec', join(track.dir, check.source!), '--out', out, '--recipe', binding.release.id,
        '--selected-check', check.stableKey, '--expected-recipe-sha256', binding.release.contentSha256,
        '--selection-sha256', selection.sha256]);
      writeFileSync(join(app, 'stdout.log'), execution.stdout);
      writeFileSync(join(app, 'stderr.log'), execution.stderr);
      writeFileSync(join(app, 'effects.json'), JSON.stringify({ purchases, stock, navigationsAfterPurchase }));
      const report = readGradeArtifactPayload(out);
      assert.equal(first(first(report.features).criteria).evidence.status, fixture.expected);
      if (fixture.opens) assert.equal(purchases, delayGuest ? 0 : 1, 'observe initial data before the purchase');
      assert.equal(navigationsAfterPurchase, 0, 'observers must not reload to see the write');
    } finally {
      server.closeAllConnections();
      await new Promise<void>(resolve => server.close(() => resolve()));
    }
  });
});

test('cart checks reopen catalog after authentication and reload without refreshing live observers', async t => {
  // Authentication and reload can both return a state-based app to its home
  // screen. Persistence must still be measured; stale live views must fail.
  const root = mkdtempSync(join(STACK_BENCH_ROOT, 'results', 'diagnostics', 'cart-navigation-'));
  t.diagnostic(`Evidence: ${root}; repeat: node --test --test-name-pattern="cart checks reopen" dist/tests/selected-grade.integration.js`);
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2, { id: 'ecommerce.progression-catalog' });
  for (const fixture of [{ id: '4b', live: true }, { id: '4c', live: true }, { id: '4c', live: false }]) {
    await t.test(`${fixture.id}-${fixture.live ? 'live' : 'stale'}`, async () => {
      const check = binding.release.checkCatalog.find(c => c.criterionId === fixture.id && c.source === 'scenarios/01-cart.json');
      assert(check?.source);
      const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
      const app = join(root, `${fixture.id}-${fixture.live}`);
      mkdirSync(app);
      let item = '', writes = 0;
      const html = `<!doctype html><h1>Home</h1>
        <input id="signup-username"><input id="signup-password"><button id="signup-submit">Sign up</button>
        <input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button>
        <span id="current-user" hidden></span><button id="catalog-link">Catalog</button>
        <section id="catalog" hidden><div id="item-list">
          ${['Laptop Stand', 'Induction Cooktop'].map(name => `<article data-role="item-card">${name}
            <button data-role="add-to-cart" onclick="fetch('/cart',{method:'POST',body:'${name}'})">Add</button></article>`).join('')}
        </div><button id="cart-toggle">Cart</button><div id="cart" hidden><span id="cart-total">1</span><div id="lines"></div></div></section>
        <script>
          const catalog=document.querySelector('#catalog'),current=document.querySelector('#current-user');
          if(sessionStorage.user){current.textContent=sessionStorage.user;current.hidden=false;}
          for(const action of ['signup','signin'])document.querySelector('#'+action+'-submit').onclick=()=>{
            sessionStorage.user=document.querySelector('#'+action+'-username').value;
            current.textContent=sessionStorage.user;current.hidden=false;catalog.hidden=true;
          };
          document.querySelector('#catalog-link').onclick=()=>{catalog.hidden=false;};
          async function update(){const response=await fetch('/cart');const item=await response.text();
            document.querySelector('#lines').innerHTML=item?'<div data-role="cart-item">'+item+'</div>':'';}
          document.querySelector('#cart-toggle').onclick=()=>{document.querySelector('#cart').hidden=false;update();};
          if(${fixture.live})setInterval(update,100);
        </script>`;
      const server = createServer((request, response) => {
        if (request.url === '/cart') {
          response.setHeader('content-type', 'text/plain');
          response.setHeader('cache-control', 'no-store');
          if (request.method === 'POST') {
            let body = ''; request.on('data', chunk => { body += chunk; });
            request.on('end', () => { item = body; writes++; response.end(item); });
          } else response.end(item);
        } else { response.setHeader('content-type', 'text/html'); response.end(html); }
      });
      writeFileSync(join(app, 'index.html'), html);
      writeFileSync(join(app, 'expected.json'), JSON.stringify(fixture));
      await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
      try {
        const address = server.address(); assert(address && typeof address !== 'string');
        const out = join(app, 'grade.json');
        const execution = await run(GRADER, ['--url', `http://127.0.0.1:${address.port}`, '--level', '2', '--track', 'ecommerce',
          '--backend', 'postgres', '--app', app, '--spec', join(track.dir, check.source), '--out', out,
          '--recipe', binding.release.id, '--selected-check', check.stableKey,
          '--expected-recipe-sha256', binding.release.contentSha256, '--selection-sha256', selection.sha256]);
        writeFileSync(join(app, 'stdout.log'), execution.stdout);
        writeFileSync(join(app, 'stderr.log'), execution.stderr);
        writeFileSync(join(app, 'effects.json'), JSON.stringify({ item, writes }));
        const report = readGradeArtifactPayload(out);
        assert.equal(first(first(report.features).criteria).evidence.status, fixture.live ? 'passed' : 'failed');
        assert.equal(writes, 1, 'the cart observation must follow an actual add');
      } finally {
        server.closeAllConnections();
        await new Promise<void>(resolve => server.close(() => resolve()));
      }
    });
  }
});

test('setup can wait for app readiness without relaxing scored checks', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-ready-grade-'));
  const out = join(root, 'grade.json');
  const spec = join(root, 'scenario.json');
  writeFileSync(spec, JSON.stringify({
    schemaVersion: 1,
    level: 1,
    features: [{
      id: 1,
      name: 'delayed app readiness',
      actors: ['a'],
      setup: [{ do: 'signUp', actor: 'a', name: 'Alice' }],
      criteria: [
        { id: '1a', desc: 'setup completed', points: 1,
          steps: [{ do: 'expect', actor: 'a', testid: 'current-user' }] },
        { id: '1b', desc: 'scored checks keep the normal deadline', points: 1,
          steps: [
            { do: 'click', actor: 'a', testid: 'slow-action' },
            { do: 'expect', actor: 'a', testid: 'slow-result' },
          ] },
      ],
    }],
  }));
  const server = startBlankApp(`<!doctype html><html><body>
    <div id="app"></div>
    <script>
      setTimeout(() => {
        document.querySelector('#app').innerHTML = \`
          <input data-role="signup-username"><input data-role="signup-password">
          <button data-role="signup-submit">Sign up</button>
          <div data-role="current-user" hidden></div>
          <button data-role="slow-action">Start</button>
        \`;
        document.querySelector('[data-role="signup-submit"]').onclick = () => {
          const current = document.querySelector('[data-role="current-user"]');
          current.textContent = document.querySelector('[data-role="signup-username"]').value;
          current.hidden = false;
        };
        document.querySelector('[data-role="slow-action"]').onclick = () => {
          setTimeout(() => {
            const result = document.createElement('div');
            result.dataset.testid = 'slow-result';
            document.body.append(result);
          }, 6000);
        };
      }, 6000);
    </script>
  </body></html>`);
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1',
      '--spec', spec, '--out', out]);
    const report = readGradeArtifactPayload(out);
    const feature = first(report.features);
    assert.equal(feature.setupEvidence.status, 'passed');
    assert.deepEqual(actionIds(feature.setupEvidence.actions), ['signUp']);
    assert.equal(report.total, 1);
    assert.equal(report.max, 2);
    assert.equal(first(feature.criteria).evidence.status, 'passed');
    assert.deepEqual(actionIds(first(feature.criteria).evidence.actions), ['expect']);
    const failedCriterion = feature.criteria[1];
    assert(failedCriterion);
    assert.equal(failedCriterion.evidence.status, 'failed');
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('an inconclusive check keeps the recipe denominator fixed', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-fixed-denominator-'));
  const app = join(root, 'app');
  const out = join(root, 'grade.json');
  const spec = join(root, 'scenario.json');
  mkdirSync(app, { recursive: true });
  writeFileSync(spec, JSON.stringify({
    schemaVersion: 1,
    level: 1,
    features: [{
      id: 1,
      name: 'unsupported action',
      actors: ['a'],
      setup: [],
      criteria: [{ id: '1a', desc: 'the declared point remains in the contract', points: 3,
        steps: [{ do: 'callAction', actor: 'a', action: 'not-declared',
          input: { testid: 'action-input', attribute: 'data-input' } }] }],
    }],
  }));
  const server = startBlankApp('<!doctype html><div data-role="action-input" data-input="{}"></div>');
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1',
      '--backend', 'postgres', '--app', app, '--spec', spec, '--out', out]);
    const report = readGradeArtifactPayload(out);
    assert.equal(report.total, 0);
    assert.equal(report.max, 3);
    const feature = first(report.features);
    assert.equal(feature.max, 3);
    assert.equal(first(feature.criteria).evidence.status, 'inconclusive');
    assert.equal(first(report.inconclusive).points, 3);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('standalone zero-point diagnostics reach execution without a recipe binding', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-standalone-diagnostic-'));
  const spec = join(root, 'scenario.json');
  const out = join(root, 'grade.json');
  writeFileSync(spec, JSON.stringify({ schemaVersion: 1, track: 'ecommerce', level: 2,
    features: [{ id: 9000, name: 'Standalone diagnostic', actors: ['a'], setup: [],
      criteria: [{ id: '9000a', category: 'production', points: 0,
        desc: 'local entry point control',
        steps: [{ do: 'expect', actor: 'a', testid: 'diagnostic-ready', within: 1000 }] }],
    }],
  }));
  const server = startBlankApp('<div data-role="diagnostic-ready">ready</div>');
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '2', '--spec', spec, '--out', out]);
    const report = readGradeArtifactPayload(out);
    assert.equal(report.recipeRelease, null);
    assert.equal(report.selection, null);
    assert.equal(report.max, 0);
    assert.equal(report.total, 0);
    assert.equal(report.features.length, 1);
    const criterion = first(first(report.features).criteria);
    assert.equal(criterion.evidence.status, 'passed');
    assert.equal(criterion.evidence.phase, 'assertion');
    assert.deepEqual(actionIds(criterion.evidence.actions), ['expect']);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});
