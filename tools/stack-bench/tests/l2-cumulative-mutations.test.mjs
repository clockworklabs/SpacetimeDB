import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { mutationEdits, mutationTargetKeys,
  validateMutationDefinitions } from '../mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource } from '../reference-fixtures.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes', 'l2-standard-1.4.0.json'), {
  trackRoot: TRACK,
});
const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.id.startsWith('ecommerce-l2-server-actions-'))
  .map(fixture => [fixture.backend, fixture]));
const cases = ['mongodb', 'postgres', 'spacetime'];
const loadJson = path => JSON.parse(readFileSync(path, 'utf8'));
const activeManifest = backend => loadJson(join(ROOT, 'grader', 'mutations',
  `${backend}-ecom-l2-cumulative-1.4.0.json`));
const activeL1Manifest = backend => loadJson(join(ROOT, 'grader', 'mutations',
  `${backend}-ecom-l1-modular-2.3.0.json`));
const stableByLegacyTarget = new Map(release.checkCatalog.map(check => [
  `${check.source}:${check.featureId}:${check.criterionId}`,
  check.stableKey,
]));
const l2CoverageByBackend = new Map();

for (const backend of cases) {
  test(`${backend} cumulative L2 1.4 mutations are production-bound`, t => {
    const root = mkdtempSync(join(tmpdir(), `stack-bench-l2-mutations-${backend}-`));
    try {
      const manifest = activeManifest(backend);
      const qualifiedL1 = activeL1Manifest(backend);
      assert.deepEqual({ schemaVersion: manifest.schemaVersion, status: manifest.status,
        backend: manifest.backend, track: manifest.track, level: manifest.level }, {
        schemaVersion: 1, status: 'active', backend, track: 'ecommerce', level: 2,
      });
      assert.equal(Object.hasOwn(manifest, 'scenario'), false,
        'combined manifests must not rely on a fallback scenario');
      assert.equal(Object.hasOwn(manifest, 'kind'), false);
      assert.equal(Object.hasOwn(manifest, 'state'), false);
      assert.equal(manifest.mutations.length, 35);
      assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size, 35);
      assert.equal(manifest.mutations.every(mutation => Object.hasOwn(mutation, 'scenario')), true);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        requireScenario: true,
      }).issues, []);

      const inheritedIds = qualifiedL1.mutations.map(mutation => mutation.id);
      assert.deepEqual(manifest.mutations.slice(0, inheritedIds.length)
        .map(mutation => mutation.id), inheritedIds,
      'the qualified L1 mutation set must remain first and complete');

      const fixture = fixtures.get(backend);
      assert(fixture, `missing ${backend} derived fixture`);
      const app = join(root, 'app');
      const prepared = prepareReferenceFixtureSource(fixture, app);
      assert.equal(manifest.fixtureSha256, prepared.sha256);

      const l2StableKeys = new Set();
      for (const mutation of manifest.mutations) {
        const scenario = mutation.scenario.replaceAll('\\', '/')
          .replace(/^tracks\/ecommerce\//, '');
        for (const key of mutationTargetKeys(mutation)) {
          const separator = key.indexOf(':');
          const stableKey = stableByLegacyTarget.get(
            `${scenario}:${key.slice(0, separator)}:${key.slice(separator + 1)}`);
          assert(stableKey, `${mutation.id} target ${scenario}:${key} is absent from L2 1.4`);
          if (scenario.startsWith('scenarios/02-')) l2StableKeys.add(stableKey);
        }

        const path = join(app, ...mutation.file.split('/'));
        let mutated = readFileSync(path, 'utf8');
        for (const edit of mutationEdits(mutation)) {
          assert.equal(mutated.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match its exact derived source once`);
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

      assert.equal(l2StableKeys.size, 27);
      l2CoverageByBackend.set(backend, [...l2StableKeys].sort());
      t.diagnostic(`${backend}: 35 mutations, ${l2StableKeys.size} distinct L2 keys`);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}

test('all backends cover the same distinct L2 keys', t => {
  const expected = l2CoverageByBackend.get('mongodb');
  assert(expected);
  assert.deepEqual(l2CoverageByBackend.get('postgres'), expected);
  assert.deepEqual(l2CoverageByBackend.get('spacetime'), expected);
  const l2Keys = release.checkCatalog.filter(check => check.source.startsWith('scenarios/02-'))
    .map(check => check.stableKey);
  assert.deepEqual(l2Keys.filter(stableKey => !expected.includes(stableKey)), [
    'ecommerce.inventory-operations.stock-conservation.202d',
  ], 'only the deliberately separate direct transfer/purchase race may lack a mutation');
  t.diagnostic(`shared L2 coverage (${expected.length}): ${expected.join(', ')}`);
});
