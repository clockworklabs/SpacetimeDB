import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { mutationFileEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource, selectReferenceFixture }
  from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = 'ecommerce.l2-standard@1.5.0';
const FIXTURE_SHA256 = 'b0bfc4405e684511874f5a867a5dc84e28b258e46729b7199a1e8aa5e27b61ce';
const MANIFEST = join(ROOT, 'grader', 'mutations', 'candidates',
  'spacetime-ecom-l2-cumulative-1.5.0.json');
const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l2-standard-1.5.0.json'), { trackRoot: TRACK });
const scored = release.checkCatalog.filter(check => check.points > 0);
const checkByTarget = new Map(release.checkCatalog.map(check => [
  `${check.source}:${check.featureId}:${check.criterionId}`,
  check,
]));
const SERVER_GUARANTEES = new Set([
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

function resolveTargets(mutation) {
  const source = mutationScenario(manifest, mutation).replaceAll('\\', '/')
    .replace(/^tracks\/ecommerce\//, '');
  return mutationTargetKeys(mutation).map(target => {
    const separator = target.indexOf(':');
    return checkByTarget.get(
      `${source}:${target.slice(0, separator)}:${target.slice(separator + 1)}`,
    );
  });
}

test('SpacetimeDB L2 1.5 candidate covers every scored cumulative check honestly', t => {
  assert.deepEqual({
    schemaVersion: manifest.schemaVersion,
    status: manifest.status,
    fixtureSha256: manifest.fixtureSha256,
    backend: manifest.backend,
    track: manifest.track,
    level: manifest.level,
  }, {
    schemaVersion: 1,
    status: 'candidate',
    fixtureSha256: FIXTURE_SHA256,
    backend: 'spacetime',
    track: 'ecommerce',
    level: 2,
  });
  assert.equal(Object.hasOwn(manifest, 'scenario'), false);
  assert.equal(manifest.mutations.length, 73);
  assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size, 73);
  assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
    requireScenario: true,
  }).issues, []);

  assert.match(manifest.note, /202d/);
  assert.match(manifest.note, /validates only the exact stock-conservation total assertion/);
  assert.match(manifest.note, /reducer atomic serialization is structural/);
  assert.match(manifest.note, /does not claim to create or detect a backend lost-update interleaving/);

  const covered = new Set();
  for (const mutation of manifest.mutations) {
    const targets = resolveTargets(mutation);
    assert(targets.length > 0, `${mutation.id} must own at least one exact target`);
    assert(targets.every(Boolean), `${mutation.id} must resolve every exact scenario target`);
    for (const check of targets) {
      if (SERVER_GUARANTEES.has(check.stableKey)) {
        assert.equal(mutation.file, 'backend/spacetimedb/src/index.ts',
          `${mutation.id} must exercise ${check.stableKey} below the UI`);
      }
      if (check.points > 0) covered.add(check.stableKey);
    }
  }

  const missing = scored.map(check => check.stableKey)
    .filter(stableKey => !covered.has(stableKey)).sort();
  const unexpected = [...covered]
    .filter(stableKey => !scored.some(check => check.stableKey === stableKey)).sort();
  assert.deepEqual(missing, []);
  assert.deepEqual(unexpected, []);
  assert.equal(scored.length, 74);
  assert.equal(scored.reduce((sum, check) => sum + check.points, 0), 117);
  assert.equal(covered.size, 74);
  assert.equal(scored.filter(check => !check.source.startsWith('scenarios/02-')).length, 46);
  assert.equal(scored.filter(check => check.source.startsWith('scenarios/02-')).length, 28);

  const conservation = manifest.mutations.filter(mutation => resolveTargets(mutation)
    .some(check => check.stableKey
      === 'ecommerce.inventory-operations.stock-conservation.202d'));
  assert.equal(conservation.length, 1);
  assert.equal(conservation[0].id, 'stock-conservation-view-keeps-the-initial-total');
  assert.equal(conservation[0].file, 'client/src/App.tsx');
  assert.match(conservation[0].desc, /validates the exact starting-total-minus-one oracle/);
  assert.match(conservation[0].desc, /does not model a lost-update interleaving/);
  t.diagnostic(`${manifest.mutations.length} mutations cover ${covered.size}/${scored.length} scored keys`);
});

test('all SpacetimeDB L2 1.5 mutations bind and transpile against the exact candidate source', t => {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), {
    backend: 'spacetime', track: 'ecommerce', level: 2, recipe: RECIPE,
  });
  assert.equal(fixture.id, 'ecommerce-l2-cumulative-1.5-spacetime');
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-spacetime-l2-1.5-mutations-'));
  try {
    const app = join(work, 'app');
    const prepared = prepareReferenceFixtureSource(fixture, app);
    assert.equal(prepared.sha256, FIXTURE_SHA256);
    assert.equal(manifest.fixtureSha256, prepared.sha256);

    for (const mutation of manifest.mutations) {
      const sources = new Map();
      for (const edit of mutationFileEdits(mutation)) {
        const path = join(app, ...edit.file.split('/'));
        let mutated = sources.get(edit.file) ?? readFileSync(path, 'utf8');
        assert.equal(mutated.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact prepared source once`);
        mutated = mutated.replace(edit.find, edit.replace);
        sources.set(edit.file, mutated);
      }
      for (const [file, source] of sources) {
        const transpiled = ts.transpileModule(source, {
          compilerOptions: {
            jsx: ts.JsxEmit.ReactJSX,
            module: ts.ModuleKind.ESNext,
            target: ts.ScriptTarget.ES2022,
          },
          fileName: file,
          reportDiagnostics: true,
        });
        assert.deepEqual((transpiled.diagnostics ?? [])
          .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
          .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), [],
        `${mutation.id} must leave ${file} syntactically valid`);
      }
    }
    t.diagnostic(`${manifest.mutations.length} SpacetimeDB cumulative mutations bind and transpile`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
