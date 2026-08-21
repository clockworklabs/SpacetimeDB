import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { prepareReferenceSource } from '../src/references/reference-agent.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = 'ecommerce.l1-modular@2.4.0';
const MANIFEST = join(ROOT, 'grader', 'mutations',
  'mongodb-ecom-l1-modular-2.4.0.json');
const FIXTURE_SHA256 = '76810d72211fc0182aa31b663ffc153a82ff1918cd34902187873a4b53a4ebf2';
const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.4.0.json'), { trackRoot: TRACK });

const releaseKeyByTarget = new Map(release.checkCatalog.map(check => [
  `${check.source}:${check.featureId}:${check.criterionId}`,
  check,
]));

function resolvedTargets(mutation) {
  const source = mutationScenario(manifest, mutation).replaceAll('\\', '/')
    .replace(/^tracks\/ecommerce\//, '');
  return mutationTargetKeys(mutation).map(target => {
    const separator = target.indexOf(':');
    return releaseKeyByTarget.get(
      `${source}:${target.slice(0, separator)}:${target.slice(separator + 1)}`,
    );
  });
}

test('MongoDB L1 2.4 candidate covers every scored stable check exactly', t => {
  assert.deepEqual({
    schemaVersion: manifest.schemaVersion,
    status: manifest.status,
    fixtureSha256: manifest.fixtureSha256,
    backend: manifest.backend,
    track: manifest.track,
    level: manifest.level,
  }, {
    schemaVersion: 1,
    status: 'active',
    fixtureSha256: FIXTURE_SHA256,
    backend: 'mongodb',
    track: 'ecommerce',
    level: 1,
  });
  assert.equal(Object.hasOwn(manifest, 'scenario'), false,
    'every candidate mutation must declare its exact scenario');
  assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size,
    manifest.mutations.length, 'mutation ids must be unique');
  assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
    requireScenario: true,
  }).issues, []);

  const covered = new Set();
  for (const mutation of manifest.mutations) {
    const targets = resolvedTargets(mutation);
    assert(targets.every(Boolean), `${mutation.id} must resolve every target in ${RECIPE}`);
    for (const check of targets) {
      assert(check.points > 0, `${mutation.id} must not target zero-point ${check.stableKey}`);
      covered.add(check.stableKey);
    }
  }

  const scored = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();
  const missing = scored.filter(stableKey => !covered.has(stableKey));
  const extra = [...covered].filter(stableKey => !scored.includes(stableKey)).sort();
  t.diagnostic(`MongoDB L1 2.4 mutation coverage: ${covered.size}/${scored.length}`);
  t.diagnostic(`missing scored keys: ${missing.length ? missing.join(', ') : 'none'}`);
  assert.deepEqual(missing, []);
  assert.deepEqual(extra, []);
  assert.equal(scored.length, 46);
  assert.equal(covered.size, 46);
});

test('server-side L1 guarantees use server-side defects', () => {
  const serverGuarantees = new Set([
    'ecommerce.spec.concurrency-safety.stock-limit.3d',
    'ecommerce.feature.cart-checkout.cart.4d',
    'ecommerce.spec.access-control.purchase-session.101a',
    'ecommerce.spec.access-control.purchase-attribution.102a',
    'ecommerce.spec.access-control.admin-write.103a',
    'ecommerce.spec.transactional-integrity.server-price.104a',
    'ecommerce.spec.access-control.order-ownership.106a',
    'ecommerce.spec.transactional-integrity.books-balance.107a',
    'ecommerce.spec.transactional-integrity.books-balance.107b',
    'ecommerce.spec.access-control.review-eligibility.108a',
    'ecommerce.spec.access-control.review-eligibility.108b',
    'ecommerce.spec.access-control.cart-boundary.109a',
    'ecommerce.spec.access-control.cart-boundary.109b',
    'ecommerce.spec.concurrency-safety.last-unit.201a',
    'ecommerce.spec.concurrency-safety.last-unit.201b',
    'ecommerce.spec.concurrency-safety.last-unit.201c',
    'ecommerce.spec.concurrency-safety.restock-race.202a',
    'ecommerce.spec.concurrency-safety.duplicate-checkout.203a',
    'ecommerce.spec.concurrency-safety.duplicate-checkout.203b',
  ]);
  for (const mutation of manifest.mutations) {
    const keys = resolvedTargets(mutation).map(check => check.stableKey);
    if (keys.some(key => serverGuarantees.has(key))) {
      assert.equal(mutation.file, 'server/src/index.ts',
        `${mutation.id} must exercise its server guarantee below the UI`);
    }
  }
});

test('cart-boundary mutants exercise the named server actions', () => {
  const scenario = JSON.parse(readFileSync(join(TRACK, 'scenarios',
    '01-cart-boundary-2.4.0.json'), 'utf8'));
  const cart = scenario.features.find(feature => feature.id === 109);
  const ownership = cart.criteria.find(criterion => criterion.id === '109a');
  const quantity = cart.criteria.find(criterion => criterion.id === '109b');
  assert(ownership.steps.some(step => step.do === 'callAction'
    && step.action === 'cart-add'));
  assert(quantity.steps.some(step => step.do === 'callAction'
    && step.action === 'cart-set-quantity'
    && step.namedAction.method === 'PATCH'
    && step.input.attribute === 'data-cart-input'));

  const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
  assert.equal(mutations.get('cart-add-is-scoped-by-item-instead-of-owner').file,
    'server/src/index.ts');
  assert.equal(mutations.get('negative-cart-quantity-is-accepted').file,
    'server/src/index.ts');
  assert.equal(mutationEdits(mutations.get('negative-cart-quantity-is-accepted')).length, 2,
    'the negative-quantity mutant must bypass both the route check and schema validation');

  const reconnect = mutationEdits(mutations.get('reconnect-hydration-loses-account-state'));
  assert.equal(reconnect.length, 4);
  assert(reconnect.some(edit => edit.replace.includes('ignoreCartAfterNetworkRestore')));
  assert(reconnect.some(edit => edit.replace.includes('token && !ignoreCartCatchup')));
  assert(reconnect.some(edit => edit.replace.includes('if (!ignoreCartCatchup) setCart(data)')),
    'the reconnect defect must preserve initial hydration and reject all post-restore catch-up');

  const sharedCart = mutationEdits(mutations.get('shared-cart-live-events-ignored'));
  assert.equal(sharedCart.length, 1);
  assert(sharedCart[0].replace.includes('current.items.length === 0 ? current : data'),
    'the shared-cart defect must ignore the empty second session without breaking checkout cleanup');
});

test('MongoDB L1 2.4 candidate is bound to the exact derived fixture and compiles', t => {
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-mongodb-l1-2.4-mutations-'));
  try {
    const app = join(work, 'app');
    const prepared = prepareReferenceSource({
      backend: 'mongodb',
      track: 'ecommerce',
      level: 1,
      recipe: RECIPE,
      app,
    });
    assert.equal(prepared.fixture.id, 'ecommerce-l1-action-inputs-2.4-mongodb');
    assert.equal(prepared.sourceSha256, FIXTURE_SHA256);
    assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
    const client = readFileSync(join(app, 'client', 'src', 'App.tsx'), 'utf8');
    assert.equal(client.split('data-cart-input=').length - 1, 1,
      'the exact prepared fixture must expose one named cart quantity input');

    for (const mutation of manifest.mutations) {
      const path = join(app, ...mutation.file.split('/'));
      let mutated = readFileSync(path, 'utf8');
      for (const edit of mutationEdits(mutation)) {
        assert.equal(mutated.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact derived source once`);
        mutated = mutated.replace(edit.find, edit.replace);
      }
      const transpiled = ts.transpileModule(mutated, {
        compilerOptions: {
          jsx: ts.JsxEmit.ReactJSX,
          module: ts.ModuleKind.ESNext,
          target: ts.ScriptTarget.ES2022,
        },
        fileName: mutation.file,
        reportDiagnostics: true,
      });
      assert.deepEqual((transpiled.diagnostics ?? [])
        .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
        .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), [],
      `${mutation.id} must leave ${mutation.file} syntactically valid`);
    }
    t.diagnostic(`${manifest.mutations.length} MongoDB mutations bind and transpile`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
