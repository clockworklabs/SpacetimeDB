import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { mutationEdits, mutationTargetKeys, resolveMutationFile,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { prepareReferenceSource } from '../src/references/reference-agent.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = 'ecommerce.l1-modular@2.4.0';
const FIXTURE_SHA256 = 'd5cb5af9db96b3ae4ff2b0d928dec4394ac6f88dfdce83201a0d8508b69902e5';
const MANIFEST = join(ROOT, 'grader', 'mutations',
  'spacetime-ecom-l1-modular-2.4.0.json');
const EXPECTED_UNMUTATED = [];
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

function targetStableKey(release, mutation, target) {
  const source = mutation.scenario.replaceAll('\\', '/')
    .replace(/^tracks\/ecommerce\//, '');
  return release.checkCatalog.find(check => check.source === source
    && check.featureId === Number(target.split(':', 1)[0])
    && check.criterionId === target.slice(target.indexOf(':') + 1))?.stableKey;
}

test('Spacetime L1 2.4 candidate mutations bind every honest defect to exact source', t => {
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-l1-24-spacetime-mutations-'));
  try {
    const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
      'l1-modular-2.4.0.json'), { trackRoot: TRACK });
    const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
    const app = join(work, 'app');
    const prepared = prepareReferenceSource({ backend: 'spacetime', track: 'ecommerce',
      level: 1, recipe: RECIPE, app });

    assert.deepEqual({ schemaVersion: manifest.schemaVersion, status: manifest.status,
      backend: manifest.backend, track: manifest.track, level: manifest.level }, {
      schemaVersion: 1, status: 'active', backend: 'spacetime', track: 'ecommerce', level: 1,
    });
    assert.equal(Object.hasOwn(manifest, 'scenario'), false,
      'each mutation must own its exact scenario instead of using a fallback');
    assert.match(manifest.note, /database\/app-server independence is structural in SpacetimeDB/);
    assert.match(manifest.note, /validates only the scored stale-view oracle/);
    assert.match(manifest.note, /not evidence about storage durability/,
      'the candidate must explicitly avoid claiming a backend durability mutation');
    assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size,
      manifest.mutations.length, 'mutation ids must be unique');
    assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
      requireScenario: true,
    }).issues, []);

    assert.equal(prepared.fixture.id, 'ecommerce-l1-action-inputs-2.4-spacetime');
    assert.equal(prepared.sourceSha256, FIXTURE_SHA256);
    assert.equal(manifest.fixtureSha256, prepared.sourceSha256);

    const covered = new Set();
    for (const mutation of manifest.mutations) {
      const targetKeys = mutationTargetKeys(mutation);
      assert(targetKeys.length > 0, `${mutation.id} must declare an exact target`);
      for (const target of targetKeys) {
        const stableKey = targetStableKey(release, mutation, target);
        assert(stableKey, `${mutation.id} target ${mutation.scenario}:${target} is absent from L1 2.4`);
        if (SERVER_GUARANTEES.has(stableKey)) {
          assert.equal(mutation.file, 'backend/spacetimedb/src/index.ts',
            `${mutation.id} must exercise ${stableKey} below the UI`);
        }
        covered.add(stableKey);
      }

      const path = resolveMutationFile(app, mutation.file);
      let mutated = readFileSync(path, 'utf8');
      for (const edit of mutationEdits(mutation)) {
        assert.equal(mutated.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact prepared source once`);
        mutated = mutated.replace(edit.find, edit.replace);
      }
      const transpiled = ts.transpileModule(mutated, {
        compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext,
          target: ts.ScriptTarget.ES2022 },
        fileName: mutation.file,
        reportDiagnostics: true,
      });
      assert.deepEqual((transpiled.diagnostics ?? [])
        .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
        .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), [],
      `${mutation.id} must remain syntactically valid`);
    }

    const scored = release.checkCatalog.filter(check => check.points > 0);
    assert.equal(release.checkCatalog.length, 48);
    assert.equal(scored.length, 46);
    assert.equal(scored.reduce((sum, check) => sum + check.points, 0), 58);
    const missing = scored.map(check => check.stableKey)
      .filter(stableKey => !covered.has(stableKey)).sort();
    assert.deepEqual(missing, EXPECTED_UNMUTATED,
      `exact scored mutation gaps: ${missing.join(', ') || '<none>'}`);
    t.diagnostic(`${manifest.mutations.length} source mutants cover ${scored.length - missing.length}`
      + `/${scored.length} scored checks; missing: ${missing.length ? missing.join(', ') : 'none'}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
