import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { mutationFileEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = 'ecommerce.l2-standard@1.5.0';
const FIXTURE_SHA256 = '7b3d6c936623ed8ff8b4ff3e61204c291b784c2d36c2bc1edbd5bcca47f9c952';
const MANIFEST = join(ROOT, 'grader', 'mutations',
  'postgres-ecom-l2-cumulative-1.5.0.json');
const L1_MANIFEST = join(ROOT, 'grader', 'mutations',
  'postgres-ecom-l1-modular-2.4.0.json');
const PRIOR_L2_MANIFEST = join(ROOT, 'grader', 'mutations',
  'postgres-ecom-l2-cumulative-1.4.0.json');

const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const inheritedL1 = JSON.parse(readFileSync(L1_MANIFEST, 'utf8')).mutations;
const provenL2 = JSON.parse(readFileSync(PRIOR_L2_MANIFEST, 'utf8')).mutations
  .filter(mutation => mutation.scenario?.replaceAll('\\', '/').includes('/scenarios/02-'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l2-standard-1.5.0.json'), { trackRoot: TRACK });
const scored = release.checkCatalog.filter(check => check.points > 0);
const l1Checks = scored.filter(check => !check.source.startsWith('scenarios/02-'));
const l2Checks = scored.filter(check => check.source.startsWith('scenarios/02-'));
const checkByTarget = new Map(scored.map(check => [
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

test('PostgreSQL L2 1.5 is the exact cumulative mutation composition', () => {
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
    backend: 'postgres',
    track: 'ecommerce',
    level: 2,
  });
  assert.equal(Object.hasOwn(manifest, 'scenario'), false);
  assert.equal(manifest.mutations.length, inheritedL1.length + provenL2.length + 1);
  assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size,
    manifest.mutations.length);
  assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
    requireScenario: true,
  }).issues, []);

  assert.deepEqual(manifest.mutations.slice(0, inheritedL1.length), inheritedL1,
    'the complete active L1 2.4 defect set must remain first and exact');
  assert.deepEqual(manifest.mutations.slice(inheritedL1.length, -1), provenL2,
    'the previously proven L2-only defect set must remain exact');
  assert.equal(manifest.mutations.at(-1).id,
    'transfer-overwrites-concurrent-purchase-with-stale-stock');
});

test('PostgreSQL L2 1.5 covers all 74 cumulative scored keys', t => {
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
  assert.equal(scored.length, 74);
  assert.equal(manifest.mutations.filter(mutation => mutationTargetKeys(mutation)
    .includes('202:202d')).length, 1);
  t.diagnostic('inherited L1 coverage: 46/46; L2-only coverage: 28/28');
  t.diagnostic('cumulative mutation coverage: 74/74');
});

test('202d uses source synchronization and a stale PostgreSQL write without sleeps', () => {
  const mutation = manifest.mutations.find(candidate => mutationTargetKeys(candidate)
    .includes('202:202d'));
  assert(mutation);
  assert.equal(mutation.scenario,
    'tracks/ecommerce/scenarios/02-server-actions-1.1.0.json');
  const edits = mutationFileEdits(mutation);
  assert.equal(edits.length, 5);
  const replacement = edits.map(edit => edit.replace).join('\n');
  assert.match(replacement, /mutationTransferCaptured/);
  assert.match(replacement, /mutationBuyCommitted/);
  assert.match(replacement, /SELECT quantity FROM stock/);
  assert.match(replacement, /mutationStaleSourceQuantity - qty/);
  assert.doesNotMatch(replacement, /setTimeout|sleep|delay/i);
  assert(replacement.indexOf('await mutationBuyCommitted')
    < replacement.lastIndexOf('mutationStaleSourceQuantity - qty'));
});

test('the exact PostgreSQL L2 1.5 fixture carries every cumulative action input', () => {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), {
    backend: 'postgres', track: 'ecommerce', level: 2, recipe: RECIPE,
  });
  assert.equal(fixture.id, 'ecommerce-l2-cumulative-1.5-postgres');
  assert.equal(fixture.status, 'active');
  assert.equal(fixture.imported.sourceSha256, FIXTURE_SHA256);

  const work = mkdtempSync(join(tmpdir(), 'stack-bench-postgres-l2-1.5-source-'));
  try {
    const app = join(work, 'app');
    assert.equal(prepareReferenceFixtureSource(fixture, app).sha256, FIXTURE_SHA256);
    const client = readFileSync(join(app, 'client', 'src', 'App.tsx'), 'utf8');
    for (const attribute of [
      'data-buy-input=', 'data-cart-input=', 'data-restock-input=', 'data-ship-input=',
      'data-cancel-input=', 'data-transfer-input=', 'data-price-input=',
    ]) assert.equal(client.split(attribute).length - 1, 1, attribute);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});

test('all PostgreSQL cumulative mutations bind once and remain valid TypeScript', t => {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), {
    backend: 'postgres', track: 'ecommerce', level: 2, recipe: RECIPE,
  });
  const work = mkdtempSync(join(tmpdir(), 'stack-bench-postgres-l2-1.5-mutations-'));
  try {
    const app = join(work, 'app');
    assert.equal(prepareReferenceFixtureSource(fixture, app).sha256, manifest.fixtureSha256);

    for (const mutation of manifest.mutations) {
      const sources = new Map();
      for (const edit of mutationFileEdits(mutation)) {
        const path = join(app, ...edit.file.split('/'));
        let source = sources.get(edit.file) ?? readFileSync(path, 'utf8');
        assert.equal(source.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact L2 source once`);
        source = source.replace(edit.find, edit.replace);
        sources.set(edit.file, source);
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
        `${mutation.id}:${file} must remain syntactically valid`);
      }
    }
    t.diagnostic(`${manifest.mutations.length} PostgreSQL cumulative mutations bind and transpile`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
