import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { mutationFileEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { prepareReferenceFixtureSource } from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const FIXTURE_SHA256 = '2b678936abe7faa9670a914a425369f478956a2f5ef3326725d32a3b7aacebdb';
const MANIFEST = join(ROOT, 'grader', 'mutations', 'candidates',
  'mongodb-ecom-l2-cumulative-1.5.0.json');
const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l2-standard-1.5.0.json'), { trackRoot: TRACK });
const l1Checks = release.checkCatalog.filter(check => check.points > 0
  && !check.source.startsWith('scenarios/02-'));
const l2Checks = release.checkCatalog.filter(check => check.points > 0
  && check.source.startsWith('scenarios/02-'));
const expectedChecks = [...l1Checks, ...l2Checks];
const candidateFixture = {
  id: 'ecommerce-l2-cumulative-1.5-mongodb-candidate',
  source: {
    basePath: 'reference-apps/ecommerce/l2/mongodb',
    baseSha256: '6aafa2ec888b4049a87f488469f16118cd480517bfa876e57538f751744d1c7b',
    patchPath: 'reference-apps/patches/ecommerce-l2-cumulative-1.5/mongodb.json',
  },
};
const checkByTarget = new Map(expectedChecks.map(check => [
  `${check.source}:${check.featureId}:${check.criterionId}`,
  check,
]));

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

test('MongoDB L2 1.5 candidate covers the exact cumulative scored catalog', t => {
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
    backend: 'mongodb',
    track: 'ecommerce',
    level: 2,
  });
  assert.equal(Object.hasOwn(manifest, 'scenario'), false);
  assert.equal(manifest.mutations.length, 71);
  assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size, 71);
  assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
    requireScenario: true,
  }).issues, []);

  const coveredL1 = new Set();
  const coveredL2 = new Set();
  for (const mutation of manifest.mutations) {
    const targets = resolveTargets(mutation);
    assert(targets.every(Boolean), `${mutation.id} must resolve every exact scenario target`);
    for (const check of targets) {
      (check.source.startsWith('scenarios/02-') ? coveredL2 : coveredL1)
        .add(check.stableKey);
    }
  }

  const missingL1 = l1Checks.map(check => check.stableKey)
    .filter(stableKey => !coveredL1.has(stableKey));
  const missingL2 = l2Checks.map(check => check.stableKey)
    .filter(stableKey => !coveredL2.has(stableKey));
  assert.deepEqual(missingL1, []);
  assert.deepEqual(missingL2, []);
  assert.equal(coveredL1.size, 46);
  assert.equal(coveredL2.size, 28);
  assert.equal(new Set([...coveredL1, ...coveredL2]).size, 74);
  assert.equal(expectedChecks.length, 74);
  t.diagnostic(`inherited L1 coverage: ${coveredL1.size}/${l1Checks.length}; missing: ${missingL1.join(', ')}`);
  t.diagnostic(`L2-only coverage: ${coveredL2.size}/${l2Checks.length}; missing: ${missingL2.join(', ')}`);
  t.diagnostic('cumulative coverage: 74/74 scored stable keys');
});

test('the exact L2 1.5 source supplies the inherited cart action hook', () => {
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-mongodb-l2-1.5-source-gap-'));
  try {
    const app = join(work, 'app');
    const prepared = prepareReferenceFixtureSource(candidateFixture, app);
    assert.equal(prepared.sha256, FIXTURE_SHA256);
    const client = readFileSync(join(app, 'client', 'src', 'App.tsx'), 'utf8');
    assert.equal(client.split('data-cart-input=').length - 1, 1,
      'the recipe-specific L2 overlay must carry the L1 2.4 named cart input');
    assert(manifest.mutations.some(mutation => mutationTargetKeys(mutation)
      .includes('109:109b')), '109b must be inherited once its action input is present');
    assert(manifest.mutations.some(mutation => mutationTargetKeys(mutation)
      .includes('202:202d')),
    '202d must have a deterministic source-level transfer/purchase synchronization defect');
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});

test('the 202d defect deterministically loses a committed purchase without timing tricks', () => {
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'transfer-overwrites-concurrent-purchase-with-stale-stock');
  assert(mutation);
  assert.equal(mutation.scenario,
    'tracks/ecommerce/scenarios/02-server-actions-1.1.0.json');
  assert.deepEqual(mutationTargetKeys(mutation), ['202:202d']);
  assert.equal(mutation.file, 'server/src/index.ts');
  assert.equal(mutation.edits.length, 4);

  const replacement = mutation.edits.map(edit => edit.replace).join('\n');
  assert.match(replacement, /await mutationTransferCaptured/);
  assert.match(replacement, /mutationBuyCommittedResolve\(\)/);
  assert.match(replacement, /await mutationBuyCommitted/);
  assert.match(replacement, /\$set: \{ quantity: staleSource\.quantity - qty \}/);
  assert.doesNotMatch(replacement, /setTimeout|setInterval|\bsleep\b/i);
});

test('all 71 mutations bind and transpile against the exact L2 1.5 source', t => {
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-mongodb-l2-1.5-mutations-'));
  try {
    const app = join(work, 'app');
    const prepared = prepareReferenceFixtureSource(candidateFixture, app);
    assert.equal(prepared.sha256, manifest.fixtureSha256);

    for (const mutation of manifest.mutations) {
      const sources = new Map();
      for (const edit of mutationFileEdits(mutation)) {
        const path = join(app, ...edit.file.split('/'));
        let mutated = sources.get(edit.file) ?? readFileSync(path, 'utf8');
        assert.equal(mutated.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact L2 source once`);
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
    t.diagnostic(`${manifest.mutations.length} MongoDB cumulative mutations bind and transpile`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
