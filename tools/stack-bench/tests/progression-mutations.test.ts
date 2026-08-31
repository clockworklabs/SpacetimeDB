import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { extname, join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { mutationFileEdits, mutationTargetKeys, readMutationManifest,
  type LoadedMutationDefinition }
  from '../src/evidence/mutation-analysis.js';
import { compileFeatureCatalogInput,
  compileProgressionDefinitionFile } from '../src/progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import { loadReferenceRegistry } from '../src/references/reference-fixtures.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const definition = compileProgressionDefinitionFile(
  join(TRACK, 'progression', 'ecommerce-2.0.1.json'), { trackRoot: TRACK });
const binding = resolveRecipeRelease(loadTrack('ecommerce'), 3,
  'ecommerce.progression-catalog@2.0.1');
const selection = resolveProgressionRecipeLevelSelection(binding,
  compileFeatureCatalogInput(definition), 3, { cumulative: true });
const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.track === 'ecommerce')
  .map(fixture => [fixture.backend, fixture]));

function mutationManifest(backend: string) {
  const fixture = fixtures.get(backend);
  assert(fixture, `missing ${backend} ecommerce reference`);
  assert.equal(fixture.mutationManifests?.length, 1);
  const manifestPath = fixture.mutationManifests?.[0];
  assert(manifestPath);
  return readMutationManifest(join(ROOT, manifestPath));
}

function syntaxErrors(source: string, file: string): string[] {
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
    assert(fixture, `missing ${backend} ecommerce reference`);
    assert(fixture.targetPath, `${backend} fixture must have a target path`);
    const manifest = mutationManifest(backend);
    const selected = new Set(selection.grader.checkKeys);
    const covered = new Set(manifest.mutations.flatMap(mutation => mutationTargetKeys(mutation)));
    assert.equal(selected.size, 97);
    assert.deepEqual([...selected].filter(check => !covered.has(check)), []);

    for (const mutation of manifest.mutations) {
      const editsByFile = new Map<string, ReturnType<typeof mutationFileEdits>>();
      for (const edit of mutationFileEdits(mutation)) {
        const edits = editsByFile.get(edit.file) ?? [];
        edits.push(edit);
        editsByFile.set(edit.file, edits);
      }
      for (const [file, edits] of editsByFile) {
        let source: string = readFileSync(join(ROOT, fixture.targetPath, ...file.split('/')), 'utf8');
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
  assert(fixture, 'missing spacetime progression fixture');
  assert(fixture.targetPath, 'the spacetime fixture must have a target path');
  const manifest = readMutationManifest(join(ROOT, 'grader', 'mutations',
    'spacetime-ecommerce-2.0.1.json'));
  const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));

  const renewal = mutations.get('renewed-reservation-expires-too-soon');
  assert(renewal, 'the reservation renewal mutation must exist');
  assert.deepEqual(renewal.targets, ['ecommerce.l3.reservations.reservations.308a']);
  const renewalEdit = mutationFileEdits(renewal)[0];
  assert(renewalEdit, 'the reservation renewal mutation must have an edit');
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
  assert(catalog, 'the catalog mutation must exist');
  assert.deepEqual(catalog.targets,
    ['ecommerce.progression.catalog-management.catalog-management.622a']);
  const catalogEdit = mutationFileEdits(catalog)[0];
  assert(catalogEdit, 'the catalog mutation must have an edit');
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
    assert(fixture, `missing ${item.backend} ecommerce reference`);
    assert(fixture.targetPath, `${item.backend} fixture must have a target path`);
    const manifest = mutationManifest(item.backend);
    const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
    const name = mutations.get(item.nameId);
    const variants = mutations.get(item.variantsId);
    assert(name, `missing ${item.nameId}`);
    assert(variants, `missing ${item.variantsId}`);
    assert.deepEqual(name.targets,
      ['ecommerce.progression.catalog-management.catalog-management.622a']);
    assert.deepEqual(variants.targets,
      ['ecommerce.progression.catalog-management.catalog-management.622b']);

    for (const mutation of [name, variants]) {
      const edit = mutationFileEdits(mutation)[0];
      assert(edit, `${mutation.id} must have an edit`);
      const file = join(ROOT, fixture.targetPath, ...edit.file.split('/'));
      const source = readFileSync(file, 'utf8');
      assert.equal(source.split(edit.find).length - 1, 1,
        `${mutation.id} anchor must match once`);
      assert.deepEqual(syntaxErrors(source.replace(edit.find, edit.replace), edit.file), []);
    }
    const nameEdit = mutationFileEdits(name)[0];
    const variantEdit = mutationFileEdits(variants)[0];
    assert(nameEdit, `${name.id} must have an edit`);
    assert(variantEdit, `${variants.id} must have an edit`);
    assert.match(nameEdit.replace, /Travel Mug/);
    assert.doesNotMatch(variantEdit.replace, /Unavailable product/);
  }

  const cartCases = [
    { backend: 'postgres', incrementId: 'progression-concurrent-cart-line-does-not-increment',
      checkoutId: 'progression-concurrent-checkout-leaves-cart-lines' },
    { backend: 'spacetime', incrementId: 'existing-cart-line-does-not-increment',
      checkoutId: 'checkout-does-not-empty-cart' },
  ];
  for (const item of cartCases) {
    const manifest = mutationManifest(item.backend);
    const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
    const increment = requiredMutation(mutations, item.incrementId);
    const checkout = requiredMutation(mutations, item.checkoutId);
    assert.deepEqual(increment.targets,
      ['ecommerce.spec.concurrency-safety.duplicate-checkout.203a']);
    assert.deepEqual(checkout.targets,
      ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b']);
  }
});

function requiredMutation(mutations: Map<string, LoadedMutationDefinition>, id: string): LoadedMutationDefinition {
  const mutation = mutations.get(id);
  if (!mutation) throw new Error(`mutation ${id} is required`);
  return mutation;
}
