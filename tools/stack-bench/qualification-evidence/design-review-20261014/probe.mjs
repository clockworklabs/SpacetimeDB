import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { chromium } from 'playwright';
import { gradeFeature } from '../../.loop-test/critical-review-20261014/grader/grade.js';
import { compileScenarioDefinition } from '../../.loop-test/critical-review-20261014/src/composition/definition-compiler.js';
import { resolveFeatureCatalog } from '../../.loop-test/critical-review-20261014/src/progression/feature-catalog-selection.js';
import { loadTrack } from '../../.loop-test/critical-review-20261014/src/composition/tracks.js';
import { compileDependencyPolicyInput, dependencyRuntimeDefinition } from '../../.loop-test/critical-review-20261014/src/progression/progression-definition.js';
import { progressionEngine } from '../../.loop-test/critical-review-20261014/src/progression/progression-engine.js';
import { dependencyCompletionBreakdown } from '../../.loop-test/critical-review-20261014/src/progression/dependency-score.js';

// Expected sensitivity cases, set before execution:
// catalog: the unchanged value check should pass both all 13 items and only Air Purifier;
// changing that sampled item's price should fail.
// privacy: the unchanged 631b assertion should pass a server leak filtered by the UI;
// rendering the leak should fail. Setup is replaced by a prepared notification fixture.
// graph: inject exactly one persistent failure, pass every other selected check,
// then count the actual terminal outcomes. These are synthetic grades, not model results.
const out = dirname(fileURLToPath(import.meta.url));
const root = resolve(out, '../..');
const read = path => JSON.parse(readFileSync(resolve(root, path), 'utf8'));
const report = { kind: 'bounded-review-probe', createdAt: new Date().toISOString(),
  rerun: 'node local-notes/critical-review-20261014/probe.mjs',
  build: 'node node_modules/typescript/bin/tsc -p tsconfig.build.json --outDir .loop-test/critical-review-20261014',
  platform: process.platform, node: process.version, identities: {}, graph: [], browser: [],
  limits: ['Local Chromium observation checks only; no Docker, database, model calls, or qualification claim.',
    '631b uses its unchanged criterion steps with prepared preconditions instead of its database/restock setup.',
    'Synthetic graph outcomes measure policy effects, not actual coding-agent success.',
    'The first fixture attempt failed: wrong account suffix and empty zero-height notification panel. Original script and result retained as fixture-failure files.'] };
for (const path of ['tracks/ecommerce/scenarios/01-catalog-values.json',
  'tracks/ecommerce/scenarios/progression-stock-alerts.json', 'tracks/ecommerce/progression/ecommerce.json',
  'grader/grade.ts', 'src/actions/browser-action-executors.ts', 'src/progression/dependency-mode.ts'])
  report.identities[path] = createHash('sha256').update(readFileSync(resolve(root, path))).digest('hex');

const catalog = resolveFeatureCatalog('progression/ecommerce.json', loadTrack('ecommerce'));
for (const depth of [3, 6]) for (const failure of [null, 'ecommerce.spec.state-durability.session-reload.1e',
  'ecommerce.feature.catalog.catalog-values.2a']) {
  const policy = compileDependencyPolicyInput({ selection: 'batch', budget: { total: 0 } }, catalog,
    { selectedLevels: Array.from({ length: depth }, (_, i) => i + 1), workSelection: 'progressive' });
  const definition = dependencyRuntimeDefinition(catalog, policy);
  let state = progressionEngine.initialize(definition);
  const prompts = [];
  for (let i = 0; i < 30 && state.phase !== 'terminal'; i++) {
    const selected = progressionEngine.gradingSelection(state);
    prompts.push(progressionEngine.promptSelection(state));
    state = progressionEngine.recordResult(state, { attemptId: `synthetic-${i}`, outcome: 'conclusive',
      nodes: selected.nodeIds.map(id => ({ id, checks: selected.checks.filter(c => c.nodeId === id)
        .map(c => ({ id: c.id, outcome: c.id === failure ? 'fail' : 'pass' })) })) });
  }
  assert.equal(state.phase, 'terminal');
  const score = progressionEngine.score(state);
  report.graph.push({ depth, failure, featureCompletion: dependencyCompletionBreakdown(state).featureCompletion,
    completion: score.completion, nodeStates: state.nodes, blockedNodes: Object.entries(state.nodes).filter(([, n]) => n.status === 'blocked').map(([id]) => id),
    promptedNodeIds: prompts.map(p => p.nodeIds), weightedPercentage: score.uniqueChecks.percentage,
    questlineAveragePercentage: score.questlineAveragePercentage });
}

let mode = 'catalog-all';
const requests = [];
const items = read('tracks/ecommerce/composition/fixtures/operations.json').items;
const server = createServer((req, res) => {
  const parsed = new URL(req.url, 'http://probe.test');
  if (parsed.pathname === '/notifications') {
    const user = parsed.searchParams.get('user');
    const owner = 'stock-subscriberreviewf631';
    const records = mode === 'privacy-correct' && user !== owner ? []
      : [{ owner, item: 'Air Purifier', privateMarker: 'private-alert-review-631b' }];
    requests.push({ mode, user, records });
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(records)); return;
  }
  res.writeHead(200, { 'Content-Type': 'text/html' });
  if (mode.startsWith('catalog')) {
    const visible = mode === 'catalog-all' ? items : items.slice(0, 1);
    res.end('<!doctype html><section id="item-list">' + visible.map(item =>
      `<article data-role="item-card"><span data-role="item-name">${item.name}</span>` +
      `<span data-role="item-price">${mode === 'catalog-wrong-price' ? 999 : item.price}</span>` +
      `<span data-role="item-stock">${Object.values(item.stock).reduce((a,b) => a+b,0)}</span></article>`).join('') + '</section>');
    return;
  }
  res.end(`<!doctype html><main></main><script>
    const mode=${JSON.stringify(mode)}, main=document.querySelector('main'), user=sessionStorage.getItem('user');
    if (!user) {
      main.innerHTML='<form><input id="signin-username"><input id="signin-password" type="password"><button id="signin-submit">Sign in</button></form>';
      main.querySelector('form').onsubmit=e=>{e.preventDefault();sessionStorage.setItem('user',document.querySelector('#signin-username').value);location.reload();};
    } else {
      main.innerHTML='<span id="current-user"></span><section id="notifications-panel" aria-busy="true">Notifications</section>';
      document.querySelector('#current-user').textContent=user;
      fetch('/notifications?user='+encodeURIComponent(user)).then(r=>r.json()).then(rows=>{
        const panel=document.querySelector('#notifications-panel');
        for (const row of rows) if(mode==='privacy-visible-leak'||row.owner===user) {
          const item=document.createElement('div');item.dataset.role='notification-item';
          const delivery=document.createElement('span');delivery.dataset.role='stock-alert-delivery';delivery.textContent=row.item;
          item.append(delivery);panel.append(item);
        }
        panel.setAttribute('aria-busy','false');
      });
    }
  </script>`);
});
let browser;
try {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true });
  report.browserVersion = browser.version();
  for (const testMode of ['catalog-all', 'catalog-single-item', 'catalog-wrong-price',
    'privacy-correct', 'privacy-filtered-leak', 'privacy-visible-leak']) {
    mode = testMode; requests.length = 0;
    const source = mode.startsWith('catalog') ? 'tracks/ecommerce/scenarios/01-catalog-values.json'
      : 'tracks/ecommerce/scenarios/progression-stock-alerts.json';
    const definition = read(source);
    if (mode.startsWith('privacy')) {
      definition.features[0].setup = [];
      definition.features[0].actors = ['subscriber', 'other'];
      definition.features[0].criteria = definition.features[0].criteria.filter(c => c.id === '631b');
    }
    const feature = compileScenarioDefinition(definition).features[0];
    const grade = await gradeFeature(browser, feature,
      { url, level: definition.level, headed: false, selectedCheckKeys: [], nullControl: false },
      { runId: 'review', roomName: name => name, url, actions: [], spacetime: null,
        backend: 'postgres', nullControl: false });
    const expected = ['catalog-wrong-price', 'privacy-visible-leak'].includes(mode) ? 'failed' : 'passed';
    const row = { mode, expected, source, setupReplaced: mode.startsWith('privacy'),
      renderedCatalogItems: mode === 'catalog-all' ? items.length : mode.startsWith('catalog') ? 1 : null,
      requests: [...requests], grade };
    report.browser.push(row);
    writeFileSync(resolve(out, 'probe-evidence.json'), JSON.stringify(report, null, 2));
    assert.equal(grade.criteria[0].evidence.status, expected, JSON.stringify(row));
    if (mode === 'privacy-filtered-leak') assert(requests.some(r => r.user.startsWith('stock-other') && r.records.length === 1));
    console.log(JSON.stringify({ mode, expected, actual: grade.criteria[0].evidence.status }));
  }
} finally {
  if (browser) await browser.close();
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  report.cleanup = { browserClosed: Boolean(browser), serverClosed: !server.listening };
  writeFileSync(resolve(out, 'probe-evidence.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify(report.graph.map(({ promptedNodeIds, nodeStates, blockedNodes, ...row }) => ({ ...row, blockedNodes: blockedNodes.length })), null, 2));
