import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { extname, join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { loadTrack } from '../dist/src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../dist/src/composition/recipe-release.mjs';
import { mutationFileEdits } from '../dist/src/evidence/mutation-analysis.js';
import { rebaseMutationManifest } from '../dist/src/evidence/mutation-rebase.js';
import { compileFeatureCatalogInput,
  compileProgressionDefinitionFile } from '../dist/src/progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../dist/src/progression/progression-recipe-selection.js';
import { loadReferenceRegistry } from '../dist/src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const definition = compileProgressionDefinitionFile(
  join(TRACK, 'progression', 'ecommerce-1.0.0.json'), { trackRoot: TRACK });
const binding = resolveRecipeRelease(loadTrack('ecommerce'), 3,
  'ecommerce.progression-catalog@1.0.0');
const selection = resolveProgressionRecipeLevelSelection(binding,
  compileFeatureCatalogInput(definition), 3, { cumulative: true });
const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.id.startsWith('ecommerce-progression-'))
  .map(fixture => [fixture.backend, fixture]));

function syntaxErrors(source, file) {
  if (!['.ts', '.tsx'].includes(extname(file))) return [];
  return (ts.transpileModule(source, {
    compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022 },
    fileName: file,
    reportDiagnostics: true,
  }).diagnostics ?? []).filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
    .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
}

for (const backend of ['mongodb', 'postgres', 'spacetime']) {
  test(`${backend} progression mutations cover every selected L1-L3 check`, () => {
    const fixture = fixtures.get(backend);
    assert(fixture, `missing ${backend} progression fixture`);
    const manifest = JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations',
      `${backend}-ecom-progression-1.0.0.json`), 'utf8'));
    const result = rebaseMutationManifest(manifest, {
      release: binding.release,
      selectedCheckKeys: selection.grader.checkKeys,
      app: join(ROOT, fixture.targetPath),
      fixtureSha256: fixture.imported.sourceSha256,
    });

    assert.equal(result.coverage.selected.length, 112);
    assert.deepEqual(result.coverage.missing, []);
    assert.deepEqual(result.blocked, []);
    assert.deepEqual(result.excluded, []);
    assert(result.manifest.mutations.every(mutation => mutation.targets.every(target =>
      binding.release.checkCatalog.find(check => check.stableKey === target)?.source
        === mutation.scenario.replace(`tracks/${binding.release.track}/`, ''))));

    for (const mutation of manifest.mutations) {
      const editsByFile = Map.groupBy(mutationFileEdits(mutation), edit => edit.file);
      for (const [file, edits] of editsByFile) {
        let source = readFileSync(join(ROOT, fixture.targetPath, ...file.split('/')), 'utf8');
        for (const edit of edits) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match once in ${file}`);
          source = source.replace(edit.find, edit.replace);
        }
        assert.deepEqual(syntaxErrors(source, file), [],
          `${mutation.id} must remain syntactically valid`);
      }
    }
  });
}

test('SpacetimeDB reservation and catalog mutants affect only their owned checks', () => {
  const fixture = fixtures.get('spacetime');
  const manifest = JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations',
    'spacetime-ecom-progression-1.0.0.json'), 'utf8'));
  const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));

  const renewal = mutations.get('renewed-reservation-expires-too-soon');
  assert.deepEqual(renewal.targets, ['ecommerce.l3.reservations.reservations.308a']);
  const renewalEdit = mutationFileEdits(renewal)[0];
  const renewalPath = join(ROOT, fixture.targetPath, ...renewalEdit.file.split('/'));
  const renewalSource = readFileSync(renewalPath, 'utf8');
  assert.equal(renewalSource.split(renewalEdit.find).length - 1, 1);
  const renewed = renewalSource.replace(renewalEdit.find, renewalEdit.replace);
  assert.notEqual(renewed, renewalSource);
  assert.match(renewed, /const expiresMicros = nowMicros\(ctx\) \+ 90n \* SECOND/,
    'initial reservations must keep their 90-second window');
  assert.match(renewed, /for \(const renewed of findReservations/,
    'only the replacement path should shorten the renewed reservation');
  assert.deepEqual(syntaxErrors(renewed, renewalEdit.file), []);

  const catalog = mutations.get('catalog-product-is-not-published');
  assert.deepEqual(catalog.targets,
    ['ecommerce.progression.catalog-management.catalog-management.622a']);
  const catalogEdit = mutationFileEdits(catalog)[0];
  const catalogPath = join(ROOT, fixture.targetPath, ...catalogEdit.file.split('/'));
  const catalogSource = readFileSync(catalogPath, 'utf8');
  assert.equal(catalogSource.split(catalogEdit.find).length - 1, 1);
  const unpublished = catalogSource.replace(catalogEdit.find, catalogEdit.replace);
  assert.notEqual(unpublished, catalogSource);
  assert.match(unpublished, /variants\.map\(variant =>/,
    'the product variants must remain public for their separate check');
  assert.deepEqual(syntaxErrors(unpublished, catalogEdit.file), []);
});

test('catalog and duplicate-checkout mutants keep independent check ownership', () => {
  const catalogCases = [
    { backend: 'mongodb', nameId: 'catalog-product-name-is-not-published',
      variantsId: 'catalog-variants-are-discarded' },
    { backend: 'postgres', nameId: 'progression-catalog-product-name-is-not-published',
      variantsId: 'progression-catalog-variants-are-discarded' },
  ];
  for (const item of catalogCases) {
    const fixture = fixtures.get(item.backend);
    const manifest = JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations',
      `${item.backend}-ecom-progression-1.0.0.json`), 'utf8'));
    const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
    const name = mutations.get(item.nameId);
    const variants = mutations.get(item.variantsId);
    assert.deepEqual(name.targets,
      ['ecommerce.progression.catalog-management.catalog-management.622a']);
    assert.deepEqual(variants.targets,
      ['ecommerce.progression.catalog-management.catalog-management.622b']);

    for (const mutation of [name, variants]) {
      const edit = mutationFileEdits(mutation)[0];
      const file = join(ROOT, fixture.targetPath, ...edit.file.split('/'));
      const source = readFileSync(file, 'utf8');
      assert.equal(source.split(edit.find).length - 1, 1,
        `${mutation.id} anchor must match once`);
      assert.deepEqual(syntaxErrors(source.replace(edit.find, edit.replace), edit.file), []);
    }
    assert.match(mutationFileEdits(name)[0].replace, /Travel Mug/);
    assert.doesNotMatch(mutationFileEdits(variants)[0].replace, /Unavailable product/);
  }

  const cartCases = [
    { backend: 'postgres', incrementId: 'progression-concurrent-cart-line-does-not-increment',
      checkoutId: 'progression-concurrent-checkout-leaves-cart-lines' },
    { backend: 'spacetime', incrementId: 'existing-cart-line-does-not-increment',
      checkoutId: 'checkout-does-not-empty-cart' },
  ];
  for (const item of cartCases) {
    const manifest = JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations',
      `${item.backend}-ecom-progression-1.0.0.json`), 'utf8'));
    const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
    assert.deepEqual(mutations.get(item.incrementId).targets,
      ['ecommerce.spec.concurrency-safety.duplicate-checkout.203a']);
    assert.deepEqual(mutations.get(item.checkoutId).targets,
      ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b']);
  }
});
